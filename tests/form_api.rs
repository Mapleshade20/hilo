mod common;

use std::sync::Arc;

use common::{MockEmailer, create_test_image, get_access_token, spawn_app};
use hilo::models::{Form, Gender, UserStatus};
use reqwest::multipart;
use serde_json::{Value, json};
use sqlx::PgPool;

fn create_male_form_submission() -> serde_json::Value {
    json!({
        "wechat_id": "test_wechat_123",
        "gender": "male",
        "familiar_tags": ["basketball", "pc_fps", "japanese"],
        "aspirational_tags": ["cooking", "study_together"],
        "recent_topics": "I've been really interested in machine learning and AI lately, especially large language models and their applications in natural language processing.",
        "self_traits": ["humor", "curiosity", "reliable"],
        "ideal_traits": ["humor", "curiosity", "reliable"],
        "physical_boundary": 2,
        "self_intro": "Hi! I'm a computer science student who loves sports and gaming. I enjoy learning new technologies and meeting interesting people.",
    })
}

fn create_female_form_submission() -> serde_json::Value {
    json!({
        "wechat_id": "test_wechat_456",
        "gender": "female",
        "familiar_tags": ["dance", "cooking", "china"],
        "aspirational_tags": ["wild", "board_games"],
        "recent_topics": "I've been exploring different cooking techniques from various cultures and really enjoying the process of creating new dishes.",
        "self_traits": ["humor", "curiosity", "reliable"],
        "ideal_traits": ["humor", "curiosity", "reliable"],
        "physical_boundary": 1,
        "self_intro": "I'm passionate about arts and culture. I love dancing, cooking, and exploring new places with interesting people."
    })
}

async fn create_form_with_profile_photo(
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
        "familiar_tags": ["dance", "cooking", "china"],
        "aspirational_tags": ["wild", "board_games"],
        "recent_topics": "I've been exploring different cooking techniques from various cultures and really enjoying the process of creating new dishes.",
        "self_traits": ["humor", "curiosity", "reliable"],
        "ideal_traits": ["humor", "curiosity", "reliable"],
        "physical_boundary": 1,
        "self_intro": "I'm passionate about arts and culture. I love dancing, cooking, and exploring new places with interesting people.",
        "profile_photo_filename": filename
    })
}

async fn setup_verified_user(
    client: &reqwest::Client,
    address: &str,
    mock_emailer: &Arc<MockEmailer>,
    pool: &PgPool,
    email: &str,
) -> String {
    let access_token = get_access_token(client, address, mock_emailer, email).await;

    // Update the user status to verified
    sqlx::query!(
        "UPDATE users SET status = 'verified' WHERE email = $1",
        email
    )
    .execute(pool)
    .await
    .expect("Failed to update user status");

    access_token
}

#[sqlx::test]
async fn test_submit_form_success(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool.clone()).await;
    let client = reqwest::Client::new();
    let test_email = "test@mails.tsinghua.edu.cn";

    let access_token =
        setup_verified_user(&client, &address, &mock_emailer, &pool, test_email).await;

    let form_data = create_form_with_profile_photo(&client, &address, &access_token).await;

    let response = client
        .post(format!("{}/api/form", &address))
        .header("Authorization", format!("Bearer {access_token}"))
        .json(&form_data)
        .send()
        .await
        .expect("Failed to submit form");

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.text().await.expect("Failed to read response");
    assert_eq!(body, "Form submitted successfully");

    // Verify user status was updated to form_completed
    let user_status = sqlx::query!(
        r#"SELECT status as "status: UserStatus" FROM users WHERE email = $1"#,
        test_email
    )
    .fetch_one(&pool)
    .await
    .expect("Failed to fetch user status");
    assert_eq!(user_status.status, UserStatus::FormCompleted);
}

