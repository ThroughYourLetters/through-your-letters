use serde_json::json;

/// Transactional email service backed by the Resend HTTP API.
///
/// When `api_key` is `None` (e.g., in local development without `RESEND_API_KEY`),
/// every `send_*` call is a no-op and simply returns `Ok(())` so that the rest of
/// the application can proceed without a real email provider configured.
pub struct EmailService {
    client: reqwest::Client,
    api_key: Option<String>,
    from: String,
    app_base_url: String,
}

impl EmailService {
    pub fn new(api_key: Option<String>, from: String, app_base_url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            from,
            app_base_url,
        }
    }

    /// Send an email verification link to a newly registered user.
    pub async fn send_email_verification(
        &self,
        to: &str,
        raw_token: &str,
    ) -> anyhow::Result<()> {
        let link = format!(
            "{}/api/v1/auth/verify-email/{}",
            self.app_base_url, raw_token
        );
        let html = format!(
            r#"<p>Welcome to Through Your Letters.</p>
<p>Click the link below to verify your email address. The link expires in 24 hours.</p>
<p><a href="{link}">{link}</a></p>
<p>If you did not create an account, you can safely ignore this email.</p>"#
        );
        self.send(to, "Verify your email — Through Your Letters", &html)
            .await
    }

    /// Send a password reset link.
    pub async fn send_password_reset(&self, to: &str, raw_token: &str) -> anyhow::Result<()> {
        let link = format!("{}/auth?token={}", self.app_base_url, raw_token);
        let html = format!(
            r#"<p>You requested a password reset for your Through Your Letters account.</p>
<p>Click the link below to set a new password. The link expires in 1 hour.</p>
<p><a href="{link}">{link}</a></p>
<p>If you did not request this reset, you can safely ignore this email.</p>"#
        );
        self.send(to, "Reset your password — Through Your Letters", &html)
            .await
    }

    async fn send(&self, to: &str, subject: &str, html: &str) -> anyhow::Result<()> {
        let Some(api_key) = &self.api_key else {
            tracing::debug!(
                to = to,
                subject = subject,
                "Email not sent — RESEND_API_KEY not configured"
            );
            return Ok(());
        };

        let body = json!({
            "from": self.from,
            "to": [to],
            "subject": subject,
            "html": html,
        });

        let res = self
            .client
            .post("https://api.resend.com/emails")
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Resend request failed: {}", e))?;

        if !res.status().is_success() {
            let status = res.status();
            let text = res.text().await.unwrap_or_default();
            anyhow::bail!("Resend API error {}: {}", status, text);
        }

        tracing::debug!(to = to, subject = subject, "Email sent via Resend");
        Ok(())
    }
}
