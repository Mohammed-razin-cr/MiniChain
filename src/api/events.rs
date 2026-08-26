use std::time::Duration;

use axum::{
    extract::{
        State, WebSocketUpgrade,
        ws::{CloseFrame, Message, WebSocket},
    },
    response::Response,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::{sync::broadcast::error::RecvError, time::timeout};

use super::router::ApiState;

const AUTH_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthRequest {
    token: String,
}

#[derive(Serialize)]
struct EventEnvelope<T> {
    event: T,
    timestamp: chrono::DateTime<Utc>,
}

pub(crate) async fn events(ws: WebSocketUpgrade, State(state): State<ApiState>) -> Response {
    ws.on_upgrade(move |socket| event_stream(socket, state))
}

async fn event_stream(mut socket: WebSocket, state: ApiState) {
    let auth = timeout(AUTH_TIMEOUT, socket.recv()).await;
    let context = match auth {
        Ok(Some(Ok(Message::Text(text)))) => serde_json::from_str::<AuthRequest>(text.as_str())
            .ok()
            .and_then(|request| state.auth.authenticate_token(&request.token).ok()),
        _ => None,
    };
    let Some(context) = context else {
        close(&mut socket, 1008, "authentication required").await;
        return;
    };

    let authenticated = json!({
        "type": "authenticated",
        "identity": context.identity,
        "role": context.role,
        "timestamp": Utc::now(),
    });
    if send_json(&mut socket, &authenticated).await.is_err() {
        return;
    }

    let mut events = state.node.subscribe();
    loop {
        enum Next {
            Client(Option<Result<Message, axum::Error>>),
            Event(Result<crate::network::NetworkEvent, RecvError>),
        }
        let next = tokio::select! {
            message = socket.recv() => Next::Client(message),
            event = events.recv() => Next::Event(event),
        };
        match next {
            Next::Client(Some(Ok(Message::Close(_))) | None | Some(Err(_))) => break,
            Next::Client(_) => {}
            Next::Event(Ok(event)) => {
                let envelope = EventEnvelope {
                    event,
                    timestamp: Utc::now(),
                };
                if send_json(&mut socket, &envelope).await.is_err() {
                    break;
                }
            }
            Next::Event(Err(RecvError::Lagged(skipped))) => {
                let notice = json!({
                    "type": "resync_required",
                    "skipped": skipped,
                    "timestamp": Utc::now(),
                });
                if send_json(&mut socket, &notice).await.is_err() {
                    break;
                }
            }
            Next::Event(Err(RecvError::Closed)) => break,
        }
    }
}

async fn send_json(socket: &mut WebSocket, value: &impl Serialize) -> Result<(), axum::Error> {
    let text = serde_json::to_string(value).expect("serializable WebSocket event");
    socket.send(Message::Text(text.into())).await
}

async fn close(socket: &mut WebSocket, code: u16, reason: &'static str) {
    let _ = socket
        .send(Message::Close(Some(CloseFrame {
            code,
            reason: reason.into(),
        })))
        .await;
}