#[sqlx::test]
async fn test_submit_form_upsert_behavior(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool.clone()).await;
    let client = reqwest::Client::new();
    let test_email = "test@mails.tsinghua.edu.cn";

    let access_token =
        setup_verified_user(&client, &address, &mock_emailer, &pool, test_email).await;

    // Submit first form
    let first_form = create_male_form_submission();
    let response = client
        .post(format!("{}/api/form", &address))
        .header("Authorization", format!("Bearer {access_token}"))
        .json(&first_form)
        .send()
        .await
        .expect("Failed to submit first form");
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    // Verify the status is form_completed
    let user_status = sqlx::query!(
        r#"SELECT status as "status: UserStatus" FROM users WHERE email = $1"#,
        test_email
    )
    .fetch_one(&pool)
    .await
    .expect("Failed to fetch user status");
    assert_eq!(user_status.status, UserStatus::FormCompleted);

    // Submit updated form
    let mut updated_form = create_female_form_submission();
    updated_form["recent_topics"] =
        json!("Updated interests: Now I'm into sustainable cooking and zero-waste lifestyle.");

    let response = client
        .post(format!("{}/api/form", &address))
        .header("Authorization", format!("Bearer {access_token}"))
        .json(&updated_form)
        .send()
        .await
        .expect("Failed to submit updated form");
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    // Verify the form was updated, not duplicated
    let form_count = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM forms WHERE user_id = (SELECT id FROM users WHERE email = $1)",
        test_email
    )
    .fetch_one(&pool)
    .await
    .expect("Failed to count forms");
    assert_eq!(form_count, Some(1));

    // Verify the content was actually updated
    let form = sqlx::query!(
        r#"SELECT recent_topics, gender as "gender: Gender" FROM forms WHERE user_id = (SELECT id FROM users WHERE email = $1)"#,
        test_email
    )
    .fetch_one(&pool)
    .await
    .expect("Failed to fetch form");
    assert!(form.recent_topics.contains("sustainable cooking"));
    assert_eq!(form.gender, Gender::Female);
}

#[sqlx::test]
async fn test_get_form_success(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool.clone()).await;
    let client = reqwest::Client::new();
    let test_email = "test@mails.tsinghua.edu.cn";

    let access_token =
        setup_verified_user(&client, &address, &mock_emailer, &pool, test_email).await;

    // Submit a form first
    let form_data = create_male_form_submission();
    let response = client
        .post(format!("{}/api/form", &address))
        .header("Authorization", format!("Bearer {access_token}"))
        .json(&form_data)
        .send()
        .await
        .expect("Failed to submit form");
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    // Now retrieve the form
    let response = client
        .get(format!("{}/api/form", &address))
        .header("Authorization", format!("Bearer {access_token}"))
        .send()
        .await
        .expect("Failed to get form");

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let retrieved_form: Form = response.json().await.expect("Failed to parse form");
    assert_eq!(retrieved_form.gender, Gender::Male);
    assert_eq!(
        retrieved_form.familiar_tags,
        vec!["basketball", "pc_fps", "japanese"]
    );
    assert_eq!(
        retrieved_form.aspirational_tags,
        vec!["cooking", "study_together"]
    );
    assert_eq!(retrieved_form.physical_boundary, 2);
    assert!(retrieved_form.recent_topics.contains("machine learning"));
    assert!(
        retrieved_form
            .self_intro
            .contains("computer science student")
    );
}

#[sqlx::test]
async fn test_get_form_not_found(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool.clone()).await;
    let client = reqwest::Client::new();
    let test_email = "test@mails.tsinghua.edu.cn";

    let access_token =
        setup_verified_user(&client, &address, &mock_emailer, &pool, test_email).await;

    // Try to get form without submitting one first
    let response = client
        .get(format!("{}/api/form", &address))
        .header("Authorization", format!("Bearer {access_token}"))
        .send()
        .await
        .expect("Failed to get form");

    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);
    let body = response.text().await.expect("Failed to read response");
    assert_eq!(body, "Form not found");
}

