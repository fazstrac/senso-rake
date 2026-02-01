mod service;

pub mod schema; // Needed for integration tests
pub use service::DbHandle;
pub use service::DbService;
