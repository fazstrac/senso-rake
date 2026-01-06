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
    pub fn new(services: Vec<Box<dyn Service>>, shutdown_token: ShutdownToken, shutdown_tx: crossbeam_channel::Sender<()>) -> Self {
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

        for svc in self.services.iter().filter(|s| matches!(s.svc(), crate::service::ServiceType::Sink)) {
             let handle = svc.start().await?;
             self.sink_handles.push(handle);
        }

        for svc in self.services.iter().filter(|s| matches!(s.svc(), crate::service::ServiceType::Source)) {
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

        for svc in self.services.iter().filter(|s| matches!(s.svc(), crate::service::ServiceType::Source)) {
            let _ = svc.shutdown().await;
            let handle = self.source_handles.pop();
            if let Some(h) = handle {
                let _ = h.await;
            }
        }

        // Start by sending shutdown signal to DB worker
        let _ = self.shutdown_tx.send(());


        for svc in self.services.iter().filter(|s| matches!(s.svc(), crate::service::ServiceType::Sink)) {
            let _ = svc.shutdown().await;
            let handle = self.sink_handles.pop();
            if let Some(h) = handle {
                let _ = h.await;
            }
        }    
    }
}
