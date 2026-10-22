use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;
use serde_json::json;

use crate::mcp::RoprobeMcp;

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ScreenshotArgs {
    /// Roblox PID to capture; defaults to any Roblox window.
    pid: Option<u32>,
}

#[cfg(target_os = "windows")]
fn capture(pid: Option<u32>) -> Result<String, String> {
    use base64::Engine as _;

    let script = include_str!("../../scripts/screenshot.ps1");
    let utf16: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let encoded = base64::engine::general_purpose::STANDARD.encode(utf16);

    let mut cmd = std::process::Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-EncodedCommand", &encoded]);
    if let Some(p) = pid {
        cmd.env("ROPROBE_PID", p.to_string());
    }

    let out = cmd.output().map_err(|e| format!("could not run powershell: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() { "screenshot failed".into() } else { err });
    }
    let b64 = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if b64.is_empty() {
        return Err("screenshot produced no image".into());
    }
    Ok(b64)
}

#[cfg(not(target_os = "windows"))]
fn capture(_: Option<u32>) -> Result<String, String> {
    Err("screenshot is windows only".into())
}

#[tool_router(router = screenshot_router, vis = "pub(crate)")]
impl RoprobeMcp {
    #[tool(name = "screenshot", description = "Capture a Roblox window as base64 JPEG (Windows). pid: target a specific process.")]
    async fn screenshot(&self, Parameters(a): Parameters<ScreenshotArgs>) -> Result<String, String> {
        let b64 = tokio::task::spawn_blocking(move || capture(a.pid))
            .await
            .map_err(|e| e.to_string())??;
        Ok(json!({"image": b64}).to_string())
    }
}
