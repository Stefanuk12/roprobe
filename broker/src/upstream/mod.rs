// `import!` is avoided here: both submodules export `maintain` and
// `DEFAULT_PORT`, so glob re-exports would be ambiguous.
pub mod luau_lsp;
pub mod verde;

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tokio::sync::watch;

/// The local tools the broker maintains outbound connections to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Upstream {
    Verde,
    LuauLsp,
}

/// The switch states as set by flags/clients, before considering sessions.
struct Wanted {
    sessions: usize,
    verde: bool,
    luau_lsp: bool,
}

/// Runtime switches for the upstream connection tasks.
///
/// An upstream task only runs while its switch is on AND at least one
/// client session is active — there's no point holding connections
/// that nobody is brokering for.
pub struct Controls {
    wanted: Mutex<Wanted>,
    verde: watch::Sender<bool>,
    luau_lsp: watch::Sender<bool>,
}

impl Controls {
    /// Create the switches with their starting states.
    /// Both start inactive: no client sessions exist yet.
    pub fn new(verde: bool, luau_lsp: bool) -> Self {
        Self {
            wanted: Mutex::new(Wanted {
                sessions: 0,
                verde,
                luau_lsp,
            }),
            verde: watch::channel(false).0,
            luau_lsp: watch::channel(false).0,
        }
    }

    /// Create the receiver a `maintain` task listens on.
    pub fn subscribe(&self, upstream: Upstream) -> watch::Receiver<bool> {
        match upstream {
            Upstream::Verde => self.verde.subscribe(),
            Upstream::LuauLsp => self.luau_lsp.subscribe(),
        }
    }

    /// Enable or disable an upstream. Its task reacts immediately:
    /// disabling drops any live connection, enabling resumes retrying
    /// (once a client session is active).
    pub fn set(&self, upstream: Upstream, enabled: bool) {
        let mut wanted = self.wanted.lock().expect("controls lock");
        match upstream {
            Upstream::Verde => wanted.verde = enabled,
            Upstream::LuauLsp => wanted.luau_lsp = enabled,
        }
        self.apply(&wanted);
    }

    /// Count a client session for as long as the returned guard lives.
    #[must_use = "the session is only counted while the guard is alive"]
    pub fn track_session(&self) -> SessionGuard<'_> {
        let mut wanted = self.wanted.lock().expect("controls lock");
        wanted.sessions += 1;
        self.apply(&wanted);
        SessionGuard(self)
    }

    /// Push `wanted && has sessions` out to the tasks.
    fn apply(&self, wanted: &Wanted) {
        let active = wanted.sessions > 0;
        self.verde.send_replace(active && wanted.verde);
        self.luau_lsp.send_replace(active && wanted.luau_lsp);
    }
}

/// Keeps a client session counted until dropped.
pub struct SessionGuard<'a>(&'a Controls);

impl Drop for SessionGuard<'_> {
    fn drop(&mut self) {
        let mut wanted = self.0.wanted.lock().expect("controls lock");
        wanted.sessions -= 1;
        self.0.apply(&wanted);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn upstreams_only_activate_while_a_session_exists() {
        let controls = Controls::new(true, false);
        let mut verde = controls.subscribe(Upstream::Verde);
        let mut luau_lsp = controls.subscribe(Upstream::LuauLsp);

        // Wanted, but no sessions yet.
        assert!(!*verde.borrow_and_update());

        let first = controls.track_session();
        assert!(*verde.borrow_and_update());
        assert!(!*luau_lsp.borrow_and_update());

        // Toggling while a session is active applies immediately.
        controls.set(Upstream::LuauLsp, true);
        assert!(*luau_lsp.borrow_and_update());
        controls.set(Upstream::Verde, false);
        assert!(!*verde.borrow_and_update());
        controls.set(Upstream::Verde, true);

        // Still one session left: stays active.
        let second = controls.track_session();
        drop(first);
        assert!(*verde.borrow_and_update());

        // Last session gone: everything deactivates, wanted state kept.
        drop(second);
        assert!(!*verde.borrow_and_update());
        assert!(!*luau_lsp.borrow_and_update());

        // A new session restores the wanted state.
        let _third = controls.track_session();
        assert!(*verde.borrow_and_update());
        assert!(*luau_lsp.borrow_and_update());
    }
}
