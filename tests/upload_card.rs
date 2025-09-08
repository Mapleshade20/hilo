mod common;

use common::{create_test_image, get_access_token, spawn_app};
use hilo::models::UserStatus;
use reqwest::multipart;
use sqlx::PgPool;

#[sqlx::test]
async fn test_upload_card_success(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool.clone()).await;
    let client = reqwest::Client::new();
    let test_email = "test@mails.tsinghua.edu.cn";

    // Get access token
    let access_token = get_access_token(&client, &address, &mock_emailer, test_email).await;

    // Create test image
    let image_data = create_test_image();

    // Upload card
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

    // Verify database was updated (both file path, grade, and status)
    let user_record = sqlx::query!(
        r#"SELECT card_photo_filename, grade, status as "status: UserStatus" FROM users WHERE email = $1"#,
        test_email
    )
    .fetch_one(&pool)
    .await
    .expect("Failed to fetch user");

    assert!(user_record.card_photo_filename.is_some());
    assert_eq!(user_record.grade, Some("undergraduate".to_string()));
    assert_eq!(user_record.status, UserStatus::VerificationPending);
}

#[sqlx::test]
async fn test_upload_card_invalid_format(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool).await;
    let client = reqwest::Client::new();
    let test_email = "test@mails.tsinghua.edu.cn";

    // Get access token
    let access_token = get_access_token(&client, &address, &mock_emailer, test_email).await;

    // Create invalid file data
    let invalid_data = b"This is not an image";

    let form = multipart::Form::new()
        .part(
            "card",
            multipart::Part::bytes(invalid_data.to_vec())
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

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn test_upload_card_wrong_content_type(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool).await;
    let client = reqwest::Client::new();
    let test_email = "test@mails.tsinghua.edu.cn";

    // Get access token
    let access_token = get_access_token(&client, &address, &mock_emailer, test_email).await;

    // Create test image but with wrong content type
    let image_data = create_test_image();

    let form = multipart::Form::new()
        .part(
            "card",
            multipart::Part::bytes(image_data)
                .file_name("card.txt")
                .mime_str("text/plain")
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

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn test_upload_card_unauthorized(pool: PgPool) {
    let (address, _) = spawn_app(pool).await;
    let client = reqwest::Client::new();

    // Create test image
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
        .multipart(form)
        .send()
        .await
        .expect("Failed to upload card");

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn test_upload_card_missing_grade(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool).await;
    let client = reqwest::Client::new();
    let test_email = "test@mails.tsinghua.edu.cn";

    // Get access token
    let access_token = get_access_token(&client, &address, &mock_emailer, test_email).await;

    // Create test image
    let image_data = create_test_image();

    let form = multipart::Form::new().part(
        "card",
        multipart::Part::bytes(image_data)
            .file_name("card.png")
            .mime_str("image/png")
            .unwrap(),
    );

    let response = client
        .post(format!("{address}/api/upload/card"))
        .header("Authorization", format!("Bearer {access_token}"))
        .multipart(form)
        .send()
        .await
        .expect("Failed to upload card");

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn test_upload_card_invalid_grade(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool).await;
    let client = reqwest::Client::new();
    let test_email = "test@mails.tsinghua.edu.cn";

    // Get access token
    let access_token = get_access_token(&client, &address, &mock_emailer, test_email).await;

    // Create test image
    let image_data = create_test_image();

    let form = multipart::Form::new()
        .part(
            "card",
            multipart::Part::bytes(image_data)
                .file_name("card.png")
                .mime_str("image/png")
                .unwrap(),
        )
        .text("grade", "invalid_grade");

    let response = client
        .post(format!("{address}/api/upload/card"))
        .header("Authorization", format!("Bearer {access_token}"))
        .multipart(form)
        .send()
        .await
        .expect("Failed to upload card");

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
}
