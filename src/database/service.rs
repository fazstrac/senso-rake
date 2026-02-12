use log::{error, info};

use crate::database::schema;
// Database interaction module
use crate::service::{Service, ServiceType};

use anyhow::Result;
use crossbeam_channel::{Receiver, Sender, unbounded};
use duckdb::Connection;
use duckdb::arrow::record_batch::RecordBatch;
use tokio::sync::oneshot;
use tokio::task;
use tokio::task::JoinHandle;

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
        Ok(Self {
            db_path,
            db_handle: handle,
            rx,
            shutdown_rx,
        })
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
        let db_join_handle = start_db_worker(
            self.db_path.clone(),
            self.rx.clone(),
            self.shutdown_rx.clone(),
        )?;

        Ok(db_join_handle)
    }
}

pub enum DbCommand {
    Query(String),
    QueryWithParams(String, Vec<String>),
    // ExecuteBatch(String),
    InsertBatch(RecordBatch, String),
}

pub struct DbJob {
    pub command: DbCommand,
    pub response: tokio::sync::oneshot::Sender<anyhow::Result<DbResponse>>,
}

pub enum DbResponse {
    QueryResult(String),
    // ExecuteBatchResult,
    InsertResult,
}

#[derive(Clone)]
pub struct DbHandle {
    tx: Sender<DbJob>,
}

impl DbHandle {
    pub(crate) fn new(tx: Sender<DbJob>) -> Self {
        DbHandle { tx }
    }

    pub async fn insert_batch(&self, batch: RecordBatch, table: &str) -> anyhow::Result<()> {
        let (tx, rx) = oneshot::channel();
        let job = DbJob {
            command: DbCommand::InsertBatch(batch, table.to_string()),
            response: tx,
        };
        self.tx
            .send(job)
            .map_err(|e| anyhow::anyhow!("DB job send error: {}", e))?;
        rx.await
            .map_err(|e| anyhow::anyhow!("DB job response error: {}", e))??;
        Ok(())
    }

    // These will be readded later as needed

    // pub async fn execute_batch(&self, batch: String) -> anyhow::Result<()> {
    //     let (tx, rx) = oneshot::channel();
    //     let job = DbJob {
    //         command: DbCommand::ExecuteBatch(batch),
    //         response: tx,
    //     };
    //     self.tx
    //         .send(job)
    //         .map_err(|e| anyhow::anyhow!("DB job send error: {}", e))?;
    //     rx.await
    //         .map_err(|e| anyhow::anyhow!("DB job response error: {}", e))??;
    //     Ok(())
    // }

    pub async fn query(&self, query: String) -> anyhow::Result<String> {
        let (tx, rx) = oneshot::channel();
        let job = DbJob {
            command: DbCommand::Query(query),
            response: tx,
        };
        self.tx
            .send(job)
            .map_err(|e| anyhow::anyhow!("DB job send error: {}", e))?;

        match rx.await
            .map_err(|e| anyhow::anyhow!("DB job response error: {}", e))?? {
            DbResponse::QueryResult(json) => Ok(json),
            _ => Err(anyhow::anyhow!("Unexpected DB response")),
        }
    }

    pub async fn query_with_params(&self, query: String, params: Vec<String>) -> anyhow::Result<String> {
        let (tx, rx) = oneshot::channel();
        let job = DbJob {
            command: DbCommand::QueryWithParams(query, params),
            response: tx,
        };
        self.tx
            .send(job)
            .map_err(|e| anyhow::anyhow!("DB job send error: {}", e))?;

        match rx.await
            .map_err(|e| anyhow::anyhow!("DB job response error: {}", e))?? {
            DbResponse::QueryResult(json) => Ok(json),
            _ => Err(anyhow::anyhow!("Unexpected DB response")),
        }
    }
}

