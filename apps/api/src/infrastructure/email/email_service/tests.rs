use super::*;
use axum::{Json, Router, extract::State, response::IntoResponse, routing::post};
use serde_json::Value;
use std::sync::{Arc, Mutex};

const TOKEN: &str = "0194f123-4567-7abc-8def-0123456789ab0194f123-4567-7abc-8def-0123456789ac";
const RECIPIENT: &str = "private-recipient@example.test";
const KEY: &str = "private-provider-key";

struct Provider {
    status: StatusCode,
    delay: Duration,
    requests: Mutex<Vec<Value>>,
}

struct Fixture {
    service: EmailService,
    provider: Arc<Provider>,
    server: tokio::task::JoinHandle<()>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}

async fn respond(
    State(provider): State<Arc<Provider>>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    provider.requests.lock().unwrap().push(body.clone());
    tokio::time::sleep(provider.delay).await;
    // A hostile or diagnostic response can echo every credential-bearing field.
    (provider.status, [("location", "/emails")], Json(body))
}

fn service(base: &str, api_key: Option<&str>, timeout_ms: u32) -> Result<EmailService, EmailError> {
    EmailService::new(
        api_key.map(str::to_owned),
        "archive@example.test".into(),
        base.into(),
        &EmailLimits {
            request_timeout_ms: timeout_ms,
        },
    )
}

async fn fixture(base: &str, status: StatusCode, delay: Duration, timeout_ms: u32) -> Fixture {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/emails", listener.local_addr().unwrap());
    let provider = Arc::new(Provider {
        status,
        delay,
        requests: Mutex::new(Vec::new()),
    });
    let router = Router::new()
        .route("/emails", post(respond))
        .with_state(provider.clone());
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let mut service = service(base, Some(KEY), timeout_ms).unwrap();
    service.endpoint = endpoint.parse().unwrap();
    service.client = build_client(
        Client::builder().no_proxy(),
        &EmailLimits {
            request_timeout_ms: timeout_ms,
        },
    )
    .unwrap();
    Fixture {
        service,
        provider,
        server,
    }
}

fn assert_safe_error(error: &EmailError) {
    for text in [error.to_string(), format!("{error:?}")] {
        for secret in [TOKEN, RECIPIENT, KEY, "href=", "?token="] {
            assert!(!text.contains(secret), "email error exposed private data");
        }
    }
}

