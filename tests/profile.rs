mod common;

use common::{get_access_token, spawn_app};
use hilo::handlers::ProfileResponse;
use hilo::utils::user_status::UserStatus;
use sqlx::PgPool;

#[sqlx::test]
async fn test_profile_with_valid_token(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool).await;
    let client = reqwest::Client::new();
    let test_email = "test@mails.tsinghua.edu.cn";

    // Get access token
    let access_token = get_access_token(&client, &address, &mock_emailer, test_email).await;

    // Access profile endpoint with valid token
    let response = client
        .get(format!("{}/api/profile", &address))
        .header("Authorization", format!("Bearer {access_token}"))
        .send()
        .await
        .expect("Failed to execute request");

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let profile: ProfileResponse = response.json().await.expect("Failed to parse response");
    assert_eq!(profile.email, test_email);
    assert_eq!(profile.status, UserStatus::Unverified);
}

#[sqlx::test]
async fn test_profile_without_token(pool: PgPool) {
    let (address, _) = spawn_app(pool).await;
    let client = reqwest::Client::new();

    // Try to access profile endpoint without token
    let response = client
        .get(format!("{}/api/profile", &address))
        .send()
        .await
        .expect("Failed to execute request");

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn test_profile_with_invalid_token(pool: PgPool) {
    let (address, _) = spawn_app(pool).await;
    let client = reqwest::Client::new();

    // Try to access profile endpoint with invalid token
    let response = client
        .get(format!("{}/api/profile", &address))
        .header("Authorization", "Bearer invalid-token")
        .send()
        .await
        .expect("Failed to execute request");

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn test_profile_with_malformed_authorization_header(pool: PgPool) {
    let (address, _) = spawn_app(pool).await;
    let client = reqwest::Client::new();

    let test_cases = vec![
        ("Bearer", "Missing token after Bearer"),
        ("Basic token123", "Wrong auth type"),
        ("token123", "Missing Bearer prefix"),
        ("", "Empty header"),
    ];

    for (auth_header, description) in test_cases {
        let response = client
            .get(format!("{}/api/profile", &address))
            .header("Authorization", auth_header)
            .send()
            .await
            .unwrap_or_else(|_| panic!("Failed to execute request for {description}"));

        assert_eq!(
            response.status(),
            reqwest::StatusCode::UNAUTHORIZED,
            "Failed for case: {description}"
        );
    }
}

#[sqlx::test]
async fn test_profile_multiple_users(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool).await;
    let client = reqwest::Client::new();

    let users = vec![
        "user1@mails.tsinghua.edu.cn",
        "user2@mails.tsinghua.edu.cn",
        "user3@mails.tsinghua.edu.cn",
    ];

    // Test that each user gets their own profile data
    for email in &users {
        let access_token = get_access_token(&client, &address, &mock_emailer, email).await;

        let response = client
            .get(format!("{}/api/profile", &address))
            .header("Authorization", format!("Bearer {access_token}"))
            .send()
            .await
            .expect("Failed to execute request");

        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let profile: ProfileResponse = response.json().await.expect("Failed to parse response");
        assert_eq!(profile.email, *email);
        assert_eq!(profile.status, UserStatus::Unverified);
    }
}