#[sqlx::test]
async fn test_submit_form_unauthorized(pool: PgPool) {
    let (address, _) = spawn_app(pool).await;
    let client = reqwest::Client::new();

    let form_data = create_male_form_submission();

    // Test without any authorization header
    let response = client
        .post(format!("{}/api/form", &address))
        .json(&form_data)
        .send()
        .await
        .expect("Failed to make request");

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);

    // Test with invalid token
    let response = client
        .post(format!("{}/api/form", &address))
        .header("Authorization", "Bearer invalid-token")
        .json(&form_data)
        .send()
        .await
        .expect("Failed to make request");

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn test_submit_form_user_status_forbidden(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool.clone()).await;
    let client = reqwest::Client::new();
    let test_email = "test@mails.tsinghua.edu.cn";

    let access_token = get_access_token(&client, &address, &mock_emailer, test_email).await;

    // User starts as 'unverified', should not be able to submit form
    let form_data = create_male_form_submission();

    let response = client
        .post(format!("{}/api/form", &address))
        .header("Authorization", format!("Bearer {access_token}"))
        .json(&form_data)
        .send()
        .await
        .expect("Failed to submit form");

    assert_eq!(response.status(), reqwest::StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn test_submit_form_too_many_tags(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool.clone()).await;
    let client = reqwest::Client::new();
    let test_email = "test@mails.tsinghua.edu.cn";

    let access_token =
        setup_verified_user(&client, &address, &mock_emailer, &pool, test_email).await;

    // Create form with too many tags (TOTAL_TAGS limit is 10)
    let mut form_data = create_male_form_submission();
    form_data["familiar_tags"] = json!([
        "basketball",
        "volleyball",
        "pc_fps",
        "japanese",
        "soccer",
        "badminton"
    ]);
    form_data["aspirational_tags"] = json!([
        "cooking",
        "study_together",
        "drawing_photo",
        "crafts",
        "instruments",
        "dance"
    ]);

    let response = client
        .post(format!("{}/api/form", &address))
        .header("Authorization", format!("Bearer {access_token}"))
        .json(&form_data)
        .send()
        .await
        .expect("Failed to submit form");

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    let body = response.text().await.expect("Failed to read response");
    assert!(body.contains("Total tags cannot exceed"));
}

#[sqlx::test]
async fn test_submit_form_invalid_tags(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool.clone()).await;
    let client = reqwest::Client::new();
    let test_email = "test@mails.tsinghua.edu.cn";

    let access_token =
        setup_verified_user(&client, &address, &mock_emailer, &pool, test_email).await;

    // Create form with invalid tags
    let mut form_data = create_male_form_submission();
    form_data["familiar_tags"] = json!(["invalid_tag", "pc_fps"]);

    let response = client
        .post(format!("{}/api/form", &address))
        .header("Authorization", format!("Bearer {access_token}"))
        .json(&form_data)
        .send()
        .await
        .expect("Failed to submit form");

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    let body = response.text().await.expect("Failed to read response");
    assert!(body.contains("Invalid familiar tag"));

    // Test with non-matchable tags
    form_data["familiar_tags"] = json!(["desktop"]);

    let response = client
        .post(format!("{}/api/form", &address))
        .header("Authorization", format!("Bearer {access_token}"))
        .json(&form_data)
        .send()
        .await
        .expect("Failed to submit form");

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    let body = response.text().await.expect("Failed to read response");
    assert!(body.contains("Invalid familiar tag"));

    // Test with internal duplicate tags
    form_data["familiar_tags"] = json!(["pc_fps", "pc_fps"]);

    let response = client
        .post(format!("{}/api/form", &address))
        .header("Authorization", format!("Bearer {access_token}"))
        .json(&form_data)
        .send()
        .await
        .expect("Failed to submit form");

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    let body = response.text().await.expect("Failed to read response");
    assert!(body.contains("Duplicate tag"));

    // Test with duplicate tags between familiar and aspirational
    form_data["familiar_tags"] = json!(["pc_fps"]);
    form_data["aspirational_tags"] = json!(["pc_fps", "cooking"]);

    let response = client
        .post(format!("{}/api/form", &address))
        .header("Authorization", format!("Bearer {access_token}"))
        .json(&form_data)
        .send()
        .await
        .expect("Failed to submit form");

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    let body = response.text().await.expect("Failed to read response");
    assert!(body.contains("Duplicate tag"));
}

