#![allow(dead_code)]

use std::sync::{Arc, Mutex, Once};

use async_trait::async_trait;
use hilo::{
    handlers::AuthResponse,
    services::email::{EmailError, EmailService},
};
use reqwest::multipart;
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::net::TcpListener;

pub fn init_tracing_once() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        tracing_subscriber::fmt()
            .with_env_filter("hilo=debug")
            .with_test_writer()
            .init();
    });
}

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
/// Includes both main app routes and admin routes for comprehensive testing.
///
/// Returned address format: `http://127.0.0.1:8492`
pub async fn spawn_app(test_db_pool: PgPool) -> (String, Arc<MockEmailer>) {
    dotenvy::from_filename_override("tests/data/.test.env").unwrap();
    init_tracing_once();

    let mock_emailer = Arc::new(MockEmailer::new());
    let mock_cloned = Arc::clone(&mock_emailer);

    // Randomly choose an available port
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind random port at localhost");
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        // Create the main app with admin routes merged in
        let main_app = hilo::app_with_email_service(test_db_pool.clone(), mock_cloned);
        let admin_router = hilo::handlers::admin_router(test_db_pool);
        let combined_app = main_app.merge(admin_router);

        axum::serve(listener, combined_app).await.unwrap();
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

/// Test structure for admin API responses
pub struct TestApp {
    pub address: String,
}

/// Spawns only the admin app for testing admin-specific functionality
pub async fn spawn_admin_app(test_db_pool: PgPool) -> TestApp {
    dotenvy::from_filename_override("tests/data/.test.env").unwrap();
    init_tracing_once();

    // Randomly choose an available port
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind random port at localhost");
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        let admin_router = hilo::handlers::admin_router(test_db_pool);
        axum::serve(listener, admin_router).await.unwrap();
    });

    let address = format!("http://127.0.0.1:{port}");

    // Wait for server to be ready (try a simple request)
    let client = reqwest::Client::new();
    for _ in 0..10 {
        if client
            .get(format!("{address}/api/admin/stats"))
            .send()
            .await
            .is_ok()
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    TestApp { address }
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
    mock_emailer: &MockEmailer,
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
    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);

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
    vec![
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
    ]
}

pub fn create_male_form_submission() -> serde_json::Value {
    json!({
        "wechat_id": "test_wechat_123",
        "gender": "male",
        "familiar_tags": ["basketball", "pc_fps", "japanese"],
        "aspirational_tags": ["badminton"],
        "recent_topics": "I've been really interested in machine learning and AI lately, especially large language models and their applications in natural language processing.",
        "self_traits": ["humor", "curiosity", "reliable"],
        "ideal_traits": ["humor", "curiosity", "reliable"],
        "physical_boundary": 2,
        "self_intro": "Hi! I'm a computer science student who loves sports and gaming. I enjoy learning new technologies and meeting interesting people.",
    })
}

pub fn create_female_form_submission() -> serde_json::Value {
    json!({
        "wechat_id": "test_wechat_456",
        "gender": "female",
        "familiar_tags": ["running", "wild"],
        "aspirational_tags": ["fitness", "soccer"],
        "recent_topics": "I've been exploring different cooking techniques from various cultures and really enjoying the process of creating new dishes.",
        "self_traits": ["humor", "curiosity", "reliable"],
        "ideal_traits": ["humor", "curiosity", "reliable"],
        "physical_boundary": 1,
        "self_intro": "I'm passionate about arts and culture. I love dancing, cooking, and exploring new places with interesting people."
    })
}

pub async fn create_form_with_profile_photo(
    client: &reqwest::Client,
    address: &str,
    access_token: &str,
) -> serde_json::Value {
    // Create test image
    let image_data = create_test_image();

    // Upload profile photo
    let form = multipart::Form::new().part(
        "photo",
        multipart::Part::bytes(image_data)
            .file_name("profile.png")
            .mime_str("image/png")
            .unwrap(),
    );

    let response = client
        .post(format!("{address}/api/upload/profile-photo"))
        .header("Authorization", format!("Bearer {access_token}"))
        .multipart(form)
        .send()
        .await
        .expect("Failed to upload profile photo");

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    // Parse JSON response
    let response_json: Value = response
        .json()
        .await
        .expect("Failed to parse JSON response");
    let filename = response_json["filename"]
        .as_str()
        .expect("Should return a filename");

    json!({
        "wechat_id": "test_wechat_456",
        "gender": "female",
        "familiar_tags": ["wild"],
        "aspirational_tags": ["running"],
        "recent_topics": "I've been exploring different cooking techniques from various cultures and really enjoying the process of creating new dishes.",
        "self_traits": ["humor", "curiosity", "reliable"],
        "ideal_traits": ["humor", "curiosity", "reliable"],
        "physical_boundary": 1,
        "self_intro": "I'm passionate about arts and culture. I love dancing, cooking, and exploring new places with interesting people.",
        "profile_photo_filename": filename
    })
}

