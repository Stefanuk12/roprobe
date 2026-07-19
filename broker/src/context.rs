use std::sync::Arc;

use tokio::sync::Notify;

use crate::{lockfile::Lockfile, server::manager::Sessions, upstream::Controls};

/// A global context used throughout the broker.
#[derive(Clone, Debug)]
pub struct Context {
    pub sessions: Sessions,
    pub lockfile: Lockfile,
    pub controls: Controls,
    pub shutdown: Arc<Notify>,
}

impl Context {
    pub fn new(
        sessions: Sessions,
        lockfile: Lockfile,
        controls: Controls,
        shutdown: Arc<Notify>,
    ) -> Self {
        Self {
            sessions,
            lockfile,
            controls,
            shutdown,
        }
    }
}
