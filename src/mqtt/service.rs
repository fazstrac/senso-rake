// MQTT background task. This connects to the broker using `rumqttc` and
// subscribes to the configured topic namespace. For each incoming message
// we increment the provided `IntCounter` and print the event. In a real
// implementation you'd persist raw messages to DuckDB/DuckLake and perform
// structured parsing/validation.
use crate::service::{Service, ServiceType};
use crate::shutdown_token::ShutdownToken;

use log::{debug, error, info};
use prometheus::IntCounter;
use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, QoS};
use tokio::time::{self, Duration};

use crate::mqtt::mqtt_buffer;

/// Start a long-running MQTT loop. This function never returns unless an
/// unrecoverable error occurs. It is intended to be spawned with
/// `tokio::task::spawn` from `server::run()` so it runs in the background.
use crate::database::DbHandle;

pub struct MqttService {
    counter_tot_msg: IntCounter,
    counter_unflushed_msg: IntCounter,
    db: DbHandle,
    shutdown_token: ShutdownToken,
}

impl MqttService {
    pub fn new(
        counter_tot_msg: IntCounter,
        counter_unflushed_msg: IntCounter,
        db: DbHandle,
        shutdown_token: ShutdownToken,
    ) -> Self {
        MqttService {
            counter_tot_msg,
            counter_unflushed_msg,
            db,
            shutdown_token,
        }
    }
}

#[async_trait::async_trait]
impl Service for MqttService {
    fn svc(&self) -> ServiceType {
        ServiceType::Source
    }

    async fn start(&self) -> anyhow::Result<tokio::task::JoinHandle<()>> {
        start_mqtt_worker(
            self.counter_tot_msg.clone(),
            self.counter_unflushed_msg.clone(),
            self.db.clone(),
            self.shutdown_token.clone(),
        )
        .await
    }
}

async fn flush_pending(
    db: &DbHandle,
    rows: &mut Vec<mqtt_buffer::ProcessedMsg>,
    unflushed_counter: &IntCounter,
) -> anyhow::Result<()> {
    if rows.is_empty() {
        return Ok(());
    }

    let batch = mqtt_buffer::create_arrow_record_batch(rows)?;
    db.insert_batch(batch, "data_landing").await?;

    rows.clear();
    unflushed_counter.reset();
    Ok(())
}

