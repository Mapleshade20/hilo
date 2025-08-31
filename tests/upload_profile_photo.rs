mod common;

use common::{create_test_image, get_access_token, spawn_app};
use hilo::models::UserStatus;
use reqwest::multipart;
use serde_json::Value;
use sqlx::PgPool;

#[sqlx::test]
async fn test_upload_profile_photo_success_verified_user(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool.clone()).await;
    let client = reqwest::Client::new();
    let test_email = "test@mails.tsinghua.edu.cn";

    // Get access token (this creates a user with 'unverified' status)
    let access_token = get_access_token(&client, &address, &mock_emailer, test_email).await;

    // Update user status to 'verified' to allow profile photo upload
    sqlx::query!(
        "UPDATE users SET status = $1 WHERE email = $2",
        UserStatus::Verified as UserStatus,
        test_email
    )
    .execute(&pool)
    .await
    .expect("Failed to update user status");

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
    assert!(
        response_json["file_path"]
            .as_str()
            .unwrap()
            .contains("profile_photos/")
    );

    // Verify user status hasn't changed (should still be 'verified')
    let user_record = sqlx::query!(
        r#"SELECT status as "status: UserStatus" FROM users WHERE email = $1"#,
        test_email
    )
    .fetch_one(&pool)
    .await
    .expect("Failed to fetch user");

    assert_eq!(user_record.status, UserStatus::Verified);
}

#[sqlx::test]
async fn test_upload_profile_photo_forbidden_verification_pending_user(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool.clone()).await;
    let client = reqwest::Client::new();
    let test_email = "test@mails.tsinghua.edu.cn";

    // Get access token
    let access_token = get_access_token(&client, &address, &mock_emailer, test_email).await;

    // Update user status to 'verification_pending'
    sqlx::query!(
        "UPDATE users SET status = $1 WHERE email = $2",
        UserStatus::VerificationPending as UserStatus,
        test_email
    )
    .execute(&pool)
    .await
    .expect("Failed to update user status");

    // Create test image
    let image_data = create_test_image();

    // Try to upload profile photo
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

    assert_eq!(response.status(), reqwest::StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn test_upload_profile_photo_invalid_format(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool.clone()).await;
    let client = reqwest::Client::new();
    let test_email = "test@mails.tsinghua.edu.cn";

    // Get access token and set user to verified status
    let access_token = get_access_token(&client, &address, &mock_emailer, test_email).await;
    sqlx::query!(
        "UPDATE users SET status = $1 WHERE email = $2",
        UserStatus::Verified as UserStatus,
        test_email
    )
    .execute(&pool)
    .await
    .expect("Failed to update user status");

    // Create invalid file data
    let invalid_data = b"This is not an image";

    let form = multipart::Form::new().part(
        "photo",
        multipart::Part::bytes(invalid_data.to_vec())
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

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn test_upload_profile_photo_unauthorized(pool: PgPool) {
    let (address, _) = spawn_app(pool).await;
    let client = reqwest::Client::new();

    // Create test image
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
        .multipart(form)
        .send()
        .await
        .expect("Failed to upload profile photo");

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn test_upload_profile_photo_empty_file(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool.clone()).await;
    let client = reqwest::Client::new();
    let test_email = "test@mails.tsinghua.edu.cn";

    // Get access token and set user to verified status
    let access_token = get_access_token(&client, &address, &mock_emailer, test_email).await;
    sqlx::query!(
        "UPDATE users SET status = $1 WHERE email = $2",
        UserStatus::Verified as UserStatus,
        test_email
    )
    .execute(&pool)
    .await
    .expect("Failed to update user status");

    // Create empty file
    let empty_data: Vec<u8> = vec![];

    let form = multipart::Form::new().part(
        "photo",
        multipart::Part::bytes(empty_data)
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

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
}
