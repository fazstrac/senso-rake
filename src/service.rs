#[async_trait::async_trait]
pub trait Service: Send + Sync {
    async fn start(&self) -> anyhow::Result<tokio::task::JoinHandle<()>>;
    async fn shutdown(&self) -> anyhow::Result<()>;
}
