mod legacy_assignments;
mod service;

pub mod schema; // Needed for integration tests
pub use legacy_assignments::DuckDBLegacyAssignmentRepository;
pub use service::DbHandle;
pub use service::DbService;

// Re-exported for tests
#[allow(unused_imports)]
pub use service::DbCommand;
#[allow(unused_imports)]
pub use service::DbJob;
#[allow(unused_imports)]
pub use service::DbResponse;
