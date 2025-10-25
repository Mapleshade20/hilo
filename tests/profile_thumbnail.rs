mod common;

use common::{get_access_token, spawn_app};
use hilo::{models::UserStatus, utils::constant::THUMBNAIL_SIZE};
use image::{GenericImageView, ImageBuffer, Rgb};
use reqwest::multipart;
use sqlx::PgPool;

/// Creates a valid test image using the image crate
fn create_valid_test_image() -> Vec<u8> {
    let img = ImageBuffer::from_fn(100, 100, |x, y| {
        if (x + y) % 2 == 0 {
            Rgb([255u8, 0u8, 0u8])
        } else {
            Rgb([0u8, 0u8, 255u8])
        }
    });

    let mut buffer = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut buffer),
        image::ImageFormat::Png,
    )
    .expect("Failed to encode test image");
    buffer
}

#[sqlx::test]
async fn test_fetch_thumbnail_success(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool.clone()).await;
    let client = reqwest::Client::new();
    let test_email = "test@mails.tsinghua.edu.cn";
    let other_email = "other@mails.tsinghua.edu.cn";

    // Create first user and upload their profile photo
    let access_token = get_access_token(&client, &address, &mock_emailer, test_email).await;

    // Set user to verified and create a form
    sqlx::query!(
        "UPDATE users SET status = $1 WHERE email = $2",
        UserStatus::Verified as UserStatus,
        test_email
    )
    .execute(&pool)
    .await
    .expect("Failed to update user status");

    let user_id = sqlx::query!("SELECT id FROM users WHERE email = $1", test_email)
        .fetch_one(&pool)
        .await
        .expect("Failed to fetch user")
        .id;

    // Upload profile photo
    let image_data = create_valid_test_image();
    let form = multipart::Form::new().part(
        "photo",
        multipart::Part::bytes(image_data.clone())
            .file_name("profile.png")
            .mime_str("image/png")
            .unwrap(),
    );

    let upload_response = client
        .post(format!("{address}/api/upload/profile-photo"))
        .header("Authorization", format!("Bearer {access_token}"))
        .multipart(form)
        .send()
        .await
        .expect("Failed to upload profile photo");

    assert_eq!(upload_response.status(), reqwest::StatusCode::OK);

    let upload_json: serde_json::Value = upload_response
        .json()
        .await
        .expect("Failed to parse upload response");
    let filename = upload_json["filename"].as_str().unwrap();

    // Create a form entry for the user
    sqlx::query!(
        r#"INSERT INTO forms (user_id, profile_photo_filename, gender, familiar_tags, aspirational_tags, recent_topics, self_traits, ideal_traits, physical_boundary, self_intro)
         VALUES ($1, $2, 'male', '{}', '{}', 'test topics', '{}', '{}', 2, 'test intro')"#,
        user_id,
        filename
    )
    .execute(&pool)
    .await
    .expect("Failed to create form");

    // Update user to form_completed
    sqlx::query!(
        "UPDATE users SET status = $1 WHERE email = $2",
        UserStatus::FormCompleted as UserStatus,
        test_email
    )
    .execute(&pool)
    .await
    .expect("Failed to update user status");

    // Create second user with form_completed status
    let other_token = get_access_token(&client, &address, &mock_emailer, other_email).await;
    sqlx::query!(
        "UPDATE users SET status = $1 WHERE email = $2",
        UserStatus::FormCompleted as UserStatus,
        other_email
    )
    .execute(&pool)
    .await
    .expect("Failed to update other user status");

    // Fetch thumbnail with the second user
    let thumbnail_response = client
        .get(format!("{address}/api/images/thumbnail/{user_id}"))
        .header("Authorization", format!("Bearer {other_token}"))
        .send()
        .await
        .expect("Failed to fetch thumbnail");

    assert_eq!(thumbnail_response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        thumbnail_response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("image/png")
    );

    // Verify thumbnail is an image and has reasonable size (should be smaller than original)
    let thumbnail_bytes = thumbnail_response
        .bytes()
        .await
        .expect("Failed to read thumbnail bytes");
    assert!(!thumbnail_bytes.is_empty());
    assert!(thumbnail_bytes.len() < image_data.len());

    // Verify it's a valid image with correct dimensions (THUMBNAIL_SIZE on larger dimension)
    let img = image::load_from_memory(&thumbnail_bytes).expect("Invalid thumbnail image");
    let (width, height) = img.dimensions();
    assert!(
        width == THUMBNAIL_SIZE || height == THUMBNAIL_SIZE,
        "Expected one dimension to be {}, got {}x{}",
        THUMBNAIL_SIZE,
        width,
        height
    );
    assert!(
        width <= THUMBNAIL_SIZE && height <= THUMBNAIL_SIZE,
        "Thumbnail dimensions exceed {}",
        THUMBNAIL_SIZE
    );
}

