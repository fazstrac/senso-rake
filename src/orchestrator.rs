use std::vec;

use crate::{service::Service, shutdown_token::ShutdownToken};
use tokio::task::JoinHandle;

pub struct Orchestrator {
    services: vec::Vec<Box<dyn Service>>,
    sink_handles: vec::Vec<JoinHandle<()>>,
    source_handles: vec::Vec<JoinHandle<()>>,
    shutdown_token: ShutdownToken,
    shutdown_tx: crossbeam_channel::Sender<()>,
}

impl Orchestrator {
    pub fn new(
        services: Vec<Box<dyn Service>>,
        shutdown_token: ShutdownToken,
        shutdown_tx: crossbeam_channel::Sender<()>,
    ) -> Self {
        Self {
            services,
            sink_handles: Vec::new(),
            source_handles: Vec::new(),
            shutdown_token,
            shutdown_tx,
        }
    }

    // Start all services
    // Note the order: phase 2 services (e.g., DB) are started before phase 1 services (e.g., MQTT, HTTP)
    pub async fn start_all(&mut self) -> anyhow::Result<()> {
        for svc in self
            .services
            .iter()
            .filter(|s| matches!(s.svc(), crate::service::ServiceType::Sink))
        {
            let handle = svc.start().await?;
            self.sink_handles.push(handle);
        }

        for svc in self
            .services
            .iter()
            .filter(|s| matches!(s.svc(), crate::service::ServiceType::Source))
        {
            let handle = svc.start().await?;
            self.source_handles.push(handle);
        }

        Ok(())
    }

    // Initiate shutdown by triggering the shutdown token
    // This will notify all services using this token to begin their shutdown procedures
    pub async fn initiate_shutdown(&mut self) {
        self.shutdown_token.trigger();
    }

