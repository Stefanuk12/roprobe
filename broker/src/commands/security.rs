use std::io;

use futures_util::SinkExt;
use tracing::info;

use crate::{
    SecurityLevel,
    commands::{CommandResult, connect_broker},
    protocol::ClientMessage,
    server::SessionId,
};

pub async fn security(id: SessionId, level: SecurityLevel) -> CommandResult {
    // Connect to the broker and set the session's security level.
    let mut ws = connect_broker().await?;
    ws.send(
        ClientMessage::SetSecurity {
            id,
            level: level.ordinal(),
        }
        .try_into()?,
    )
    .await
    .map_err(io::Error::other)?;
    ws.close(None).await.ok();

    info!(
        id = id.0,
        level = level.as_str(),
        "asked broker to set the session's security level"
    );
    println!(
        "set session {id} security to {}; run `sessions` to confirm",
        level.as_str()
    );
    Ok(())
}
