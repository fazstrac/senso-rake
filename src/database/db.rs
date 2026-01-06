// Database interaction module
use crate::service::{Service, ServiceType};

use duckdb::arrow::record_batch::RecordBatch;
use crossbeam_channel::{unbounded, Sender, Receiver};
use tokio::sync::oneshot;
use tokio::task;
use tokio::task::JoinHandle;
use duckdb::Connection;
use anyhow::Result;

pub struct DbService {
    db_path: Option<String>,
    db_handle: DbHandle,
    rx: Receiver<DbJob>,
    shutdown_rx: Receiver<()>,
}

impl DbService {
    pub fn new(db_path: Option<String>, shutdown_rx: Receiver<()>) -> anyhow::Result<Self> {
        let (tx, rx): (Sender<DbJob>, Receiver<DbJob>) = unbounded();
        let handle = DbHandle::new(tx);
        Ok(Self { db_path, db_handle: handle, rx, shutdown_rx })
    }

    pub fn get_handle(&self) -> DbHandle {
        self.db_handle.clone()
    }
}

#[async_trait::async_trait]
impl Service for DbService {
    fn svc(&self) -> ServiceType {
        ServiceType::Sink
    }

    async fn start(&self) -> anyhow::Result<tokio::task::JoinHandle<()>> {
        let db_join_handle = start_db_worker(self.db_path.clone(), self.rx.clone(), self.shutdown_rx.clone())?;

        Ok(db_join_handle) 
    }
}


pub enum DbCommand {
    Query(String),
    InsertBatch(RecordBatch, String),
    Flush,
}

struct DbJob {
    command: DbCommand,
    response: tokio::sync::oneshot::Sender<anyhow::Result<DbResponse>>,
}

pub enum DbResponse {
    QueryResult,
    InsertResult,
    FlushResult,
}

#[derive(Clone)]
pub struct DbHandle {
    tx: Sender<DbJob>,
}

impl DbHandle {
    fn new(tx: Sender<DbJob>) -> Self {
        DbHandle { tx }
    }

    pub async fn insert_batch(&self, batch: RecordBatch, table: &str) -> anyhow::Result<()> {
        let (tx, rx) = oneshot::channel();
        let job = DbJob {
            command: DbCommand::InsertBatch(batch, table.to_string()),
            response: tx,
        };
        self.tx.send(job).map_err(|e| anyhow::anyhow!("DB job send error: {}", e))?;
        rx.await.map_err(|e| anyhow::anyhow!("DB job response error: {}", e))??;
        Ok(())
    }

    pub async fn query(&self, query: String) -> anyhow::Result<()> {
        let (tx, rx) = oneshot::channel();
        let job = DbJob {
            command: DbCommand::Query(query),
            response: tx,
        };
        self.tx.send(job).map_err(|e| anyhow::anyhow!("DB job send error: {}", e))?;
        rx.await.map_err(|e| anyhow::anyhow!("DB job response error: {}", e))??;
        Ok(())
    }

    pub async fn flush(&self) -> anyhow::Result<()> {
        let (tx, rx) = oneshot::channel();
        let job = DbJob {
            command: DbCommand::Flush,
            response: tx,
        };
        self.tx.send(job).map_err(|e| anyhow::anyhow!("DB job send error: {}", e))?;
        rx.await.map_err(|e| anyhow::anyhow!("DB job response error: {}", e))??;
        Ok(())
    }
}

// Start the DB worker thread which owns a DuckDB connection and executes jobs.
// If `path` is `Some`, opens that file, otherwise uses an in-memory DB.
// Params:
// - path: Option<String> - Path to DuckDB file or None for in-memory
// - rx: Receiver<DbJob> - Channel receiver for DB jobs
// - shutdown_rx: Receiver<()> - Channel receiver for shutdown signal
// Returns: JoinHandle<()> - Handle to the spawned DB worker thread
fn start_db_worker(path: Option<String>, rx: Receiver<DbJob>, shutdown_rx: Receiver<()>) -> anyhow::Result<JoinHandle<()>> {
    let conn = match path.as_deref() {
        Some(p) => Connection::open(p).map_err(|e| anyhow::anyhow!("Failed to open DuckDB at path {}: {}", p, e)),
        None => Connection::open_in_memory().map_err(|e| anyhow::anyhow!("Failed to open in-memory DuckDB: {}", e)),
    }?;

    // Spawn a blocking thread that owns the DuckDB connection.
    // TODO: Handle connection errors more gracefully - currently panics on failure which is not OK
    let join = task::spawn_blocking(move || {
        loop {
            crossbeam_channel::select! {
                recv(rx) -> job => {
                    let job = match job {
                        Ok(j) => j,
                        Err(_) => {
                            println!("DB worker channel closed, exiting.");
                            // All senders have been dropped, close the connection and exit the loop
                            // Channel closed, exit the loop
                            break;
                        }
                    };
                    
                    handle_db_job(job, &conn).map_err(|e| {
                        println!("Error handling DB job: {}", e);
                    }).ok();
                }

                recv(shutdown_rx) -> _ => {
                    println!("DB worker received shutdown signal.");
                    // Shutdown signal received, exit the loop
                    break;
                }   
            }
        };

        conn.execute("CHECKPOINT", []).map_err(|e| {
            println!("Error in checkpointing {}", e);
        }).ok();

        conn.close().map_err(|e| {
            println!("Error closing DuckDB connection: {}", e.1);
        }).ok();
    });

    Ok(join)
}


