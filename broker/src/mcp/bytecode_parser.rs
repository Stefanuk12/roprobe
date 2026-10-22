use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};

const PREVIEW: usize = 64;

// skeleton: real decoder comes later, probably
pub fn summarize(payload: &str) -> Result<Value, String> {
    let v: Value = serde_json::from_str(payload).map_err(|e| format!("bad bytecode reply: {e}"))?;
    let b64 = v["base64"].as_str().ok_or("reply has no base64")?;
    let bytes = STANDARD.decode(b64).map_err(|e| format!("bad base64: {e}"))?;
    let version = *bytes.first().ok_or("empty bytecode")?;
    if version == 0 {
        let msg = String::from_utf8_lossy(&bytes[1..]).into_owned();
        return Err(format!("compile error: {msg}"));
    }
    let head = &bytes[..bytes.len().min(PREVIEW)];
    Ok(json!({
        "script": v["script_path"],
        "version": version,
        "size": bytes.len(),
        "preview": STANDARD.encode(head),
        "base64": b64,
    }))
}
