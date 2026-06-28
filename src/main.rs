use log::info;

use tokio;
use senso_rake::server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    info!("Starting service");

    server::run().await
}
