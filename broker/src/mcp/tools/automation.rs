use std::time::Duration;

use rmcp::{Peer, RoleServer, handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::broadcast::error::RecvError;

use crate::{mcp::RoprobeMcp, server::SessionId};

#[derive(Deserialize, schemars::JsonSchema)]
pub struct JoinArgs {
    /// Place to launch via roblox:// protocol.
    place_id: u64,
    /// Seconds to wait for a session to connect (default 30).
    timeout: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SubArgs {
    /// logs or remotes.
    channel: String,
    /// Only this session id; defaults to all.
    session: Option<u32>,
    /// logs: level (print, info, warn, error). remotes: path substring.
    filter: Option<String>,
}

#[cfg(target_os = "windows")]
async fn launch(place_id: u64) -> Result<(), String> {
    std::process::Command::new("cmd")
        .args(["/C", "start", "", &format!("roblox://placeId={place_id}")])
        .spawn()
        .map(drop)
        .map_err(|e| format!("could not launch roblox: {e}"))
}

#[cfg(not(target_os = "windows"))]
async fn launch(_: u64) -> Result<(), String> {
    Err("join-game is windows only".into())
}

#[cfg(target_os = "windows")]
async fn roblox_pids() -> Result<Vec<u32>, String> {
    let out = tokio::task::spawn_blocking(|| {
        std::process::Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq RobloxPlayerBeta.exe", "/FO", "CSV", "/NH"])
            .output()
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("tasklist failed: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(text
        .lines()
        .filter_map(|l| l.split("\",\"").nth(1)?.parse().ok()) // csv by hand, trust me bro
        .collect())
}

#[cfg(not(target_os = "windows"))]
async fn roblox_pids() -> Result<Vec<u32>, String> {
    Err("volt-status is windows only".into())
}

#[tool_router(router = automation_router, vis = "pub(crate)")]
impl RoprobeMcp {
    #[tool(name = "join-game", description = "Launch a place via roblox:// and wait for a new session to connect (Windows).")]
    async fn join_game(&self, Parameters(a): Parameters<JoinArgs>) -> Result<String, String> {
        let mut rx = self.ctx.sessions.subscribe_session_added(); // subscribe first, race later
        launch(a.place_id).await?;
        let wait = Duration::from_secs(a.timeout.unwrap_or(30).into());
        let found = tokio::time::timeout(wait, async {
            loop {
                match rx.recv().await {
                    Ok(info) => return Some(info),
                    Err(RecvError::Lagged(_)) => continue,
                    Err(RecvError::Closed) => return None,
                }
            }
        })
        .await;
        match found {
            Ok(Some(info)) => serde_json::to_string(&info).map_err(|e| e.to_string()),
            Ok(None) => Err("session feed closed".into()),
            Err(_) => Err(format!("no session connected within {}s", wait.as_secs())),
        }
    }

    #[tool(name = "subscribe", description = "Stream logs or remotes as logging notifications (loggers roprobe.log, roprobe.remote). filter: log level or remote path substring.")]
    async fn subscribe(&self, peer: Peer<RoleServer>, Parameters(a): Parameters<SubArgs>) -> Result<String, String> {
        let only = a.session.map(SessionId);
        let sessions = &self.ctx.sessions;
        match a.channel.as_str() {
            "logs" => {
                if let Some(l) = a.filter.as_deref() {
                    if !["print", "info", "warn", "error"].iter().any(|x| x.eq_ignore_ascii_case(l)) {
                        return Err("log filter must be print, info, warn or error".into());
                    }
                }
                self.streams.watch_logs(peer, sessions, only, a.filter);
            }
            "remotes" => self.streams.watch_remotes(peer, sessions, only, a.filter),
            c => return Err(format!("channel must be logs or remotes, got {c}")),
        }
        Ok(format!("subscribed to {}", a.channel))
    }

    #[tool(name = "unsubscribe", description = "Stop a subscription.")]
    async fn unsubscribe(&self) -> Result<String, String> {
        Ok("not tracked yet; feeds clear on disconnect, resubscribing replaces the old one".into())
    }

    #[tool(name = "volt-status", description = "Running RobloxPlayerBeta.exe PIDs alongside connected sessions (Windows).")]
    async fn volt_status(&self) -> Result<String, String> {
        let pids = roblox_pids().await?;
        let sessions = self.ctx.sessions.holder.read().await.list();
        Ok(json!({"pids": pids, "sessions": sessions}).to_string())
    }
}
