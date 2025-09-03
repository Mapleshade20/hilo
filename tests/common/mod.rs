#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use hilo::handlers::AuthResponse;
use hilo::services::email::{EmailError, EmailService};
use serde_json::json;
use sqlx::PgPool;
use tokio::net::TcpListener;

/// A mock email service that stores sent emails for testing purposes.
/// This is ideal for integration tests as it doesn't produce console output.
#[derive(Debug, Default)]
pub struct MockEmailer {
    sent_emails: Mutex<Vec<SentEmail>>,
}

#[derive(Debug, Clone)]
pub struct SentEmail {
    pub recipient: String,
    pub subject: String,
    pub body_html: String,
}

impl MockEmailer {
    pub fn new() -> Self {
        Self {
            sent_emails: Mutex::new(Vec::new()),
        }
    }

    /// Get all sent emails for testing verification
    pub fn get_sent_emails(&self) -> Vec<SentEmail> {
        self.sent_emails.lock().unwrap().clone()
    }

    /// Clear all stored emails
    pub fn clear(&self) {
        self.sent_emails.lock().unwrap().clear();
    }

    /// Get the count of sent emails
    pub fn sent_count(&self) -> usize {
        self.sent_emails.lock().unwrap().len()
    }

    /// Get the last sent email
    pub fn last_sent_email(&self) -> Option<SentEmail> {
        self.sent_emails.lock().unwrap().last().cloned()
    }
}

#[async_trait]
impl EmailService for MockEmailer {
    async fn send_email(
        &self,
        recipient: &str,
        subject: &str,
        code: &str,
    ) -> Result<(), EmailError> {
        let email = SentEmail {
            recipient: recipient.to_string(),
            subject: subject.to_string(),
            body_html: format!("Your verification code is: {code}"),
        };

        self.sent_emails.lock().unwrap().push(email);
        Ok(())
    }
}

/// Spawns the application and returns its address and mock emailer for testing.
///
/// Returned address format: `http://127.0.0.1:8492`
pub async fn spawn_app(test_db_pool: PgPool) -> (String, Arc<MockEmailer>) {
    dotenvy::from_filename_override("tests/data/.test.env").unwrap();

    let mock_emailer = Arc::new(MockEmailer::new());
    let mock_cloned = Arc::clone(&mock_emailer);

    // Randomly choose an available port
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind random port at localhost");
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        let app = hilo::app_with_email_service(test_db_pool, mock_cloned);
        axum::serve(listener, app).await.unwrap();
    });

    let address = format!("http://127.0.0.1:{port}");

    // Wait for server to be ready
    let client = reqwest::Client::new();
    for _ in 0..10 {
        if client
            .get(format!("{address}/health-check"))
            .send()
            .await
            .is_ok()
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    (address, mock_emailer)
}

// Helper function to extract verification code from email body
pub fn extract_verification_code(email_body: &str) -> &str {
    // Extract 6-digit code from "Your verification code is: 123456"
    email_body.trim_start_matches("Your verification code is: ")
}

/// Helper function to complete auth flow and return access token
pub async fn get_access_token(
    client: &reqwest::Client,
    address: &str,
    mock_emailer: &Arc<MockEmailer>,
    email_addr: &str,
) -> String {
    mock_emailer.clear();

    // Send verification code
    let response = client
        .post(format!("{address}/api/auth/send-code"))
        .json(&json!({"email": email_addr}))
        .send()
        .await
        .expect("Failed to send code");
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    // Extract code from email
    let sent_email = mock_emailer.last_sent_email().expect("No email sent");
    let code = extract_verification_code(&sent_email.body_html);

    // Verify code and get tokens
    let response = client
        .post(format!("{address}/api/auth/verify-code"))
        .json(&json!({"email": email_addr, "code": code}))
        .send()
        .await
        .expect("Failed to verify code");
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let auth_response: AuthResponse = response.json().await.expect("Failed to parse response");
    auth_response.access_token
}

/// Creates a simple 1x1 PNG image and returns its byte representation.
pub fn create_test_image() -> Vec<u8> {
    // Create a simple 1x1 PNG image
    let png_data = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
        0x00, 0x00, 0x00, 0x0D, // IHDR chunk length
        0x49, 0x48, 0x44, 0x52, // IHDR
        0x00, 0x00, 0x00, 0x01, // Width: 1
        0x00, 0x00, 0x00, 0x01, // Height: 1
        0x08, 0x02, 0x00, 0x00,
        0x00, // Bit depth: 8, Color type: 2 (RGB), Compression: 0, Filter: 0, Interlace: 0
        0x90, 0x77, 0x53, 0xDE, // CRC
        0x00, 0x00, 0x00, 0x0C, // IDAT chunk length
        0x49, 0x44, 0x41, 0x54, // IDAT
        0x08, 0x99, 0x01, 0x01, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0x00, // Image data
        0x02, 0x00, 0x01, 0xE5, // CRC
        0x00, 0x00, 0x00, 0x00, // IEND chunk length
        0x49, 0x45, 0x4E, 0x44, // IEND
        0xAE, 0x42, 0x60, 0x82, // CRC
    ];
    png_data
}
