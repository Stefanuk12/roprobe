use std::{
    collections::VecDeque,
    ops::Deref,
    sync::{Arc, Mutex, atomic::Ordering},
    time::Duration,
};

use tokio::sync::{RwLock, broadcast, watch};
use tracing::debug;

use crate::{
    protocol::{LogEntry, RemoteCall, SessionInfo, SpyConfig},
    server::{DomChange, Session, SessionDom, SessionId},
};

const LOG_BUFFER: usize = 256;
const LOG_HISTORY: usize = 1_000;

const REMOTE_BUFFER: usize = 256;
const REMOTE_HISTORY: usize = 5_000;

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

    /// Describe one [`Session`], by its [`SessionId`].
    pub fn info(&self, id: SessionId) -> Option<SessionInfo> {
        self.find(id).map(|s| SessionInfo {
            id: s.id,
            username: s.username.clone(),
            peer: s.peer.to_string(),
            active: *self.current.borrow() == Some(s.id),
            security_level: s.security_level.load(Ordering::Relaxed),
        })
    }

    /// List all of the current session data.
    pub fn list(&self) -> Vec<SessionInfo> {
        self.store
            .iter()
            .filter_map(|s| self.info(s.id))
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
#[derive(Clone, Debug)]
pub struct Sessions {
    pub holder: Arc<RwLock<SessionsHolder>>,
    on_session_added: Arc<broadcast::Sender<SessionInfo>>,
    on_session_removed: Arc<broadcast::Sender<SessionId>>,
    on_log: Arc<broadcast::Sender<(SessionId, Vec<LogEntry>)>>,
    history: Arc<Mutex<VecDeque<(SessionId, LogEntry)>>>,
    on_remotes: Arc<broadcast::Sender<(SessionId, Vec<RemoteCall>)>>,
    on_remotes_cleared: Arc<broadcast::Sender<()>>,
    remotes: Arc<Mutex<VecDeque<(SessionId, RemoteCall)>>>,
    spy: Arc<Mutex<SpyConfig>>,
}

impl Default for Sessions {
    fn default() -> Self {
        Self::new(Default::default())
    }
}

impl Sessions {
    /// Create a new [`Sessions`] wrapper.
    pub fn new(holder: SessionsHolder) -> Self {
        Self {
            holder: Arc::new(RwLock::new(holder)),
            on_session_added: Arc::new(broadcast::channel(16).0),
            on_session_removed: Arc::new(broadcast::channel(16).0),
            on_log: Arc::new(broadcast::channel(LOG_BUFFER).0),
            history: Arc::new(Mutex::new(VecDeque::with_capacity(LOG_HISTORY))),
            on_remotes: Arc::new(broadcast::channel(REMOTE_BUFFER).0),
            on_remotes_cleared: Arc::new(broadcast::channel(16).0),
            remotes: Arc::new(Mutex::new(VecDeque::with_capacity(REMOTE_HISTORY))),
            spy: Arc::new(Mutex::new(SpyConfig::default())),
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

    pub async fn insert(&self, session: Session) {
        let id = session.id;
        let mut holder = self.holder.write().await;
        holder.insert(session);
        let info = holder.info(id).expect("the session we just inserted");
        drop(holder);

        let _ = self.on_session_added.send(info);
    }

    pub async fn remove(&self, id: SessionId) {
        self.holder.write().await.remove(id);
        let _ = self.on_session_removed.send(id);
    }

    /// Fan one session's console batch out to the control connections.
    pub fn broadcast_log(&self, id: SessionId, entries: Vec<LogEntry>) {
        let lines = entries.len();

        {
            let mut history = self.history.lock().expect("log history poisoned");
    
            for entry in &entries {
                if history.len() == LOG_HISTORY {
                    history.pop_front();
                }
    
                history.push_back((id, entry.clone()));
            }
        }

        match self.on_log.send((id, entries)) {
            Ok(controls) => debug!(id = id.0, lines, controls, "console batch broadcast"),
            Err(_) => debug!(id = id.0, lines, "console batch dropped, no control connection"),
        }
    }

    pub fn subscribe_log(&self) -> broadcast::Receiver<(SessionId, Vec<LogEntry>)> {
        self.on_log.subscribe()
    }

    /// All of the past console logs for each session, in ascending order.
    pub fn log_history(&self) -> Vec<(SessionId, Vec<LogEntry>)> {
        let history = self.history.lock().expect("log history poisoned");

        let mut batches: Vec<(SessionId, Vec<LogEntry>)> = Vec::new();
        for (id, entry) in history.iter() {
            match batches.last_mut() {
                Some((last, entries)) if last == id => entries.push(entry.clone()),
                _ => batches.push((*id, vec![entry.clone()])),
            }
        }
        batches
    }

    /// Fan one session's captured remote calls out to the control connections.
    pub fn broadcast_remotes(&self, id: SessionId, calls: Vec<RemoteCall>) {
        let captured = calls.len();

        {
            let mut remotes = self.remotes.lock().expect("remote history poisoned");

            for call in &calls {
                if remotes.len() == REMOTE_HISTORY {
                    remotes.pop_front();
                }

                remotes.push_back((id, call.clone()));
            }
        }

        match self.on_remotes.send((id, calls)) {
            Ok(controls) => debug!(id = id.0, captured, controls, "remote batch broadcast"),
            Err(_) => debug!(id = id.0, captured, "remote batch buffered, no control connection"),
        }
    }

    pub fn subscribe_remotes(&self) -> broadcast::Receiver<(SessionId, Vec<RemoteCall>)> {
        self.on_remotes.subscribe()
    }

    /// Every buffered remote call, grouped into runs by the session that made it.
    pub fn remote_history(&self) -> Vec<(SessionId, Vec<RemoteCall>)> {
        let remotes = self.remotes.lock().expect("remote history poisoned");

        let mut batches: Vec<(SessionId, Vec<RemoteCall>)> = Vec::new();
        for (id, call) in remotes.iter() {
            match batches.last_mut() {
                Some((last, calls)) if last == id => calls.push(call.clone()),
                _ => batches.push((*id, vec![call.clone()])),
            }
        }
        batches
    }

    /// Drop the buffered remote calls, answering with how many went.
    pub fn clear_remotes(&self) -> usize {
        let mut remotes = self.remotes.lock().expect("remote history poisoned");
        let dropped = remotes.len();
        remotes.clear();
        dropped
    }

    /// Tell every control connection the history is gone, so none keeps showing
    /// calls the broker can no longer replay.
    pub fn broadcast_remotes_cleared(&self) {
        let _ = self.on_remotes_cleared.send(());
    }

    pub fn subscribe_remotes_cleared(&self) -> broadcast::Receiver<()> {
        self.on_remotes_cleared.subscribe()
    }

    /// What sessions are currently told to capture.
    pub fn spy(&self) -> SpyConfig {
        *self.spy.lock().expect("spy config poisoned")
    }

    /// Change what sessions capture, for the ones connected now and any later.
    pub fn set_spy(&self, config: SpyConfig) {
        *self.spy.lock().expect("spy config poisoned") = config;
    }

    pub fn subscribe_session_added(&self) -> broadcast::Receiver<SessionInfo> {
        self.on_session_added.subscribe()
    }

    pub fn subscribe_session_removed(&self) -> broadcast::Receiver<SessionId> {
        self.on_session_removed.subscribe()
    }
}

impl Deref for Sessions {
    type Target = RwLock<SessionsHolder>;

    fn deref(&self) -> &Self::Target {
        &self.holder
    }
}
