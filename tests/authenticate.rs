mod common;

use std::sync::Arc;

use common::{MockEmailer, spawn_app};
use hilo::handlers::AuthResponse;
use serde_json::json;
use sqlx::PgPool;

// Helper function to extract verification code from email body
fn extract_verification_code(email_body: &str) -> &str {
    // Extract 6-digit code from "Your verification code is: 123456"
    email_body.trim_start_matches("Your verification code is: ")
}

/// Helper function to complete auth flow
///
/// # Returns:
/// (AuthResponse, code)
async fn complete_auth_flow(
    client: &reqwest::Client,
    address: &str,
    mock_emailer: &Arc<MockEmailer>,
    email_addr: &str,
) -> (AuthResponse, String) {
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

    (
        response.json().await.expect("Failed to parse response"),
        code.to_string(),
    )
}

#[sqlx::test]
async fn test_verify_code_invalid_email_format(pool: PgPool) {
    let (address, _) = spawn_app(pool).await;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{}/api/auth/verify-code", &address))
        .json(&json!({
            "email": "invalid-email",
            "code": "123456"
        }))
        .send()
        .await
        .expect("Failed to execute request");

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);

    let body = response.text().await.expect("Failed to read response");
    assert!(body.contains("Invalid input"));
}

#[sqlx::test]
async fn test_verify_code_invalid_code_format(pool: PgPool) {
    let (address, _) = spawn_app(pool).await;
    let client = reqwest::Client::new();

    let test_cases = vec![
        ("12345", "too short"),    // 5 digits
        ("1234567", "too long"),   // 7 digits
        ("abcdef", "non-numeric"), // letters
        ("", "empty"),             // empty
    ];

    for (invalid_code, description) in test_cases {
        let response = client
            .post(format!("{}/api/auth/verify-code", &address))
            .json(&json!({
                "email": "test@mails.tsinghua.edu.cn",
                "code": invalid_code
            }))
            .send()
            .await
            .unwrap_or_else(|_| panic!("Failed to execute request for {description}"));

        assert_eq!(
            response.status(),
            reqwest::StatusCode::BAD_REQUEST,
            "Failed for case: {description}",
        );
    }
}

#[sqlx::test]
async fn test_verify_code_wrong_code(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool).await;
    let client = reqwest::Client::new();
    let test_email = "test@mails.tsinghua.edu.cn";

    // Send verification code
    let response = client
        .post(format!("{address}/api/auth/send-code"))
        .json(&json!({"email": test_email}))
        .send()
        .await
        .expect("Failed to send code");
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    // Extract code from email
    let sent_email = mock_emailer.last_sent_email().expect("No email sent");
    let code = extract_verification_code(&sent_email.body_html);

    // Try with wrong code
    let response = client
        .post(format!("{}/api/auth/verify-code", &address))
        .json(&json!({
            "email": test_email,
            "code": if code == "123456" { "654321" } else { "123456" }
        }))
        .send()
        .await
        .expect("Failed to verify with wrong code");

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);

    let body = response.text().await.expect("Failed to read response");
    assert_eq!(body, "Invalid or expired code");
}

#[sqlx::test]
async fn test_verify_code_removes_from_cache(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool).await;
    let client = reqwest::Client::new();
    let test_email = "test@mails.tsinghua.edu.cn";

    let code = complete_auth_flow(&client, &address, &mock_emailer, test_email)
        .await
        .1;

    // Try to use the same code again - should fail
    let response = client
        .post(format!("{}/api/auth/verify-code", &address))
        .json(&json!({
            "email": test_email,
            "code": code
        }))
        .send()
        .await
        .expect("Failed to verify code");

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn test_refresh_token_rotation(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool).await;
    let client = reqwest::Client::new();
    let test_email = "test@mails.tsinghua.edu.cn";

    // Complete auth flow to get tokens
    let AuthResponse {
        access_token,
        refresh_token,
        ..
    } = complete_auth_flow(&client, &address, &mock_emailer, test_email)
        .await
        .0;

    // Wait for 1 second to ensure new tokens have different timestamps
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;

    // Use refresh token to get new token pair
    let response = client
        .post(format!("{}/api/auth/refresh", &address))
        .json(&json!({
            "refresh_token": refresh_token
        }))
        .send()
        .await
        .expect("Failed to refresh token");

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let new_auth_response: AuthResponse = response.json().await.expect("Failed to parse JSON");
    let new_access_token = new_auth_response.access_token;
    let new_refresh_token = new_auth_response.refresh_token;

    // Tokens should be different (token rotation)
    assert_ne!(access_token, new_access_token);
    assert_ne!(refresh_token, new_refresh_token);

    // Try to use the old refresh token - should fail due to rotation
    let response = client
        .post(format!("{}/api/auth/refresh", &address))
        .json(&json!({
            "refresh_token": refresh_token
        }))
        .send()
        .await
        .expect("Failed to retry refresh");

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
    let body = response.text().await.expect("Failed to read response");
    assert_eq!(body, "Invalid refresh token");
}

#[sqlx::test]
async fn test_refresh_token_invalid(pool: PgPool) {
    let (address, _) = spawn_app(pool).await;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{}/api/auth/refresh", &address))
        .json(&json!({
            "refresh_token": "invalid-refresh-token"
        }))
        .send()
        .await
        .expect("Failed to execute request");

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);

    let body = response.text().await.expect("Failed to read response");
    assert_eq!(body, "Invalid refresh token");
}

#[sqlx::test]
async fn test_multiple_users_different_codes(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool).await;
    let client = reqwest::Client::new();

    let users = vec![
        "user1@mails.tsinghua.edu.cn",
        "user2@mails.tsinghua.edu.cn",
        "user3@mails.tsinghua.edu.cn",
    ];

    // Send codes to all users
    for email in &users {
        let response = client
            .post(format!("{}/api/auth/send-code", &address))
            .json(&json!({"email": email}))
            .send()
            .await
            .expect("Failed to send code");
        assert_eq!(response.status(), reqwest::StatusCode::OK);
    }

    assert_eq!(mock_emailer.sent_count(), users.len());

    // Verify each user can authenticate with their respective codes
    let sent_emails = mock_emailer.get_sent_emails();

    for (i, email) in users.iter().enumerate() {
        let sent_email = &sent_emails[i];
        assert_eq!(&sent_email.recipient, email);

        let code = extract_verification_code(&sent_email.body_html);

        let response = client
            .post(format!("{}/api/auth/verify-code", &address))
            .json(&json!({
                "email": email,
                "code": code
            }))
            .send()
            .await
            .expect("Failed to verify code");

        assert_eq!(response.status(), reqwest::StatusCode::OK);
    }
}

// This test would be for protected endpoints when they're implemented
// #[sqlx::test]
// async fn test_protected_endpoint_with_valid_token(_pool: PgPool) {
//     // TODO: Implement when protected endpoints are added
//     // This test should:
//     // 1. Complete auth flow to get access token
//     // 2. Use access token to access protected endpoint
//     // 3. Verify successful access
// }

// #[sqlx::test]
// async fn test_protected_endpoint_with_invalid_token(_pool: PgPool) {
//     // TODO: Implement when protected endpoints are added
//     // This test should:
//     // 1. Try to access protected endpoint with invalid/expired token
//     // 2. Verify access is denied
// }
//
// #[sqlx::test]
// async fn test_protected_endpoint_without_token(_pool: PgPool) {
//     // TODO: Implement when protected endpoints are added
//     // This test should:
//     // 1. Try to access protected endpoint without any token
//     // 2. Verify access is denied
// }