#[sqlx::test]
async fn test_fetch_thumbnail_forbidden_not_form_completed(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool.clone()).await;
    let client = reqwest::Client::new();
    let test_email = "test@mails.tsinghua.edu.cn";
    let other_email = "other@mails.tsinghua.edu.cn";

    // Create first user with form_completed status
    let _access_token = get_access_token(&client, &address, &mock_emailer, test_email).await;
    sqlx::query!(
        "UPDATE users SET status = $1 WHERE email = $2",
        UserStatus::FormCompleted as UserStatus,
        test_email
    )
    .execute(&pool)
    .await
    .expect("Failed to update user status");

    let user_id = sqlx::query!("SELECT id FROM users WHERE email = $1", test_email)
        .fetch_one(&pool)
        .await
        .expect("Failed to fetch user")
        .id;

    // Create second user with only verified status (not form_completed)
    let other_token = get_access_token(&client, &address, &mock_emailer, other_email).await;
    sqlx::query!(
        "UPDATE users SET status = $1 WHERE email = $2",
        UserStatus::Verified as UserStatus,
        other_email
    )
    .execute(&pool)
    .await
    .expect("Failed to update other user status");

    // Try to fetch thumbnail - should be forbidden
    let thumbnail_response = client
        .get(format!("{address}/api/images/thumbnail/{user_id}"))
        .header("Authorization", format!("Bearer {other_token}"))
        .send()
        .await
        .expect("Failed to fetch thumbnail");

    assert_eq!(thumbnail_response.status(), reqwest::StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn test_fetch_thumbnail_not_found_no_photo(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool.clone()).await;
    let client = reqwest::Client::new();
    let test_email = "test@mails.tsinghua.edu.cn";
    let other_email = "other@mails.tsinghua.edu.cn";

    // Create first user without profile photo
    let _access_token = get_access_token(&client, &address, &mock_emailer, test_email).await;
    sqlx::query!(
        "UPDATE users SET status = $1 WHERE email = $2",
        UserStatus::FormCompleted as UserStatus,
        test_email
    )
    .execute(&pool)
    .await
    .expect("Failed to update user status");

    let user_id = sqlx::query!("SELECT id FROM users WHERE email = $1", test_email)
        .fetch_one(&pool)
        .await
        .expect("Failed to fetch user")
        .id;

    // Create form without profile photo
    sqlx::query!(
        r#"INSERT INTO forms (user_id, profile_photo_filename, gender, familiar_tags, aspirational_tags, recent_topics, self_traits, ideal_traits, physical_boundary, self_intro)
         VALUES ($1, $2, 'male', '{}', '{}', 'test topics', '{}', '{}', 2, 'test intro')"#,
        user_id,
        None::<String>
    )
    .execute(&pool)
    .await
    .expect("Failed to create form");

    // Create second user with form_completed status
    let other_token = get_access_token(&client, &address, &mock_emailer, other_email).await;
    sqlx::query!(
        "UPDATE users SET status = $1 WHERE email = $2",
        UserStatus::FormCompleted as UserStatus,
        other_email
    )
    .execute(&pool)
    .await
    .expect("Failed to update other user status");

    // Try to fetch thumbnail - should return not found
    let thumbnail_response = client
        .get(format!("{address}/api/images/thumbnail/{user_id}"))
        .header("Authorization", format!("Bearer {other_token}"))
        .send()
        .await
        .expect("Failed to fetch thumbnail");

    assert_eq!(thumbnail_response.status(), reqwest::StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn test_fetch_thumbnail_unauthorized(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool.clone()).await;
    let client = reqwest::Client::new();
    let test_email = "test@mails.tsinghua.edu.cn";

    // Create user
    let _access_token = get_access_token(&client, &address, &mock_emailer, test_email).await;
    let user_id = sqlx::query!("SELECT id FROM users WHERE email = $1", test_email)
        .fetch_one(&pool)
        .await
        .expect("Failed to fetch user")
        .id;

    // Try to fetch thumbnail without auth token
    let thumbnail_response = client
        .get(format!("{address}/api/images/thumbnail/{user_id}"))
        .send()
        .await
        .expect("Failed to fetch thumbnail");

    assert_eq!(
        thumbnail_response.status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
}
