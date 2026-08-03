use std::{io, time::Duration};

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

use crate::{
    commands::{CommandResult, connect_broker},
    protocol::{ClientMessage, ServerMessage, SessionInfo, text_decode},
};

const REPLY_TIMEOUT: Duration = Duration::from_secs(3);

pub async fn sessions() -> CommandResult {
    // Connect to the broker, and ask for the sessions
    let mut ws = connect_broker().await?;
    ws.send(ClientMessage::ListSessions.try_into()?)
        .await
        .map_err(io::Error::other)?;

    // Wait for a reply
    let reply = tokio::time::timeout(REPLY_TIMEOUT, async {
        while let Some(Ok(frame)) = ws.next().await {
            if let Message::Text(text) = frame {
                if let Some(bytes) = text_decode(text.as_str()) {
                    if let Ok(ServerMessage::Sessions(list)) = ServerMessage::from_bytes(bytes) {
                        return Some(list);
                    }
                }
            }
        }
        None
    })
    .await;

    // Log out the sessions
    match reply {
        Ok(Some(list)) => {
            print_sessions(&list);
            Ok(())
        }
        _ => Err(io::Error::other("broker did not answer with a session list").into()),
    }
}

fn print_sessions(sessions: &[SessionInfo]) {
    if sessions.is_empty() {
        println!("no clients connected");
        return;
    }

    println!(
        "  {:<4} {:<20} {:<24} {:<8} SECURITY",
        "ID", "PLAYER", "PEER", "STATUS"
    );

    for session in sessions {
        let status = if session.active { "active" } else { "standby" };
        let security = crate::SecurityLevel::from_ordinal(session.security_level).as_str();
        let player = session.username.as_deref().unwrap_or("-");

        println!(
            "  {:<4} {player:<20} {:<24} {status:<8} {security}",
            session.id.0, session.peer
        );
    }
}
