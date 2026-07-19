use std::{ops::Deref, sync::Arc, time::Duration};
use tokio::{
    sync::{RwLock, broadcast, watch},
    time,
};

use crate::{
    protocol::SessionInfo,
    server::{DomChange, Session, SessionDom, SessionId},
};

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("id could not be found in sessions store.")]
    InvalidId,
}

/// Holds all of the current sessions, and keeps track of the current active session.
#[derive(Debug, Default)]
pub struct SessionsHolder {
    store: Vec<Session>,
    current: watch::Sender<Option<SessionId>>,
}

impl SessionsHolder {
    /// Create a new [`SessionsHolder`]
    pub fn new(store: Vec<Session>) -> Self {
        Self {
            store,
            current: watch::channel(None).0,
        }
    }

    /// Grab the current active [`Session`].
    pub fn current(&self) -> Option<&Session> {
        let id = (*self.current.borrow())?;
        self.find(id)
    }

    /// Find a [`Session`] within the session store, by its [`SessionId`].
    pub fn find(&self, id: SessionId) -> Option<&Session> {
        self.store.iter().find(|s| s.id == id)
    }

    /// Find a [`Session`] within the session store, by its [`SessionId`] (mutably).
    pub fn find_mut(&mut self, id: SessionId) -> Option<&mut Session> {
        self.store.iter_mut().find(|s| s.id == id)
    }

    /// Add a new [`Session`] into the session store, and automatically set as current if it's the first one.
    pub fn insert(&mut self, session: Session) {
        let id = session.id;
        self.store.push(session);

        if self.store.len() == 1 {
            // NOTE: this is infallable because we just added current in
            let _ = self.set_current(Some(id));
        }
    }

    /// Remove a [`Session`] from the session store, by its [`SessionId`].
    pub fn remove(&mut self, id: SessionId) -> Option<Session> {
        let i = self.store.iter().position(|s| s.id == id)?;
        if *self.current.borrow() == Some(id) {
            self.current.send_replace(None);
        }
        Some(self.store.remove(i))
    }

    /// List all of the current session data.
    pub fn list(&self) -> Vec<SessionInfo> {
        self.store
            .iter()
            .map(|s| SessionInfo {
                id: s.id,
                peer: s.peer.to_string(),
                active: *self.current.borrow() == Some(s.id),
            })
            .collect()
    }

    /// Set the current active [`Session`], identified by its [`SessionId`], or to [`None`].
    ///
    /// Notifies any listeners on the current active session.
    pub fn set_current(
        &mut self,
        id: Option<SessionId>,
    ) -> Result<Option<SessionId>, SessionError> {
        if *self.current.borrow() == id {
            return Ok(id);
        }

        if let Some(id) = id {
            self.find(id).ok_or(SessionError::InvalidId)?;
        }

        self.current.send_replace(id);
        Ok(id)
    }

    pub fn subscribe_current(&self) -> watch::Receiver<Option<SessionId>> {
        self.current.subscribe()
    }
}

/// A cloneable wrapper of the session store ([`SessionsHolder`]).
#[derive(Clone, Debug, Default)]
pub struct Sessions {
    pub holder: Arc<RwLock<SessionsHolder>>,
}

impl Sessions {
    /// Create a new [`Sessions`] wrapper.
    pub fn new(holder: SessionsHolder) -> Self {
        Self {
            holder: Arc::new(RwLock::new(holder)),
        }
    }

    /// Forwarded to [`SessionsHolder::subscribe_current`].
    pub async fn subscribe_current(&self) -> watch::Receiver<Option<SessionId>> {
        self.holder.read().await.subscribe_current()
    }

    /// Forwarded to [`SessionsHolder::current`].
    pub async fn current(&self) -> watch::Sender<Option<SessionId>> {
        self.holder.read().await.current.clone()
    }

    /// Forwarded to [`SessionsHolder::set_current`].
    pub async fn set_current(
        &self,
        id: Option<SessionId>,
    ) -> Result<Option<SessionId>, SessionError> {
        self.holder.write().await.set_current(id)
    }

    /// Perform an operation on the current dom.
    pub async fn with_dom<R>(&self, read: impl FnOnce(&SessionDom) -> R) -> Option<R> {
        let holder = self.holder.read().await;
        let session = holder.current()?;
        let mirror = &session.mirror;
        Some(mirror.with_dom(read))
    }

    /// Subscribe to the current session's DOM and its changes.
    pub async fn subscribe_current_dom<R>(
        &self,
        read: impl FnOnce(&SessionDom) -> R,
    ) -> Option<(SessionId, broadcast::Receiver<Arc<DomChange>>, R)> {
        let holder = self.holder.read().await;
        let session = holder.current()?;
        let mirror = &session.mirror;

        let (rx, dom) = mirror.subscribe_with(read);
        Some((session.id, rx, dom))
    }

    /// Wait until the current dom has some nodes.
    pub async fn wait_current_dom_populated(&self, timeout: Duration) -> Option<usize> {
        let holder = self.holder.read().await;
        let mirror = holder.current()?.mirror.clone();
        drop(holder);
        mirror.wait_population(timeout, 0).await
    }
}

impl Deref for Sessions {
    type Target = RwLock<SessionsHolder>;

    fn deref(&self) -> &Self::Target {
        &self.holder
    }
}
