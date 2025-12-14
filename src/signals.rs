use tokio::signal::unix::{signal, SignalKind};
use std::sync::Arc;
use crate::db;

pub async fn start_signal_handler(
    shutdown_notify_task2: Arc<tokio::sync::Notify>,
    mqtt_join: tokio::task::JoinHandle<()>,
    db_handle_for_signal: db::DbHandle,
    _db_join: tokio::task::JoinHandle<()>,
    _http_join_handle: tokio::task::JoinHandle<()>,
) -> tokio::task::JoinHandle<()> {

    let join_handle = tokio::spawn(async move {
        let mut sighup = signal(SignalKind::hangup()).unwrap();
        let mut sigterm = signal(SignalKind::terminate()).unwrap();
        let mut sigint = signal(SignalKind::interrupt()).unwrap();

        let handle_shutdown = async |signal_name: String| {
            println!("Received {}, shutting down...", signal_name);
            // Notify MQTT task to shut down. It will flush and shut down the DB.
            shutdown_notify_task2.notify_waiters();

            println!("Waiting for HTTP, MQTT task and DB thread to finish...");

            // REFACTOR: refactor http handlers and mqtt task to share db handle properly
            // also refactor http handler into its own module and create start_http_server function

            _http_join_handle.await.unwrap_or_else(|e| {
                eprintln!("Error joining HTTP server task on shutdown: {}", e);
            });

            println!("HTTP server has shut down.");

            // Await MQTT task completion
            mqtt_join.await.unwrap_or_else(|e| {
                eprintln!("Error joining MQTT task on shutdown: {}", e);
            });

            println!("MQTT worker has shut down.");

            db_handle_for_signal.shutdown().await.unwrap_or_else(|e| {
                eprintln!("Error shutting down DB on shutdown: {}", e);
            });

            println!("Database has shut down.");

            // Join DB thread
            _db_join.await.unwrap_or_else(|e| {
                eprintln!("Error joining DB thread on shutdown: {:?}", e);
            });

            println!("Shutdown complete.");            
        };

        // Handle signals for SIGHUP (checkpoint), SIGINT and SIGTERM (graceful shutdown)
        // As it is now, does affect 
        loop {
            tokio::select! {
                _ = sighup.recv() => {
                    println!("Received SIGHUP, CHECKPOINTING database...");

                    db_handle_for_signal.flush().await.unwrap_or_else(|e| {
                        eprintln!("Error flushing DB on SIGHUP: {}", e);
                    });
                }
                _ = sigint.recv() => {
                    handle_shutdown("SIGINT".to_string()).await;
                    break;
                },
                _ = sigterm.recv() => {
                    handle_shutdown("SIGTERM".to_string()).await;
                    break;
                }
            }
        }

        println!("Signal handling task exiting cleanly.");
    });

    join_handle
}
