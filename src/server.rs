// `server.rs` composes the HTTP application: it loads initial state,
// registers Prometheus metrics, starts the MQTT background task, and
// mounts HTTP handlers and middleware.
use crate::orchestrator::Orchestrator;
use crate::{database, http, mqtt, service::Service, shutdown_token};

use crossbeam_channel::{Receiver, Sender, unbounded};
use log::info;
use prometheus::{IntCounter, Registry};
use std::sync::Arc;
use tokio::signal::unix::{SignalKind, signal};

pub async fn run() -> anyhow::Result<()> {
    let registry = Arc::new(Registry::new());
    let mqtt_messages_received_counter =
        IntCounter::new("mqtt_messages_total", "Total MQTT messages received")?;
    let mqtt_messages_not_flushed_to_db = IntCounter::new(
        "mqtt_unflushed_total",
        "Total unflushed MQTT messages in WAL",
    )?;
    registry.register(Box::new(mqtt_messages_received_counter.clone()))?;
    registry.register(Box::new(mqtt_messages_not_flushed_to_db.clone()))?;

    let shutdown_token = shutdown_token::ShutdownToken::new();

    let mut services: Vec<Box<dyn Service>> = vec![];

    // Build DB service
    let db_path = std::env::var("DUCKDB_PATH").ok();
    let (db_shutdown_tx, db_shutdown_rx): (Sender<()>, Receiver<()>) = unbounded();

    let db_svc = database::DbService::new(db_path, db_shutdown_rx)?;
    let db_handle = db_svc.get_handle();

    services.push(Box::new(db_svc));

    // Build MQTT service
    let mqtt_service = mqtt::MqttService::new(
        mqtt_messages_received_counter.clone(),
        mqtt_messages_not_flushed_to_db.clone(),
        db_handle.clone(),
        shutdown_token.clone(),
    );

    services.push(Box::new(mqtt_service));

    // Build HTTP service
    let http_service =
        http::HttpService::new(db_handle.clone(), registry.clone(), shutdown_token.clone());

    services.push(Box::new(http_service));

    let mut orchestrator = Orchestrator::new(services, shutdown_token, db_shutdown_tx);

    orchestrator.start_all().await?;

    let (shutdown_notify_tx, shutdown_notify_rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        // let mut sighup = signal(SignalKind::hangup()).unwrap();
        let mut sigterm = signal(SignalKind::terminate()).expect("Failed to bind to SIGTERM");
        let mut sigint = signal(SignalKind::interrupt()).expect("Failed to bind to SIGINT");

        tokio::select! {
            _ = sigterm.recv() => info!("Received SIGTERM, initiating shutdown. Press again to force exit."),
            _ = sigint.recv() => info!("Received SIGINT, initiating shutdown. Press again to force exit."),
        }

        let _ = shutdown_notify_tx.send(());

        // Refactor: consider different strategy than bypassing normal shutdown. Possibly even removing this.

        tokio::select! {
            _ = sigterm.recv() => {
                info!("Second SIGTERM, exiting immediately.");
                std::process::exit(1);
            },
            _ = sigint.recv() => {
                info!("Second SIGINT, exiting immediately.");
                std::process::exit(1);
            },
        }
    });

    let _ = shutdown_notify_rx.await;
    orchestrator.initiate_shutdown().await;
    orchestrator.shutdown_all().await;

    Ok(())
}
