use crate::limits::EmailLimits;
use reqwest::{Client, ClientBuilder, StatusCode, Url, redirect::Policy};
use serde_json::json;
use std::time::Duration;
use thiserror::Error;

/// Delivery failures expose no recipient, credentials, or provider response body.
#[derive(Debug, Error)]
pub enum EmailError {
    #[error("Email delivery disabled: RESEND_API_KEY is not configured")]
    NotConfigured,
    #[error("APP_BASE_URL must be an HTTPS origin (HTTP is allowed for localhost)")]
    InvalidBaseUrl,
    #[error("EMAIL_FROM must not be empty")]
    InvalidSender,
    #[error("Could not initialize the email HTTP client")]
    Client(#[source] reqwest::Error),
    #[error("Email provider request failed")]
    Transport(#[source] reqwest::Error),
    #[error("Email provider rejected delivery with HTTP {0}")]
    Rejected(StatusCode),
}

/// Transactional email backed by Resend. Disabled delivery returns an explicit error.
pub struct EmailService {
    client: Client,
    endpoint: Url,
    api_key: Option<String>,
    from: String,
    app_base_url: Url,
}

impl EmailService {
    /// Validate link configuration and build a reusable, bounded HTTP client.
    ///
    /// Missing credentials allow local startup, but send operations return
    /// [`EmailError::NotConfigured`]. Provider redirects are never followed.
    pub fn new(
        api_key: Option<String>,
        from: String,
        app_base_url: String,
        limits: &EmailLimits,
    ) -> Result<Self, EmailError> {
        let app_base_url = Url::parse(&app_base_url).map_err(|_| EmailError::InvalidBaseUrl)?;
        let local_http = app_base_url.scheme() == "http"
            && matches!(
                app_base_url.host_str(),
                Some("localhost" | "127.0.0.1" | "[::1]")
            );
        if !(app_base_url.scheme() == "https" || local_http)
            || app_base_url.host_str().is_none()
            || !app_base_url.username().is_empty()
            || app_base_url.password().is_some()
            || app_base_url.path() != "/"
            || app_base_url.query().is_some()
            || app_base_url.fragment().is_some()
        {
            return Err(EmailError::InvalidBaseUrl);
        }
        if from.trim().is_empty() {
            return Err(EmailError::InvalidSender);
        }
        let client = build_client(Client::builder(), limits)?;
        Ok(Self {
            client,
            // A static provider URL, independent of user input or link configuration.
            endpoint: Url::parse("https://api.resend.com/emails").expect("valid Resend URL"),
            api_key: api_key.filter(|key| !key.trim().is_empty()),
            from,
            app_base_url,
        })
    }

    pub fn is_configured(&self) -> bool {
        self.api_key.is_some()
    }

    /// Send a verification link to the API route on the public frontend origin.
    pub async fn send_email_verification(
        &self,
        to: &str,
        raw_token: &str,
    ) -> Result<(), EmailError> {
        let mut link = self.app_base_url.clone();
        link.path_segments_mut()
            .expect("validated HTTP origin supports path segments")
            .extend(["api", "v1", "auth", "verify-email", raw_token]);
        let link = html_link(&link);
        let html = format!(
            r#"<p>Welcome to Through Your Letters.</p>
<p>Click the link below to verify your email address. The link expires in 24 hours.</p>
<p><a href="{link}">{link}</a></p>
<p>If you did not create an account, you can safely ignore this email.</p>"#
        );
        self.send(to, "Verify your email — Through Your Letters", &html)
            .await
    }

    /// Send a reset link to the frontend's token-based password form.
    pub async fn send_password_reset(&self, to: &str, raw_token: &str) -> Result<(), EmailError> {
        let mut link = self.app_base_url.clone();
        link.set_path("/auth");
        link.query_pairs_mut().append_pair("token", raw_token);
        let link = html_link(&link);
        let html = format!(
            r#"<p>You requested a password reset for your Through Your Letters account.</p>
<p>Click the link below to set a new password. The link expires in 1 hour.</p>
<p><a href="{link}">{link}</a></p>
<p>If you did not request this reset, you can safely ignore this email.</p>"#
        );
        self.send(to, "Reset your password — Through Your Letters", &html)
            .await
    }

    async fn send(&self, to: &str, subject: &str, html: &str) -> Result<(), EmailError> {
        let api_key = self.api_key.as_ref().ok_or(EmailError::NotConfigured)?;
        let response = self
            .client
            .post(self.endpoint.clone())
            .bearer_auth(api_key)
            .json(&json!({
                "from": self.from,
                "to": [to],
                "subject": subject,
                "html": html,
            }))
            .send()
            .await
            .map_err(EmailError::Transport)?;

        if !response.status().is_success() {
            // Providers can echo the request, including recovery links. Never retain that body.
            return Err(EmailError::Rejected(response.status()));
        }
        Ok(())
    }
}

fn build_client(builder: ClientBuilder, limits: &EmailLimits) -> Result<Client, EmailError> {
    builder
        .timeout(Duration::from_millis(limits.request_timeout_ms.into()))
        .redirect(Policy::none())
        .build()
        .map_err(EmailError::Client)
}

fn html_link(url: &Url) -> String {
    url.as_str()
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests;