#[sqlx::test]
async fn test_submit_form_invalid_fields(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool.clone()).await;
    let client = reqwest::Client::new();
    let test_email = "test@mails.tsinghua.edu.cn";

    let access_token =
        setup_verified_user(&client, &address, &mock_emailer, &pool, test_email).await;

    let mut form_data = create_male_form_submission();
    form_data["gender"] = json!("invalid-gender");

    let response = client
        .post(format!("{}/api/form", &address))
        .header("Authorization", format!("Bearer {access_token}"))
        .json(&form_data)
        .send()
        .await
        .expect("Failed to submit form");

    assert_eq!(response.status(), reqwest::StatusCode::UNPROCESSABLE_ENTITY);

    let test_cases = vec![0, 5, -1, 100];
    for invalid_boundary in test_cases {
        let mut form_data = create_male_form_submission();
        form_data["physical_boundary"] = json!(invalid_boundary);

        let response = client
            .post(format!("{}/api/form", &address))
            .header("Authorization", format!("Bearer {access_token}"))
            .json(&form_data)
            .send()
            .await
            .expect("Failed to submit form");

        assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    }
}

#[sqlx::test]
async fn test_submit_form_invalid_wechat_id(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool.clone()).await;
    let client = reqwest::Client::new();
    let test_email = "test@mails.tsinghua.edu.cn";

    let access_token =
        setup_verified_user(&client, &address, &mock_emailer, &pool, test_email).await;

    // Test empty wechat_id
    let mut form_data = create_male_form_submission();
    form_data["wechat_id"] = json!("");

    let response = client
        .post(format!("{}/api/form", &address))
        .header("Authorization", format!("Bearer {access_token}"))
        .json(&form_data)
        .send()
        .await
        .expect("Failed to submit form");

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    let body = response.text().await.expect("Failed to read response");
    assert!(body.contains("wechat_id cannot be empty"));

    // Test wechat_id too long (over 100 characters)
    form_data["wechat_id"] = json!("a".repeat(101));

    let response = client
        .post(format!("{}/api/form", &address))
        .header("Authorization", format!("Bearer {access_token}"))
        .json(&form_data)
        .send()
        .await
        .expect("Failed to submit form");

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    let body = response.text().await.expect("Failed to read response");
    assert!(body.contains("wechat_id cannot exceed 100 characters"));
}

#[sqlx::test]
async fn test_submit_form_updates_wechat_id_in_users_table(pool: PgPool) {
    let (address, mock_emailer) = spawn_app(pool.clone()).await;
    let client = reqwest::Client::new();
    let test_email = "test@mails.tsinghua.edu.cn";

    let access_token =
        setup_verified_user(&client, &address, &mock_emailer, &pool, test_email).await;

    let form_data = create_male_form_submission();

    let response = client
        .post(format!("{}/api/form", &address))
        .header("Authorization", format!("Bearer {access_token}"))
        .json(&form_data)
        .send()
        .await
        .expect("Failed to submit form");

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    // Verify wechat_id was updated in users table
    let user_data = sqlx::query!("SELECT wechat_id FROM users WHERE email = $1", test_email)
        .fetch_one(&pool)
        .await
        .expect("Failed to fetch user");

    assert_eq!(user_data.wechat_id, Some("test_wechat_123".to_string()));
}