fn link(body: &Value) -> Url {
    let html = body["html"].as_str().unwrap();
    let href = html
        .split("href=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    Url::parse(&href.replace("&amp;", "&")).unwrap()
}

#[tokio::test]
async fn recovery_links_use_the_public_routes_with_or_without_a_trailing_slash() {
    for base in [
        "https://archive.example.test",
        "https://archive.example.test/",
    ] {
        let f = fixture(base, StatusCode::OK, Duration::ZERO, 1000).await;
        f.service
            .send_email_verification(RECIPIENT, TOKEN)
            .await
            .unwrap();
        f.service
            .send_password_reset(RECIPIENT, TOKEN)
            .await
            .unwrap();
        let requests = f.provider.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        let verification = link(&requests[0]);
        assert_eq!(
            verification.origin().ascii_serialization(),
            "https://archive.example.test"
        );
        assert_eq!(
            verification.path(),
            format!("/api/v1/auth/verify-email/{TOKEN}")
        );
        assert_eq!(verification.query(), None);
        let reset = link(&requests[1]);
        assert_eq!(reset.origin(), verification.origin());
        assert_eq!(reset.path(), "/auth");
        assert_eq!(
            reset.query_pairs().collect::<Vec<_>>(),
            [("token".into(), TOKEN.into())]
        );
        for request in requests.iter() {
            assert_eq!(request["from"], "archive@example.test");
            assert_eq!(request["to"], json!([RECIPIENT]));
        }
    }
}

#[tokio::test]
async fn tokens_cannot_inject_html_or_add_query_parameters() {
    let f = fixture(
        "https://archive.example.test",
        StatusCode::OK,
        Duration::ZERO,
        1000,
    )
    .await;
    let token = "token/&extra=1\"><script>";
    f.service
        .send_email_verification(RECIPIENT, token)
        .await
        .unwrap();
    f.service
        .send_password_reset(RECIPIENT, token)
        .await
        .unwrap();
    let requests = f.provider.requests.lock().unwrap();
    for request in requests.iter() {
        assert!(!request["html"].as_str().unwrap().contains("<script>"));
    }
    assert!(link(&requests[0]).path().contains("%2F"));
    assert_eq!(link(&requests[0]).query(), None);
    assert_eq!(
        link(&requests[1]).query_pairs().collect::<Vec<_>>(),
        [("token".into(), token.into())]
    );
}

#[tokio::test]
async fn rejections_never_expose_echoed_bodies_and_redirects_are_not_followed() {
    for status in [400, 401, 422, 429, 500, 302, 307] {
        let status = StatusCode::from_u16(status).unwrap();
        let f = fixture("https://archive.example.test", status, Duration::ZERO, 1000).await;
        let error = f
            .service
            .send_email_verification(RECIPIENT, TOKEN)
            .await
            .unwrap_err();
        assert!(matches!(error, EmailError::Rejected(code) if code == status));
        assert_safe_error(&error);
        let error = f
            .service
            .send_password_reset(RECIPIENT, TOKEN)
            .await
            .unwrap_err();
        assert!(matches!(error, EmailError::Rejected(code) if code == status));
        assert_safe_error(&error);
        assert_eq!(f.provider.requests.lock().unwrap().len(), 2);
    }
}

#[tokio::test]
async fn absent_or_blank_credentials_fail_without_contacting_the_provider() {
    let mut f = fixture(
        "http://localhost:5173",
        StatusCode::OK,
        Duration::ZERO,
        1000,
    )
    .await;
    for key in [None, Some(""), Some("  ")] {
        let configured = service("http://localhost:5173", key, 1000).unwrap();
        f.service.api_key = configured.api_key;
        assert!(!f.service.is_configured());
        assert!(matches!(
            f.service.send_email_verification(RECIPIENT, TOKEN).await,
            Err(EmailError::NotConfigured)
        ));
        assert!(matches!(
            f.service.send_password_reset(RECIPIENT, TOKEN).await,
            Err(EmailError::NotConfigured)
        ));
    }
    assert!(f.provider.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn provider_timeouts_and_connection_failures_are_safe_errors() {
    let mut f = fixture(
        "https://archive.example.test",
        StatusCode::OK,
        Duration::from_secs(1),
        100,
    )
    .await;
    let error = f
        .service
        .send_password_reset(RECIPIENT, TOKEN)
        .await
        .unwrap_err();
    assert!(matches!(&error, EmailError::Transport(cause) if cause.is_timeout()));
    assert_safe_error(&error);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    f.service.endpoint = format!("http://{}/emails", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    drop(listener);
    let error = f
        .service
        .send_email_verification(RECIPIENT, TOKEN)
        .await
        .unwrap_err();
    assert!(matches!(&error, EmailError::Transport(cause) if cause.is_connect()));
    assert_safe_error(&error);
}

#[test]
fn invalid_link_configuration_is_rejected_without_echoing_it() {
    for base in [
        "not a URL",
        "http://archive.example.test",
        "ftp://archive.example.test",
        "https://private-username:private-password@archive.example.test",
        "https://archive.example.test/path",
        "https://archive.example.test?token=private-token",
        "https://archive.example.test#private-token",
    ] {
        let error = service(base, Some(KEY), 1000)
            .err()
            .expect("invalid origin");
        assert!(matches!(error, EmailError::InvalidBaseUrl));
        assert!(!error.to_string().contains("private-"));
    }
    for base in [
        "http://localhost:5173",
        "http://127.0.0.1:5173",
        "http://[::1]:5173",
    ] {
        assert!(service(base, Some(KEY), 1000).is_ok());
    }
    assert!(matches!(
        EmailService::new(
            Some(KEY.into()),
            " ".into(),
            "https://archive.example.test".into(),
            &EmailLimits {
                request_timeout_ms: 1000
            }
        ),
        Err(EmailError::InvalidSender)
    ));
}