fn handle_db_job(job: DbJob, conn: &Connection) -> anyhow::Result<()> {
    match job.command {
        DbCommand::InsertBatch(batch, table) => {
            let res: Result<()> = (|| {
                // Whitelist allowed table names to prevent SQL injection
                match table.as_str() {
                    "measurements" => {
                        let mut appender = conn.appender(&table)?;
                        appender.append_record_batch(batch)?;
                        appender.flush()?;
                        Ok(())
                    }
                    _ => Err(anyhow::anyhow!("Invalid table name")),
                }
            })();
            let _ = job.response.send(res.map(|_| DbResponse::InsertResult));
        }
        DbCommand::Query(sql) => {
            let res = conn.execute(&sql, []);
            let _ = job.response.send(res.map(|_| DbResponse::QueryResult).map_err(|e| anyhow::anyhow!(e)));
        }
        DbCommand::Flush => {
            let res = conn.execute("CHECKPOINT", []);
            let _ = job.response.send(res.map(|_| DbResponse::FlushResult).map_err(|e| anyhow::anyhow!(e)));
            // Reset the unflushed messages counter after a successful flush
        }
    }
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;
    use std::thread;
    use std::sync::Arc;
    use duckdb::arrow::array::Int32Array;
    use duckdb::arrow::datatypes::{Field, Schema, DataType};

    fn make_dummy_batch() -> RecordBatch {
        let a = Int32Array::from(vec![1i32, 2, 3]);
        let field = Field::new("v", DataType::Int32, false);
        let schema = Schema::new(vec![field]);
        RecordBatch::try_new(Arc::new(schema), vec![Arc::new(a)]).unwrap()
    }

    #[tokio::test]
    async fn test_insert_batch_roundtrip() {
        let (tx, rx) = unbounded::<DbJob>();
        let handle = DbHandle::new(tx.clone());

        // Spawn a mock worker thread that receives one job and replies OK
        thread::spawn(move || {
            if let Ok(job) = rx.recv() {
                match job.command {
                    DbCommand::InsertBatch(_batch, _table) => {
                        let _ = job.response.send(Ok(DbResponse::InsertResult));
                    }
                    _ => {
                        let _ = job.response.send(Ok(DbResponse::QueryResult));
                    }
                }
            }
        });

        let batch = make_dummy_batch();
        let res = handle.insert_batch(batch, "test_table").await;
        assert!(res.is_ok(), "insert_batch should succeed");
    }

    #[tokio::test]
    async fn test_query_roundtrip() {
        let (tx, rx) = unbounded::<DbJob>();
        let handle = DbHandle::new(tx.clone());

        thread::spawn(move || {
            if let Ok(job) = rx.recv() {
                match job.command {
                    DbCommand::Query(_q) => {
                        let _ = job.response.send(Ok(DbResponse::QueryResult));
                    }
                    _ => {
                        let _ = job.response.send(Ok(DbResponse::QueryResult));
                    }
                }
            }
        });

        let res = handle.query("SELECT 1".to_string()).await;
        assert!(res.is_ok(), "query should succeed");
    }

    #[tokio::test]
    async fn test_flush_roundtrip() {
        let (tx, rx) = unbounded::<DbJob>();
        let handle = DbHandle::new(tx.clone());

        thread::spawn(move || {
            if let Ok(job) = rx.recv() {
                let _ = job.response.send(Ok(DbResponse::FlushResult));
            }
        });

        let res = handle.flush().await;
        assert!(res.is_ok(), "flush should succeed");
    }
}