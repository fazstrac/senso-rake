pub enum ServiceType {
    Source,
    Sink,
}

#[async_trait::async_trait]
pub trait Service: Send + Sync {
    fn svc(&self) -> ServiceType;
    async fn start(&self) -> anyhow::Result<tokio::task::JoinHandle<()>>;
    async fn shutdown(&self) -> anyhow::Result<()> {
        // Default implementation does nothing
        Ok(())
    }
}