    // Wait for all services to shutdown gracefully
    pub async fn shutdown_all(&mut self) {
        for svc in self
            .services
            .iter()
            .filter(|s| matches!(s.svc(), crate::service::ServiceType::Source))
        {
            let _ = svc.shutdown().await;
            let handle = self.source_handles.pop();
            if let Some(h) = handle {
                let _ = h.await;
            }
        }

        // Start by sending shutdown signal to DB worker
        let _ = self.shutdown_tx.send(());

        for svc in self
            .services
            .iter()
            .filter(|s| matches!(s.svc(), crate::service::ServiceType::Sink))
        {
            let _ = svc.shutdown().await;
            let handle = self.sink_handles.pop();
            if let Some(h) = handle {
                let _ = h.await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};
    use tokio::sync::{Notify, oneshot};
    use tokio::task::JoinHandle;
    use tokio::time::{Duration, timeout};

    struct FakeService {
        name: String,
        is_source: bool,
        notify: Arc<Notify>,
        events: Arc<Mutex<Vec<String>>>,
        start_err: bool,
        shutdown_err: bool,
    }

    impl FakeService {
        fn new(name: &str, is_source: bool, events: Arc<Mutex<Vec<String>>>) -> Self {
            Self {
                name: name.to_string(),
                is_source,
                notify: Arc::new(Notify::new()),
                events,
                start_err: false,
                shutdown_err: false,
            }
        }
    }

    #[async_trait]
    impl crate::service::Service for FakeService {
        fn svc(&self) -> crate::service::ServiceType {
            if self.is_source {
                crate::service::ServiceType::Source
            } else {
                crate::service::ServiceType::Sink
            }
        }

        async fn start(&self) -> anyhow::Result<JoinHandle<()>> {
            if self.start_err {
                return Err(anyhow::anyhow!("start failed for {}", self.name));
            }
            let notify = self.notify.clone();
            let events = self.events.clone();
            let name = self.name.clone();
            let handle = tokio::spawn(async move {
                events
                    .lock()
                    .unwrap()
                    .push(format!("{}:handle_started", name));
                // wait until shutdown() notifies us
                notify.notified().await;
                events
                    .lock()
                    .unwrap()
                    .push(format!("{}:handle_finished", name));
            });
            Ok(handle)
        }

        async fn shutdown(&self) -> anyhow::Result<()> {
            if self.shutdown_err {
                return Err(anyhow::anyhow!("shutdown failed for {}", self.name));
            }
            // record that shutdown() was called, and notify our running handle to finish
            self.events
                .lock()
                .unwrap()
                .push(format!("{}:shutdown_called", self.name));
            self.notify.notify_waiters();
            Ok(())
        }
    }

    /*
    Test: orchestrator_shutdown_ordering
    Goal: Verify that when shutting down, source services are shut down before sink services.
    The test starts one source and one sink service, ensures their background handles are running,
    calls shutdown_all(), and asserts the order of shutdown events and that a shutdown signal
    was sent on the provided channel.
    */
    #[tokio::test]
    async fn orchestrator_shutdown_ordering() {
        let events = Arc::new(Mutex::new(Vec::new()));

        // Create one source and one sink service
        let source = Box::new(FakeService::new("source", true, events.clone()));
        let sink = Box::new(FakeService::new("sink", false, events.clone()));

        // crossbeam channel to observe shutdown_tx send
        let (tx, rx) = crossbeam_channel::bounded::<()>(1);

        let token = ShutdownToken::new();
        let mut orch = Orchestrator::new(vec![sink, source], token, tx);

        // Start all services
        orch.start_all().await.expect("start_all ok");

        // Wait for both handles to be started to avoid races where shutdown() notifies before
        // the spawned task is awaiting the notification.
        timeout(Duration::from_millis(200), async {
            loop {
                let ev = events.lock().unwrap().clone();
                if ev.contains(&"source:handle_started".to_string())
                    && ev.contains(&"sink:handle_started".to_string())
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("handles started");

        // Trigger shutdown sequence and wait (bounded to avoid test hanging)
        timeout(Duration::from_secs(1), orch.shutdown_all())
            .await
            .expect("shutdown completed");

        // Small sleep to ensure all event writes are visible
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Assert event ordering:
        // - source:shutdown_called (shutdown of sources)
        // - source:handle_finished (handle finished after shutdown notified)
        // - sink:shutdown_called (sink shutdown occurs after shutdown_tx send)
        // - sink:handle_finished
        let ev = events.lock().unwrap().clone();
        // ignore handle_started events when asserting shutdown ordering
        let filtered: Vec<String> = ev
            .into_iter()
            .filter(|e| !e.ends_with(":handle_started"))
            .collect();
        assert_eq!(
            filtered,
            vec![
                "source:shutdown_called".to_string(),
                "source:handle_finished".to_string(),
                "sink:shutdown_called".to_string(),
                "sink:handle_finished".to_string(),
            ]
        );

        // Confirm that orchestrator sent a shutdown signal on the channel
        assert!(rx.try_recv().is_ok());
    }

    /*
    Test: orchestrator_initiate_triggers_token
    Goal: Ensure Orchestrator::initiate_shutdown triggers the shared ShutdownToken and
    notifies any waiters. The test spawns a task waiting on the token, initiates shutdown,
    and asserts the waiter was notified and that no channel send occurred because no
    services were present.
    */
    #[tokio::test]
    async fn orchestrator_initiate_triggers_token() {
        let token = ShutdownToken::new();
        let token_clone = token.clone();

        // spawn a task that waits on the token but first notify readiness via oneshot
        let notified = Arc::new(Mutex::new(false));
        let notified_clone = notified.clone();
        let (ready_tx, ready_rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            // signal that the waiter has started
            let _ = ready_tx.send(());
            token_clone.wait().await;
            *notified_clone.lock().unwrap() = true;
        });

        // wait for the spawned waiter to be ready before triggering
        let _ = ready_rx.await;

        let (_tx, rx) = crossbeam_channel::bounded::<()>(1);
        let mut orch = Orchestrator::new(Vec::new(), token, _tx);

        orch.initiate_shutdown().await;

        // wait a bit for the waiter to be notified
        tokio::time::sleep(Duration::from_millis(10)).await;

        assert!(*notified.lock().unwrap());
        // also ensure no send has been made on shutdown tx (no services were present)
        assert!(rx.try_recv().is_err());
    }

    /*
    Test: start_all_returns_err_if_service_start_fails
    Goal: Confirm that Orchestrator::start_all returns an error when any service's
    start() method fails. This ensures start errors are propagated to callers.
    */
    #[tokio::test]
    async fn start_all_returns_err_if_service_start_fails() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut bad = FakeService::new("bad", false, events.clone());
        bad.start_err = true;
        let bad_box = Box::new(bad);

        let (tx, _rx) = crossbeam_channel::bounded::<()>(1);
        let token = ShutdownToken::new();
        let mut orch = Orchestrator::new(vec![bad_box], token, tx);

        let res = orch.start_all().await;
        assert!(res.is_err());
    }
}