pub async fn start_mqtt_worker(
    counter_tot_msg: IntCounter,
    counter_unflushed_msg: IntCounter,
    db: DbHandle,
    shutdown_token: ShutdownToken,
) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    // Create MQTT options from environment variables. Check for host,
    // port, username, and password; use defaults if not provided.
    // Not all fields are required; we default to localhost:1883
    // with no authentication if env vars are missing.

    let mut mqttoptions: MqttOptions;

    // Read credentials from environment and set them if both present.
    // This keeps defaults simple (no auth) while enabling secure
    // deployments by setting the env vars.
    let mqtt_host = std::env::var("MQTT_HOST").ok();
    let mqtt_port = std::env::var("MQTT_PORT").ok();
    let mqtt_user = std::env::var("MQTT_USER").ok();
    let mqtt_pass = std::env::var("MQTT_PASS").ok();
    let mqtt_topic = std::env::var("MQTT_TOPIC").ok();

    let mqtt_flush_interval = std::env::var("MQTT_FLUSH_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(600); // default to 600 seconds    

    match (mqtt_host, mqtt_port) {
        // No host or port: default to localhost:1883
        (None, None) => {
            mqttoptions = MqttOptions::new("rust_exporter_client", "localhost", 1883);
            info!("Connecting to MQTT broker at localhost:1883");
        }
        // Host and port provided, use both
        (Some(host), Some(port)) => {
            let p = port.trim().parse::<u16>().map_err(|e| {
                anyhow::anyhow!("Invalid MQTT_PORT value, expected a number, got: {}", e)
            })?;
            mqttoptions = MqttOptions::new("rust_exporter_client", &host, p);
            info!("Connecting to MQTT broker at {}:{}", host, p);
        }
        // Only host provided, default to port 1883
        (Some(host), None) => {
            mqttoptions = MqttOptions::new("rust_exporter_client", &host, 1883);
            info!("Connecting to MQTT broker at {}:1883", host);
        }
        // Only port provided, default to localhost as host
        (None, Some(port)) => {
            let p = port.trim().parse::<u16>().map_err(|e| {
                anyhow::anyhow!("Invalid MQTT_PORT value, expected a number, got: {}", e)
            })?;
            mqttoptions = MqttOptions::new("rust_exporter_client", "localhost", p);
            info!("Connecting to MQTT broker at localhost:{}", p);
        }
    }

    mqttoptions.set_keep_alive(std::time::Duration::from_secs(5));

    match (mqtt_user, mqtt_pass) {
        (Some(user), Some(pass)) => {
            mqttoptions.set_credentials(&user, &pass);
            info!("Using MQTT credentials from environment {}:*******", user);
        }
        (Some(_), None) | (None, Some(_)) => {
            // Warn but continue without credentials if only one is set.
            return Err(anyhow::anyhow!(
                "MQTT credentials incomplete: both MQTT_USER and MQTT_PASS must be set to enable auth"
            ));
        }
        (None, None) => {
            // No credentials configured; proceed unauthenticated.
            info!("No MQTT credentials provided; connecting without authentication");
        }
    }

    let (client, mut eventloop) = AsyncClient::new(mqttoptions, 10);

    match mqtt_topic {
        Some(topic) => {
            client
                .subscribe(&topic, QoS::AtLeastOnce)
                .await
                .map_err(|e| anyhow::anyhow!("Error subscribing to MQTT topic {}: {}", topic, e))?;
            info!("Subscribing to MQTT topic: {}", topic);
        }
        None => {
            return Err(anyhow::anyhow!(
                "MQTT_TOPIC environment variable not set, cannot subscribe to topic"
            ));
        }
    }

    let mut all_rows: Vec<mqtt_buffer::ProcessedMsg> = Vec::new();

    // Timer for periodic flush and checkpoint
    let mut interval_flush = time::interval(Duration::from_secs(mqtt_flush_interval));

    let join_handle = tokio::task::spawn(async move {
        loop {
            tokio::select! {
                // General idea:
                // Handle incoming MQTT messages and process them
                // Flush to DuckDB periodically or based on message count if there is a burst
                // Checkpoint DuckDB periodically to ensure data is persisted

                // MQTT event
                ev = eventloop.poll() => {
                    match ev {
                        // increase unflushed count and store normalized rows
                        // on receiving a publish
                        // If unflushed count exceeds threshold, flush to DuckDB
                        // that happens most likely during bursts of messages (over 500 msgs per 113 seconds)
                        Ok(Event::Incoming(Incoming::Publish(p))) => {
                            counter_tot_msg.inc();
                            counter_unflushed_msg.inc();
                            info!("Got topic: {}, Count: {}, Unflushed: {}", p.topic, counter_tot_msg.get(), counter_unflushed_msg.get());

                            let payload_str = String::from_utf8_lossy(&p.payload);
                            let msg = mqtt_buffer::process_message(&payload_str);
                            all_rows.push(msg);

                            // check if we should flush to DuckDB
                            if counter_unflushed_msg.get() >= 500 {
                                // Every 500 hits, flush to DuckDB
                                let len = all_rows.len();

                                match flush_pending(&db, &mut all_rows, &counter_unflushed_msg).await {
                                    Ok(_) => {
                                        info!("Every 500 row flush: Flushed {} rows to DuckDB", len);
                                    }
                                    Err(e) => error!("Error flushing data: {}", e),
                                }
                            }
                        }
                        Ok(Event::Incoming(i)) => {
                            debug!("Incoming = {i:?}");
                        }
                        Ok(Event::Outgoing(o)) => {
                            debug!("Outgoing = {o:?}");
                        }
                        Err(e) => {
                            // Don't crash on MQTT errors; restart the connection.
                            error!("MQTT error encountered, connection should be restarted automatically: {}", e);
                            // Back off briefly before retrying
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await
                        }
                    }
                }
                // Timer tick
                _ = interval_flush.tick() => {
                    // Periodic flush to DuckDB
                    let len = all_rows.len();

                    match flush_pending(&db, &mut all_rows, &counter_unflushed_msg).await {
                        Ok(_) => {
                            info!("Periodic flush: Flushed {} rows to DuckDB", len);
                        }
                        Err(e) => error!("Error flushing data: {}", e),
                    }
                }
                // Shutdown signal
                _ = shutdown_token.wait() => {
                    // Perform final flush before exiting
                    info!("MQTT loop received shutdown signal, exiting.");

                    let len = all_rows.len();

                    match flush_pending(&db, &mut all_rows, &counter_unflushed_msg).await {
                        Ok(_) => {
                            info!("Shutdown flush: Flushed {} rows to DuckDB", len);
                        }
                        Err(e) => error!("Error flushing data: {}", e),
                    }

                    info!("Final MQTT loop cleanup done, exiting.");
                    break;
                }
            }
        }

        info!("MQTT loop exiting cleanly.");
    });

    Ok(join_handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crossbeam_channel::{Receiver, TryRecvError};

    use crate::database::{DbCommand::InsertBatch, DbHandle, DbJob, DbResponse::InsertResult};
    use prometheus::IntCounter;

    fn fake_db() -> (DbHandle, Receiver<DbJob>) {
        let (tx, rx) = crossbeam_channel::unbounded();
        (DbHandle::new(tx), rx)
    }

    enum ExpectedDbCommand {
        InsertBatch { expected_row_count: usize },
    }

    struct EventHandlingTestCase {
        name: &'static str,
        rows: Vec<mqtt_buffer::ProcessedMsg>,
        unflushed_counter: u64,
        expected_db_command: Option<ExpectedDbCommand>,
        will_error: bool,
    }

    #[tokio::test]
    async fn flush_pending_follows_contract() {
        let cases = vec![
            // test cases
            EventHandlingTestCase {
                name: "empty_buffer_does_not_send_database_job",
                rows: Vec::new(),
                unflushed_counter: 0,
                expected_db_command: None,
                will_error: false,
            },
            EventHandlingTestCase {
                name: "failed_flush_keeps_rows_for_retry",
                rows: vec![mqtt_buffer::process_message(
                    r#"{"time": "1704643200.123456", "field": "value"}"#,
                )],
                unflushed_counter: 1,
                expected_db_command: Some(ExpectedDbCommand::InsertBatch {
                    expected_row_count: 1,
                }),
                will_error: true,
            },
            EventHandlingTestCase {
                name: "successful_flush_sends_all_rows_and_clears_buffer_one_message",
                rows: vec![mqtt_buffer::process_message(
                    r#"{"time": "1704643200.123456", "field": "value"}"#,
                )],
                unflushed_counter: 1,
                expected_db_command: Some(ExpectedDbCommand::InsertBatch {
                    expected_row_count: 1,
                }),
                will_error: false,
            },
            EventHandlingTestCase {
                name: "successful_flush_sends_all_rows_and_clears_buffer_multiple_messages",
                rows: vec![
                    mqtt_buffer::process_message(
                        r#"{"time": "1704643200.123456", "field": "value1"}"#,
                    ),
                    mqtt_buffer::process_message(
                        r#"{"time": "1704643200.123457", "field": "value2"}"#,
                    ),
                ],
                unflushed_counter: 2,
                expected_db_command: Some(ExpectedDbCommand::InsertBatch {
                    expected_row_count: 2,
                }),
                will_error: false,
            },
        ];

        for mut case in cases {
            let (handle, rx) = fake_db();
            let counter = IntCounter::new(
                "router_test_requests_total",
                "Requests observed by the router test",
            )
            .unwrap();

            let worker_rx = rx.clone();

            let orig_rows = case.rows.clone();
            let name = case.name;

            let worker = case.expected_db_command.map(|expected| {
                std::thread::spawn(move || {
                    let job = worker_rx.recv().unwrap();

                    match (expected, job.command) {
                        (
                            ExpectedDbCommand::InsertBatch { expected_row_count },
                            InsertBatch(batch, table),
                        ) => {
                            // Internal check - the table name remains correct
                            assert_eq!(table, "data_landing");
                            assert_eq!(batch.num_rows(), expected_row_count);

                            match case.will_error {
                                true => job
                                    .response
                                    .send(Err(anyhow::anyhow!("Planned error in test")))
                                    .unwrap(),
                                false => job.response.send(Ok(InsertResult)).unwrap(),
                            }
                        }
                        _ => panic!("{name}: unexpected database command"),
                    }
                })
            });
            counter.reset();
            counter.inc_by(case.unflushed_counter);

            match flush_pending(&handle, &mut case.rows, &counter).await {
                Ok(_) => {
                    // Verify this wasn't supposed to error
                    assert!(!case.will_error, "{name}");
                    // Verify the counter gets reset
                    assert_eq!(counter.get(), 0);
                    // Verify the rows get consumed
                    assert_eq!(case.rows.len(), 0)
                }
                Err(e) => {
                    // Verify that we were supposed to error here
                    assert!(case.will_error, "{name}");
                    // Verify the errors is due to our scaffolding, not something else - redundant with the above
                    assert_eq!(e.to_string(), "Planned error in test", "{name}");
                    // Verify the counter remains unchanged
                    assert_eq!(counter.get(), case.unflushed_counter);
                    // Verify the rows remain unchanged
                    assert_eq!(case.rows, orig_rows)
                }
            }

            match worker {
                Some(worker) => worker.join().unwrap(),
                None => match rx.try_recv() {
                    Err(TryRecvError::Empty) => {}
                    Ok(_) => panic!("{} unexpectedly sent a database job", case.name),
                    Err(TryRecvError::Disconnected) => {
                        panic!("{} database channel unexpectedly disconnected", case.name)
                    }
                },
            }
        }
    }
}
