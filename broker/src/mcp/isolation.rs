use std::collections::HashMap;

use tokio::sync::RwLock;

use crate::server::SessionId;

#[derive(Default)]
pub struct LockTable {
    locks: RwLock<HashMap<SessionId, String>>, // session -> agent
}

impl LockTable {
    pub async fn lock(&self, session: SessionId, agent: &str) -> Result<(), String> {
        let mut locks = self.locks.write().await;
        if let Some(owner) = locks.get(&session).filter(|o| *o != agent) {
            return Err(format!("session locked by agent {owner}"));
        }
        locks.insert(session, agent.to_string());
        Ok(())
    }

    pub async fn unlock(&self, session: SessionId, agent: &str) -> Result<(), String> {
        let mut locks = self.locks.write().await;
        match locks.get(&session) {
            Some(owner) if owner != agent => Err(format!("locked by {owner}, not you")),
            Some(_) => {
                locks.remove(&session);
                Ok(())
            }
            None => Ok(()),
        }
    }

    pub async fn check(&self, session: SessionId, agent: &str) -> Result<(), String> {
        match self.locks.read().await.get(&session) {
            Some(owner) if owner != agent => Err(format!("session locked by agent {owner}")),
            _ => Ok(()),
        }
    }

    pub async fn clear_agent(&self, agent: &str) {
        self.locks.write().await.retain(|_, v| v != agent);
    }

    pub async fn clear_session(&self, session: SessionId) {
        self.locks.write().await.remove(&session);
    }

    pub async fn get_owner(&self, session: SessionId) -> Option<String> {
        self.locks.read().await.get(&session).cloned()
    }
}
