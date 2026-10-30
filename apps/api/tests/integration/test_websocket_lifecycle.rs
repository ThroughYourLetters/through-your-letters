use crate::helpers::spawn_app;
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[sqlx::test]
async fn idle_feed_answers_ping_and_completes_close(pool: sqlx::PgPool) {
    let app = spawn_app(&pool).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/ws/feed", listener.local_addr().unwrap());
    let router = app.app.clone();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let (mut socket, _) = connect_async(url).await.unwrap();
    socket
        .send(Message::Ping(vec![1, 2, 3].into()))
        .await
        .unwrap();
    let pong = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .expect("idle feed must answer ping")
        .unwrap()
        .unwrap();
    assert_eq!(pong, Message::Pong(vec![1, 2, 3].into()));
    socket.close(None).await.unwrap();
    let reply = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .expect("idle feed must acknowledge close")
        .unwrap()
        .unwrap();
    assert!(matches!(reply, Message::Close(_)));
    tokio::time::timeout(Duration::from_secs(2), async {
        while app.ws_broadcaster.receiver_count() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("closed connection must release its feed subscription");
    server.abort();
}