// Start the DB worker thread which owns a DuckDB connection and executes jobs.
// If `path` is `Some`, opens that file, otherwise uses an in-memory DB.
// Params:
// - path: Option<String> - Path to DuckDB file or None for in-memory
// - rx: Receiver<DbJob> - Channel receiver for DB jobs
// - shutdown_rx: Receiver<()> - Channel receiver for shutdown signal
// Returns: JoinHandle<()> - Handle to the spawned DB worker thread
fn start_db_worker(
    path: Option<String>,
    rx: Receiver<DbJob>,
    shutdown_rx: Receiver<()>,
) -> anyhow::Result<JoinHandle<()>> {
    let conn = match path.as_deref() {
        Some(p) => Connection::open(p)
            .map_err(|e| anyhow::anyhow!("Failed to open DuckDB at path {}: {}", p, e)),
        None => Connection::open_in_memory()
            .map_err(|e| anyhow::anyhow!("Failed to open in-memory DuckDB: {}", e)),
    }?;

    let table_update_interval_secs = std::env::var("TABLE_UPDATE_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(600); // default to 600 seconds


    let ticker = crossbeam_channel::tick(std::time::Duration::from_secs(table_update_interval_secs));

    // Initialize the database
    // Consider if this could be done externally via eg init scripts
    let res = conn.execute_batch(&format!(
        "BEGIN; {} {} COMMIT; CHECKPOINT;",
        schema::SCHEMA_SQL,
        schema::UPDATE_TABLES_SQL
    ));
    info!("Database initialized successfully");

    res.map_err(|e| anyhow::anyhow!("Error initializing database: {}", e))?;

    // Spawn a blocking thread that owns the DuckDB connection.
    // TODO: Handle connection errors more gracefully - currently panics on failure which is not OK
    let join = task::spawn_blocking(move || {
        loop {
            crossbeam_channel::select! {
                // Database job received, handle it
                recv(rx) -> job => {
                    let job = match job {
                        Ok(j) => j,
                        Err(_) => {
                            info!("DB worker channel closed, exiting.");
                            // All senders have been dropped, close the connection and exit the loop
                            // Channel closed, exit the loop
                            break;
                        }
                    };

                    handle_db_job(job, &conn).map_err(|e| {
                        error!("Error handling DB job: {}", e);
                    }).ok();
                }

                // Shutdown signal received, exit the loop
                recv(shutdown_rx) -> _ => {
                    info!("DB worker received shutdown signal.");
                    // Shutdown signal received, exit the loop
                    break;
                }

                // Periodic ticker to do periodic updates
                recv(ticker) -> _ => {
                    info!("Periodic update of derived tables");
                    conn.execute_batch(&format!(
                        "BEGIN; {} COMMIT;",
                        schema::UPDATE_TABLES_SQL
                    ))
                    .map_err(|e| {
                        error!("Error in updating {}", e);
                    })
                    .ok();
                }
            }
        }

        conn.execute("CHECKPOINT", [])
            .map_err(|e| {
                error!("Error in checkpointing {}", e);
            })
            .ok();

        conn.close()
            .map_err(|e| {
                error!("Error closing DuckDB connection: {}", e.1);
            })
            .ok();
    });

    Ok(join)
}

fn handle_db_job(job: DbJob, conn: &Connection) -> anyhow::Result<()> {
    match job.command {
        // DbCommand::ExecuteBatch(sql) => {
        //     let res = conn.execute_batch(&sql);
        //     let _ = job.response.send(
        //         res.map(|_| DbResponse::ExecuteBatchResult)
        //             .map_err(|e| anyhow::anyhow!(e)),
        //     );
        // }
        DbCommand::InsertBatch(batch, table) => {
            let res: Result<()> = (|| {
                // Whitelist allowed table names to prevent SQL injection
                match table.as_str() {
                    "data_landing" => {
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
            use arrow_json::writer::ArrayWriter;

            let res: Result<String> = (|| {
                let mut stmt = conn.prepare(&sql)?;

                let arrow = stmt.query_arrow([])?;
                let batches: Vec<RecordBatch> = arrow.collect();

                let mut buf = Vec::new();
                {
                    let mut writer = ArrayWriter::new(&mut buf);
                    for batch in batches {
                        writer.write(&batch)?;
                    }
                    writer.finish()?;
                }

                let json_bytes = String::from_utf8(buf)?;
                Ok(json_bytes)
            })();
            let _ = job.response.send(
                res.map(|json| DbResponse::QueryResult(json))
                    .map_err(|e| anyhow::anyhow!(e)),
            );
        }
        DbCommand::QueryWithParams(sql, params) => {
            use arrow_json::writer::ArrayWriter;

            let res: Result<String> = (|| {
                let mut stmt = conn.prepare(&sql)?;

                // Pass parameters as array. DuckDB Rust bindings require fixed-size arrays or slices
                let arrow = match params.len() {
                    1 => {
                        let p: [&dyn duckdb::ToSql; 1] = [&params[0]];
                        stmt.query_arrow(&p)?
                    }
                    2 => {
                        let p: [&dyn duckdb::ToSql; 2] = [&params[0], &params[1]];
                        stmt.query_arrow(&p)?
                    }
                    3 => {
                        let p: [&dyn duckdb::ToSql; 3] = [&params[0], &params[1], &params[2]];
                        stmt.query_arrow(&p)?
                    }
                    _ => return Err(anyhow::anyhow!("Unsupported parameter count: {}", params.len())),
                };

                let batches: Vec<RecordBatch> = arrow.collect();

                let mut buf = Vec::new();
                {
                    let mut writer = ArrayWriter::new(&mut buf);
                    for batch in batches {
                        writer.write(&batch)?;
                    }
                    writer.finish()?;
                }

                let json_bytes = String::from_utf8(buf)?;
                Ok(json_bytes)
            })();
            let _ = job.response.send(
                res.map(|json| DbResponse::QueryResult(json))
                    .map_err(|e| anyhow::anyhow!(e)),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;
    use duckdb::arrow::array::Int32Array;
    use duckdb::arrow::datatypes::{DataType, Field, Schema};
    use std::sync::Arc;
    use std::thread;

    fn make_dummy_batch() -> RecordBatch {
        let a = Int32Array::from(vec![1i32, 2, 3]);
        let field = Field::new("v", DataType::Int32, false);
        let schema = Schema::new(vec![field]);
        RecordBatch::try_new(Arc::new(schema), vec![Arc::new(a)]).unwrap()
    }

    #[tokio::test]
    async fn test_insert_batch_roundtrip() {
        let (tx, _rx) = unbounded::<DbJob>();
        let handle = DbHandle::new(tx.clone());

        // Spawn a mock worker thread that receives one job and replies OK
        thread::spawn(move || {
            if let Ok(job) = _rx.recv() {
                match job.command {
                    DbCommand::InsertBatch(_batch, _table) => {
                        let _ = job.response.send(Ok(DbResponse::InsertResult));
                    }
                    DbCommand::Query(_sql) => {
                        let _ = job.response.send(Ok(DbResponse::QueryResult("[]".to_string())));
                    }
                    DbCommand::QueryWithParams(_sql, _params) => {
                        let _ = job.response.send(Ok(DbResponse::QueryResult("[]".to_string())));
                    }
                }
            }
        });

        let batch = make_dummy_batch();
        let res = handle.insert_batch(batch, "test_table").await;
        assert!(res.is_ok(), "insert_batch should succeed");
    }

    #[tokio::test]
    async fn test_query_returns_json() {
        let (tx, rx) = unbounded::<DbJob>();
        let handle = DbHandle::new(tx.clone());

        // Spawn a real worker thread with an in-memory database
        thread::spawn(move || {
            let conn = Connection::open_in_memory().expect("Failed to open in-memory DB");

            // Create a simple test table
            conn.execute(
                "CREATE TABLE test (id INTEGER, name VARCHAR)",
                [],
            )
            .expect("Failed to create table");

            // Insert test data
            conn.execute(
                "INSERT INTO test VALUES (1, 'Alice'), (2, 'Bob')",
                [],
            )
            .expect("Failed to insert data");

            // Process jobs
            loop {
                match rx.recv() {
                    Ok(job) => {
                        match job.command {
                            DbCommand::Query(sql) => {
                                use arrow_json::writer::ArrayWriter;

                                let res: Result<String> = (|| {
                                    let mut stmt = conn.prepare(&sql)?;
                                    let arrow = stmt.query_arrow([])?;
                                    let batches: Vec<RecordBatch> = arrow.collect();

                                    let mut buf = Vec::new();
                                    {
                                        let mut writer = ArrayWriter::new(&mut buf);
                                        for batch in batches {
                                            writer.write(&batch)?;
                                        }
                                        writer.finish()?;
                                    }

                                    let json_string = String::from_utf8(buf)?;
                                    Ok(json_string)
                                })();

                                let _ = job.response.send(
                                    res.map(|json| DbResponse::QueryResult(json))
                                        .map_err(|e| anyhow::anyhow!(e)),
                                );
                            }
                            DbCommand::QueryWithParams(sql, params) => {
                                use arrow_json::writer::ArrayWriter;

                                let res: Result<String> = (|| {
                                    let mut stmt = conn.prepare(&sql)?;

                                    let arrow = match params.len() {
                                        1 => {
                                            let p: [&dyn duckdb::ToSql; 1] = [&params[0]];
                                            stmt.query_arrow(&p)?
                                        }
                                        2 => {
                                            let p: [&dyn duckdb::ToSql; 2] = [&params[0], &params[1]];
                                            stmt.query_arrow(&p)?
                                        }
                                        3 => {
                                            let p: [&dyn duckdb::ToSql; 3] = [&params[0], &params[1], &params[2]];
                                            stmt.query_arrow(&p)?
                                        }
                                        _ => return Err(anyhow::anyhow!("Unsupported parameter count: {}", params.len())),
                                    };

                                    let batches: Vec<RecordBatch> = arrow.collect();

                                    let mut buf = Vec::new();
                                    {
                                        let mut writer = ArrayWriter::new(&mut buf);
                                        for batch in batches {
                                            writer.write(&batch)?;
                                        }
                                        writer.finish()?;
                                    }

                                    let json_string = String::from_utf8(buf)?;
                                    Ok(json_string)
                                })();

                                let _ = job.response.send(
                                    res.map(|json| DbResponse::QueryResult(json))
                                        .map_err(|e| anyhow::anyhow!(e)),
                                );
                            }
                            _ => {
                                let _ = job.response.send(Err(anyhow::anyhow!("Unsupported command")));
                            }
                        }
                    }
                    Err(_) => {
                        // Channel closed, exit
                        break;
                    }
                }
            }
        });

        // Query the data
        let json_result = handle
            .query("SELECT * FROM test ORDER BY id".to_string())
            .await;

        assert!(json_result.is_ok(), "query should succeed");

        let json_str = json_result.unwrap();
        assert!(!json_str.is_empty(), "JSON result should not be empty");

        // Parse and verify the JSON
        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).expect("Failed to parse JSON result");

        let records = parsed
            .as_array()
            .expect("JSON result should be an array");

        assert_eq!(records.len(), 2, "Should have 2 records");

        // Verify first record
        let first = &records[0];
        assert_eq!(first["id"], 1);
        assert_eq!(first["name"], "Alice");

        // Verify second record
        let second = &records[1];
        assert_eq!(second["id"], 2);
        assert_eq!(second["name"], "Bob");
    }

}
