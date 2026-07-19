use std::io;

use futures_util::SinkExt;
use tracing::info;

use crate::{
    commands::{CommandResult, connect_broker},
    protocol::ClientMessage,
    server::SessionId,
};

pub async fn swap(id: SessionId) -> CommandResult {
    // Try to connect to the broker and request the swap
    let mut ws = connect_broker().await?;
    ws.send(ClientMessage::SwapActive(id).try_into()?)
        .await
        .map_err(io::Error::other)?;
    ws.close(None).await.ok();

    info!(id = id.0, "asked broker to make the session active");
    println!("requested swap to session {id}; run `sessions` to confirm");
    Ok(())
}
