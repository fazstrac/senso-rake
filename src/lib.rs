// Library facade to expose internal modules to integration tests and other crates.
// Keep modules public but preserve internal structure.

pub mod database;
pub mod http;
pub mod mqtt;
pub mod orchestrator;
pub mod server;
pub mod service;
pub mod shutdown_token;
