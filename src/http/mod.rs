mod service;

pub use service::HttpService;

// Re-exported for tests
#[allow(unused_imports)]
pub use service::{SensorMapping, build_router};
