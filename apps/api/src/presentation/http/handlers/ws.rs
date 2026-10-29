use crate::presentation::http::state::AppState;
use axum::{
    extract::{
        State,
        ws::{Message, WebSocketUpgrade},
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(tag = "type")]
enum FeedEvent {
    #[serde(rename = "PROCESSED")]
    Processed { id: Uuid },
}

pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| async move {
        let (mut sender, mut receiver) = socket.split();
        let mut rx = state.ws_broadcaster.subscribe();
        loop {
            let msg = tokio::select! {
                incoming = receiver.next() => {
                    match incoming {
                        Some(Ok(Message::Close(_))) => {
                            // Tungstenite queues the close reply while reading.
                            let _ = sender.flush().await;
                            break;
                        }
                        Some(Ok(Message::Ping(_))) => {
                            if sender.flush().await.is_err() { break; }
                        }
                        Some(Ok(_)) => {}
                        Some(Err(_)) | None => break,
                    }
                    continue;
                }
                event = rx.recv() => match event {
                    Ok(msg) => msg,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            };
            let Ok(FeedEvent::Processed { id }) = serde_json::from_str(&msg) else {
                continue;
            };
            let visible = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM discoverable_letterings WHERE id = $1)",
            )
            .bind(id)
            .fetch_one(&state.db)
            .await;
            match visible {
                Ok(true) => {}
                Ok(false) => continue,
                Err(error) => {
                    tracing::warn!(%error, "Public feed visibility check failed");
                    break;
                }
            }
            let public_event = serde_json::json!({ "type": "PROCESSED", "id": id });
            if sender
                .send(Message::Text(public_event.to_string().into()))
                .await
                .is_err()
            {
                break;
            }
        }
    })
}
