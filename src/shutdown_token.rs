use std::sync::Arc;
use tokio::sync::Notify;

#[derive(Clone)]
pub struct ShutdownToken {
    notify: Arc<Notify>,
}

// To make clippy happy
impl Default for ShutdownToken {
    fn default() -> Self {
        Self::new()
    }
}

impl ShutdownToken {
    pub fn new() -> Self {
        Self {
            notify: Arc::new(Notify::new()),
        }
    }

    pub fn trigger(&self) {
        self.notify.notify_waiters();
    }

    pub async fn wait(&self) {
        self.notify.notified().await;
    }
}
