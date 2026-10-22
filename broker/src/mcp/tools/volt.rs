use std::time::Duration;

use rmcp::{Peer, RoleServer, handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::broadcast::error::RecvError;

use crate::mcp::RoprobeMcp;

const VOLT_DB: &str = concat!(env!("LOCALAPPDATA"), r"\Volt\state.db");

#[derive(Deserialize, schemars::JsonSchema)]
pub struct VoltLaunchArgs {
    /// Account username or userId to launch.
    account: String,
    /// Roblox place ID to join.
    place_id: u64,
    /// Seconds to wait for session to connect (default 60).
    timeout: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct VoltLaunchAllArgs {
    /// Roblox place ID to join.
    place_id: u64,
    /// Delay in seconds between each launch (default 5).
    delay: Option<u32>,
    /// Seconds to wait for all sessions to connect (default 120).
    timeout: Option<u32>,
}

// volt state.db reader
mod db {
    use serde::Deserialize;
    use serde_json::Value;

    #[derive(Debug, Deserialize)]
    pub struct AccountsDoc {
        pub accounts: Vec<Account>,
        pub settings: Option<Settings>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Account {
        pub id: String,
        pub user_id: u64,
        pub username: String,
        pub display_name: String,
        pub alias: String,
        pub cookie_status: String,
        pub encrypted_cookie: String,
        pub auto_relaunch: bool,
        #[serde(default)]
        pub assigned_private_server_id: String,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Settings {
        pub default_place_id: Option<String>,
        pub launch_delay_ms: Option<u64>,
    }

    pub fn read_accounts(db_path: &str) -> Result<AccountsDoc, String> {
        let conn = rusqlite_or_raw(db_path)?;
        let raw: Vec<u8> = conn.0;
        serde_json::from_slice(&raw).map_err(|e| format!("parse accounts: {e}"))
    }

    // minimal sqlite reader — just grab the 'accounts' blob
    fn rusqlite_or_raw(path: &str) -> Result<(Vec<u8>,), String> {
        // shell out to python since we don't have rusqlite
        let out = std::process::Command::new("python")
            .args([
                "-c",
                &format!(
                    "import sqlite3,sys;c=sqlite3.connect(r'{path}');r=c.execute(\"SELECT value FROM state_documents WHERE name='accounts'\").fetchone();sys.stdout.buffer.write(r[0] if r else b'{{}}')"
                ),
            ])
            .output()
            .map_err(|e| format!("python: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "sqlite read failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        Ok((out.stdout,))
    }

    /// Extract the credential target name from encryptedCookie field
    pub fn cred_target(encrypted: &str) -> Option<String> {
        // format: "cred:secret/UUID"
        let uuid = encrypted.strip_prefix("cred:secret/")?;
        Some(format!("com.volt.editor/secret/{uuid}"))
    }
}

/// Launch via Volt's CDP (Tauri IPC). Goes through Volt's full pipeline.
#[cfg(target_os = "windows")]
async fn volt_cdp_launch(
    account_ids: &[String],
    place_id: u64,
) -> Result<serde_json::Value, String> {
    let script = include_str!("../../scripts/volt_launch.ps1");
    let encoded = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        script.encode_utf16().flat_map(|c| c.to_le_bytes()).collect::<Vec<u8>>(),
    );
    let ids_csv = account_ids.join(",");
    let place = place_id.to_string();
    let out = tokio::task::spawn_blocking(move || {
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-EncodedCommand", &encoded])
            .env("VOLT_ACCOUNT_IDS", &ids_csv)
            .env("VOLT_PLACE_ID", &place)
            .output()
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("powershell: {e}"))?;

    let text = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !stderr.is_empty() {
        tracing::warn!("volt_launch stderr: {stderr}");
    }
    serde_json::from_str(text.trim())
        .map_err(|e| format!("parse: {e}\nstdout: {text}\nstderr: {stderr}"))
}

#[cfg(not(target_os = "windows"))]
async fn volt_cdp_launch(_: &[String], _: u64) -> Result<serde_json::Value, String> {
    Err("volt-launch is windows only".into())
}

use base64::Engine as _;

#[tool_router(router = volt_router, vis = "pub(crate)")]
impl RoprobeMcp {
    #[tool(
        name = "volt-accounts",
        description = "List Roblox accounts stored in Volt with their status."
    )]
    async fn volt_accounts(&self) -> Result<String, String> {
        let doc = tokio::task::spawn_blocking(|| db::read_accounts(VOLT_DB))
            .await
            .map_err(|e| e.to_string())??;

        let accounts: Vec<serde_json::Value> = doc
            .accounts
            .iter()
            .map(|a| {
                json!({
                    "username": a.username,
                    "userId": a.user_id,
                    "displayName": a.display_name,
                    "alias": a.alias,
                    "status": a.cookie_status,
                    "autoRelaunch": a.auto_relaunch,
                })
            })
            .collect();

        let default_place = doc
            .settings
            .as_ref()
            .and_then(|s| s.default_place_id.as_deref());
        Ok(json!({"accounts": accounts, "defaultPlaceId": default_place}).to_string())
    }

    #[tool(
        name = "volt-launch",
        description = "Launch a Volt account into a Roblox place. Uses stored credentials to authenticate and join."
    )]
    async fn volt_launch(
        &self,
        Parameters(a): Parameters<VoltLaunchArgs>,
    ) -> Result<String, String> {
        let doc = tokio::task::spawn_blocking(|| db::read_accounts(VOLT_DB))
            .await
            .map_err(|e| e.to_string())??;

        let acct = doc
            .accounts
            .iter()
            .find(|x| {
                x.username.eq_ignore_ascii_case(&a.account)
                    || x.user_id.to_string() == a.account
                    || x.alias.eq_ignore_ascii_case(&a.account)
            })
            .ok_or_else(|| format!("account '{}' not found in Volt", a.account))?;

        if acct.cookie_status != "alive" {
            return Err(format!(
                "account {} cookie is '{}', not alive",
                acct.username, acct.cookie_status
            ));
        }

        let result = volt_cdp_launch(&[acct.id.clone()], a.place_id).await?;
        if let Some(err) = result.get("error") {
            return Err(err.as_str().unwrap_or("unknown").to_string());
        }

        // wait for session to connect
        let mut rx = self.ctx.sessions.subscribe_session_added();
        let wait = Duration::from_secs(a.timeout.unwrap_or(60).into());
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
            Ok(Some(info)) => Ok(json!({
                "launched": acct.username,
                "placeId": a.place_id,
                "session": info,
            })
            .to_string()),
            Ok(None) => Err("session feed closed".into()),
            Err(_) => {
                // launched but no session yet — might still be loading
                Ok(json!({
                    "launched": acct.username,
                    "placeId": a.place_id,
                    "session": null,
                    "note": "launched but no session connected yet"
                })
                .to_string())
            }
        }
    }

    #[tool(
        name = "volt-launch-all",
        description = "Launch ALL alive Volt accounts into a place, with a delay between each."
    )]
    async fn volt_launch_all(
        &self,
        Parameters(a): Parameters<VoltLaunchAllArgs>,
    ) -> Result<String, String> {
        let doc = tokio::task::spawn_blocking(|| db::read_accounts(VOLT_DB))
            .await
            .map_err(|e| e.to_string())??;

        let alive: Vec<_> = doc
            .accounts
            .iter()
            .filter(|x| x.cookie_status == "alive")
            .collect();

        if alive.is_empty() {
            return Err("no alive accounts in Volt".into());
        }

        // launch all at once via CDP — Volt handles delay internally
        let ids: Vec<String> = alive.iter().map(|a| a.id.clone()).collect();
        let usernames: Vec<&str> = alive.iter().map(|a| a.username.as_str()).collect();

        let result = volt_cdp_launch(&ids, a.place_id).await;
        let launched: Vec<serde_json::Value> = usernames.iter().map(|u| json!({"username": u})).collect();
        let failed: Vec<serde_json::Value> = vec![];

        if let Err(e) = &result {
            return Err(format!("launch failed: {e}"));
        }

        Ok(json!({
            "launched": launched,
            "failed": failed,
            "placeId": a.place_id,
            "note": format!("waiting for sessions to connect via autoexec")
        })
        .to_string())
    }
}
