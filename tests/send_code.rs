mod common;

use common::spawn_app;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test]
async fn send_verification_code_works(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool).await;
    let client = reqwest::Client::new();

    let available_emails = ["test@mails.tsinghua.edu.cn", "test@stu.pku.edu.cn"];
    for test_email in available_emails {
        let response = client
            .post(format!("{}/api/auth/send-code", &address))
            .json(&json!({
                    "email": test_email
            }))
            .send()
            .await
            .expect("Failed to execute request");

        assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);

        let body = response.text().await.expect("Failed to read response body");
        assert_eq!(body, "Verification code sent");
    }

    // Verify that email was sent through MockEmailer
    assert_eq!(mock_emailer.sent_count(), 2);

    let sent_email = mock_emailer.last_sent_email().expect("No email was sent");
    assert_eq!(&sent_email.recipient, available_emails.last().unwrap());
    assert_eq!(sent_email.subject, "Verification code");
    assert!(sent_email.body_html.contains("Your verification code is:"));
}

#[sqlx::test]
async fn send_verification_code_invalid_email(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool).await;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{}/api/auth/send-code", &address))
        .json(&json!({
            "email": "invalid-email@tsinghua.edu.cn"
        }))
        .send()
        .await
        .expect("Failed to execute request");

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);

    // Verify that no email was sent
    assert_eq!(mock_emailer.sent_count(), 0);
}

#[sqlx::test]
async fn send_verification_code_rate_limit(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool).await;
    let client = reqwest::Client::new();

    let test_email = "test@mails.tsinghua.edu.cn";

    // Send first request
    let response1 = client
        .post(format!("{}/api/auth/send-code", &address))
        .json(&json!({
            "email": test_email
        }))
        .send()
        .await
        .expect("Failed to execute first request");

    assert_eq!(response1.status(), reqwest::StatusCode::ACCEPTED);
    assert_eq!(mock_emailer.sent_count(), 1);

    // Send second request immediately (should be rate limited)
    let response2 = client
        .post(format!("{}/api/auth/send-code", &address))
        .json(&json!({
            "email": test_email
        }))
        .send()
        .await
        .expect("Failed to execute second request");

    assert_eq!(response2.status(), reqwest::StatusCode::TOO_MANY_REQUESTS);

    let body = response2
        .text()
        .await
        .expect("Failed to read response body");
    assert!(body.contains("Rate limit exceeded"));

    // Verify that only one email was sent (not affected by rate limit)
    assert_eq!(mock_emailer.sent_count(), 1);
}
