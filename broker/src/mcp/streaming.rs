#![allow(deprecated)] // logging notifs are all we've got

use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use rmcp::{
    Peer, RoleServer,
    model::{LoggingLevel, LoggingMessageNotificationParam},
};
use serde_json::{Value, json};
use tokio::{sync::broadcast::error::RecvError, task::JoinHandle};

use crate::{
    protocol::{HookEvent, LogEntry, RemoteCall, RemoteDirection, TraceCall},
    server::{SessionId, manager::Sessions},
};

/// Forwards hook and trace broadcasts to one MCP client.
#[derive(Default)]
pub struct Streams {
    watched: Arc<Mutex<HashSet<SessionId>>>,
    hooks: Mutex<Option<JoinHandle<()>>>,
    traces: Mutex<Option<JoinHandle<()>>>,
    logs: Mutex<Option<JoinHandle<()>>>,
    remotes: Mutex<Option<JoinHandle<()>>>,
}

fn alive(slot: &Mutex<Option<JoinHandle<()>>>) -> bool {
    slot.lock().unwrap().as_ref().is_some_and(|h| !h.is_finished())
}

async fn emit(peer: &Peer<RoleServer>, logger: &str, data: Value) -> bool {
    let mut p = LoggingMessageNotificationParam::new(LoggingLevel::Info, data);
    p.logger = Some(logger.into());
    peer.notify_logging_message(p).await.is_ok()
}

impl Streams {
    pub fn watch_hooks(&self, peer: Peer<RoleServer>, sessions: &Sessions, id: SessionId) {
        self.watched.lock().unwrap().insert(id);
        if alive(&self.hooks) {
            return;
        }
        let (mut rx, watched) = (sessions.subscribe_hooks(), self.watched.clone());
        let task = tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok((sid, ev)) => {
                        if !watched.lock().unwrap().contains(&sid) {
                            continue;
                        }
                        if !emit(&peer, "roprobe.hook", hook_json(sid, &ev)).await {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(_)) => continue,
                    Err(RecvError::Closed) => break,
                }
            }
        });
        *self.hooks.lock().unwrap() = Some(task);
    }

    pub fn watch_traces(&self, peer: Peer<RoleServer>, sessions: &Sessions, id: SessionId) {
        self.watched.lock().unwrap().insert(id);
        if alive(&self.traces) {
            return;
        }
        let (mut rx, watched) = (sessions.subscribe_traces(), self.watched.clone());
        let task = tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok((sid, call)) => {
                        if !watched.lock().unwrap().contains(&sid) {
                            continue;
                        }
                        if !emit(&peer, "roprobe.trace", trace_json(sid, &call)).await {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(_)) => continue,
                    Err(RecvError::Closed) => break,
                }
            }
        });
        *self.traces.lock().unwrap() = Some(task);
    }

    /// Console lines to the client; resubscribing replaces the old feed.
    pub fn watch_logs(&self, peer: Peer<RoleServer>, sessions: &Sessions, only: Option<SessionId>, level: Option<String>) {
        let mut rx = sessions.subscribe_log();
        let task = tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok((sid, entries)) => {
                        if only.is_some_and(|o| o != sid) {
                            continue;
                        }
                        for e in entries.iter().filter(|e| level_ok(e, level.as_deref())) {
                            if !emit(&peer, "roprobe.log", log_json(sid, e)).await {
                                return;
                            }
                        }
                    }
                    Err(RecvError::Lagged(_)) => continue,
                    Err(RecvError::Closed) => break,
                }
            }
        });
        swap_task(&self.logs, task);
    }

    /// Remote calls to the client; resubscribing replaces the old feed.
    pub fn watch_remotes(&self, peer: Peer<RoleServer>, sessions: &Sessions, only: Option<SessionId>, path: Option<String>) {
        let mut rx = sessions.subscribe_remotes();
        let task = tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok((sid, calls)) => {
                        if only.is_some_and(|o| o != sid) {
                            continue;
                        }
                        for c in calls.iter().filter(|c| path.as_deref().is_none_or(|p| c.remote.path.contains(p))) {
                            if !emit(&peer, "roprobe.remote", remote_json(sid, c)).await {
                                return;
                            }
                        }
                    }
                    Err(RecvError::Lagged(_)) => continue,
                    Err(RecvError::Closed) => break,
                }
            }
        });
        swap_task(&self.remotes, task);
    }
}

fn swap_task(slot: &Mutex<Option<JoinHandle<()>>>, task: JoinHandle<()>) {
    if let Some(old) = slot.lock().unwrap().replace(task) {
        old.abort(); // one feed per channel
    }
}

fn level_ok(e: &LogEntry, want: Option<&str>) -> bool {
    want.is_none_or(|w| format!("{:?}", e.level).eq_ignore_ascii_case(w))
}

fn log_json(sid: SessionId, e: &LogEntry) -> Value {
    json!({
        "session": sid.0,
        "level": format!("{:?}", e.level).to_ascii_lowercase(),
        "content": e.content,
    })
}

fn remote_json(sid: SessionId, c: &RemoteCall) -> Value {
    json!({
        "session": sid.0,
        "id": c.id,
        "dir": if c.direction == RemoteDirection::Outgoing { "out" } else { "in" },
        "remote": c.remote.path,
        "method": c.method,
        "args": c.arguments.len(),
        "script": c.source.script,
        "line": c.source.line,
        "t": c.timestamp,
    })
}

impl Drop for Streams {
    fn drop(&mut self) {
        for slot in [&self.hooks, &self.traces, &self.logs, &self.remotes] {
            if let Some(h) = slot.lock().unwrap().take() {
                h.abort();
            }
        }
    }
}

fn hook_json(sid: SessionId, e: &HookEvent) -> Value {
    json!({
        "session": sid.0,
        "remote": e.remote.path,
        "class": e.remote.class,
        "args": e.args,
        "returns": e.returns,
        "caller": e.caller,
        "t": e.timestamp,
    })
}

fn trace_json(sid: SessionId, c: &TraceCall) -> Value {
    json!({
        "session": sid.0,
        "target": c.target,
        "args": c.args,
        "returns": c.returns,
        "t": c.timestamp,
    })
}