/// Admin helper to verify a user via email
pub async fn admin_verify_user(
    client: &reqwest::Client,
    address: &str,
    email: &str,
    status: &str,
) -> bool {
    let response = client
        .post(format!("{address}/api/admin/verify-user"))
        .json(&json!({
            "email": email,
            "status": status
        }))
        .send()
        .await
        .expect("Failed to send admin verify request");

    if response.status() != reqwest::StatusCode::OK {
        println!(
            "Admin verify failed for {}: Status {}",
            email,
            response.status()
        );
        if let Ok(body) = response.text().await {
            println!("Error response: {body}");
        }
        return false;
    }

    true
}

/// Admin helper to trigger final matching
pub async fn admin_trigger_final_match(
    client: &reqwest::Client,
    address: &str,
) -> serde_json::Value {
    let response = client
        .post(format!("{address}/api/admin/trigger-match"))
        .send()
        .await
        .expect("Failed to trigger final match");

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    response.json().await.expect("Failed to parse response")
}

/// Helper to upload card for user
pub async fn upload_card(client: &reqwest::Client, address: &str, access_token: &str) {
    let image_data = create_test_image();

    let form = multipart::Form::new()
        .part(
            "card",
            multipart::Part::bytes(image_data)
                .file_name("card.png")
                .mime_str("image/png")
                .unwrap(),
        )
        .text("grade", "undergraduate");

    let response = client
        .post(format!("{address}/api/upload/card"))
        .header("Authorization", format!("Bearer {access_token}"))
        .multipart(form)
        .send()
        .await
        .expect("Failed to upload card");

    assert_eq!(response.status(), reqwest::StatusCode::OK);
}

/// Helper to upload profile photo and get filename
pub async fn upload_profile_photo(
    client: &reqwest::Client,
    address: &str,
    access_token: &str,
) -> String {
    let image_data = create_test_image();

    let form = multipart::Form::new().part(
        "photo",
        multipart::Part::bytes(image_data)
            .file_name("profile.png")
            .mime_str("image/png")
            .unwrap(),
    );

    let response = client
        .post(format!("{address}/api/upload/profile-photo"))
        .header("Authorization", format!("Bearer {access_token}"))
        .multipart(form)
        .send()
        .await
        .expect("Failed to upload profile photo");

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let response_json: Value = response
        .json()
        .await
        .expect("Failed to parse JSON response");

    response_json["filename"]
        .as_str()
        .expect("Should return a filename")
        .to_string()
}

/// Helper to access partner image
pub async fn access_partner_image(
    client: &reqwest::Client,
    address: &str,
    access_token: &str,
    image_url: &str,
) -> reqwest::StatusCode {
    let response = client
        .get(format!("{address}{image_url}"))
        .header("Authorization", format!("Bearer {access_token}"))
        .send()
        .await
        .expect("Failed to access partner image");

    response.status()
}

/// Complete setup for two matched users - returns (male_token, female_token)
pub async fn setup_two_matched_users(
    client: &reqwest::Client,
    address: &str,
    mock_emailer: &MockEmailer,
) -> (String, String) {
    let male_email = "male@mails.tsinghua.edu.cn";
    let female_email = "female@mails.tsinghua.edu.cn";

    // 1. Get access tokens for both users
    let male_token = get_access_token(client, address, mock_emailer, male_email).await;
    let female_token = get_access_token(client, address, mock_emailer, female_email).await;

    // 2. Upload cards for both users
    upload_card(client, address, &male_token).await;
    upload_card(client, address, &female_token).await;

    // 3. Admin verifies both users
    assert!(admin_verify_user(client, address, male_email, "verified").await);
    assert!(admin_verify_user(client, address, female_email, "verified").await);

    // 4. Upload profile photos and submit forms
    let male_photo_filename = upload_profile_photo(client, address, &male_token).await;
    let female_photo_filename = upload_profile_photo(client, address, &female_token).await;

    let mut male_form = create_male_form_submission();
    male_form["profile_photo_filename"] = json!(male_photo_filename);

    let mut female_form = create_female_form_submission();
    female_form["profile_photo_filename"] = json!(female_photo_filename);

    // Submit forms
    let response = client
        .post(format!("{address}/api/form"))
        .header("Authorization", format!("Bearer {male_token}"))
        .json(&male_form)
        .send()
        .await
        .expect("Failed to submit male form");
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let response = client
        .post(format!("{address}/api/form"))
        .header("Authorization", format!("Bearer {female_token}"))
        .json(&female_form)
        .send()
        .await
        .expect("Failed to submit female form");
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    // 5. Admin triggers final matching
    admin_trigger_final_match(client, address).await;

    (male_token, female_token)
}
