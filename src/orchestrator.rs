use crate::{service::Service, shutdown_token::ShutdownToken};
use tokio::task::JoinHandle;


pub struct Orchestrator {
    services_p1: Vec<Box<dyn Service>>,
    services_p2: Vec<Box<dyn Service>>,
    handles_p1: Vec<JoinHandle<()>>,
    handles_p2: Vec<JoinHandle<()>>,
    shutdown_token: ShutdownToken,
    shutdown_tx: crossbeam_channel::Sender<()>,
}

impl Orchestrator {
    pub fn new(services_p1: Vec<Box<dyn Service>>, services_p2: Vec<Box<dyn Service>>, shutdown_token: ShutdownToken, shutdown_tx: crossbeam_channel::Sender<()>) -> Self {
        Self {
            services_p1,
            services_p2,
            handles_p1: Vec::new(),
            handles_p2: Vec::new(),
            shutdown_token,
            shutdown_tx,
        }
    }

    // Start all services
    // Note the order: phase 2 services (e.g., DB) are started before phase 1 services (e.g., MQTT, HTTP)
    pub async fn start_all(&mut self) -> anyhow::Result<()> {        
        for svc in &self.services_p2 {
            let handle = svc.start().await?;
            self.handles_p2.push(handle);
        }

        for svc in &self.services_p1 {
            let handle = svc.start().await?;
            self.handles_p1.push(handle);
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
        // PHASE1 services first

        for svc in &self.services_p1 {
            let _ = svc.shutdown().await;
        }
        for handle in self.handles_p1.drain(..) {
            let _ = handle.await;
        }

        // Then PHASE2 services

        // Start by sending shutdown signal to DB worker
        let _ = self.shutdown_tx.send(());

        for svc in &self.services_p2 {
            let _ = svc.shutdown().await;
        }
        for handle in self.handles_p2.drain(..) {
            let _ = handle.await;
        }
    }
}
