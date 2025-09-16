use serde_json::Value;
use serde_json::json;
use sqlx::PgPool;

mod common;

#[sqlx::test]
async fn test_admin_user_overview(db_pool: PgPool) {
    // Create test users
    let test_email1 = "test1@mails.tsinghua.edu.cn";
    let test_email2 = "test2@mails.tsinghua.edu.cn";

    let _user1_id = sqlx::query_scalar!(
        r#"INSERT INTO users (email, status) VALUES ($1, 'verified') RETURNING id"#,
        test_email1
    )
    .fetch_one(&db_pool)
    .await
    .unwrap();

    let _user2_id = sqlx::query_scalar!(
        r#"INSERT INTO users (email, status) VALUES ($1, 'form_completed') RETURNING id"#,
        test_email2
    )
    .fetch_one(&db_pool)
    .await
    .unwrap();

    // Test user overview endpoint
    let app = common::spawn_admin_app(db_pool.clone()).await;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/api/admin/users", app.address))
        .send()
        .await
        .expect("Failed to execute request");

    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();

    // Check pagination info
    assert!(body.get("data").is_some());
    assert!(body.get("pagination").is_some());

    let users = body["data"].as_array().unwrap();
    assert_eq!(users.len(), 2);

    // Check user fields
    let user = &users[0];
    assert!(user.get("id").is_some());
    assert!(user.get("email").is_some());
    assert!(user.get("status").is_some());
}

#[sqlx::test]
async fn test_admin_user_detail(db_pool: PgPool) {
    // Create test user
    let test_email = "test@mails.tsinghua.edu.cn";

    let user_id = sqlx::query_scalar!(
        r#"INSERT INTO users (email, status, wechat_id, grade) VALUES ($1, 'verified', 'test_wechat', 'Graduate') RETURNING id"#,
        test_email
    )
    .fetch_one(&db_pool)
    .await
    .unwrap();

    // Test user detail endpoint
    let app = common::spawn_admin_app(db_pool.clone()).await;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/api/admin/user", app.address))
        .json(&json!({ "user_id": user_id }))
        .send()
        .await
        .expect("Failed to execute request");

    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();

    // Check user detail fields
    assert_eq!(body["email"], test_email);
    assert_eq!(body["status"], "verified");
    assert_eq!(body["wechat_id"], "test_wechat");
    assert_eq!(body["grade"], "Graduate");
    assert!(body.get("created_at").is_some());
    assert!(body.get("updated_at").is_some());
    assert!(body["form"].is_null()); // No form submitted
}

#[sqlx::test]
async fn test_admin_user_stats(db_pool: PgPool) {
    // Create test users with forms
    let male_email = "male@mails.tsinghua.edu.cn";
    let female_email = "female@mails.tsinghua.edu.cn";

    let male_user_id = sqlx::query_scalar!(
        r#"INSERT INTO users (email, status) VALUES ($1, 'form_completed') RETURNING id"#,
        male_email
    )
    .fetch_one(&db_pool)
    .await
    .unwrap();

    let female_user_id = sqlx::query_scalar!(
        r#"INSERT INTO users (email, status) VALUES ($1, 'matched') RETURNING id"#,
        female_email
    )
    .fetch_one(&db_pool)
    .await
    .unwrap();

    // Create forms for these users
    sqlx::query!(
        r#"INSERT INTO forms (user_id, gender, familiar_tags, aspirational_tags, recent_topics, self_traits, ideal_traits, physical_boundary, self_intro)
           VALUES ($1, 'male', '{}', '{}', 'test topics', '{}', '{}', 2, 'test intro')"#,
        male_user_id
    )
    .execute(&db_pool)
    .await
    .unwrap();

    sqlx::query!(
        r#"INSERT INTO forms (user_id, gender, familiar_tags, aspirational_tags, recent_topics, self_traits, ideal_traits, physical_boundary, self_intro)
           VALUES ($1, 'female', '{}', '{}', 'test topics', '{}', '{}', 2, 'test intro')"#,
        female_user_id
    )
    .execute(&db_pool)
    .await
    .unwrap();

    // Test user stats endpoint
    let app = common::spawn_admin_app(db_pool.clone()).await;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/api/admin/stats", app.address))
        .send()
        .await
        .expect("Failed to execute request");

    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();

    // Check stats fields
    assert_eq!(body["total_users"], 2);
    assert_eq!(body["males"], 1);
    assert_eq!(body["females"], 1);
    assert_eq!(body["unmatched_males"], 1); // form_completed status
    assert_eq!(body["unmatched_females"], 0); // matched status
}

#[sqlx::test]
async fn test_admin_tags_with_stats(db_pool: PgPool) {
    // Create test user with form
    let test_email = "test@mails.tsinghua.edu.cn";

    let user_id = sqlx::query_scalar!(
        r#"INSERT INTO users (email, status) VALUES ($1, 'form_completed') RETURNING id"#,
        test_email
    )
    .fetch_one(&db_pool)
    .await
    .unwrap();

    // Create form with some tags
    sqlx::query!(
        r#"INSERT INTO forms (user_id, gender, familiar_tags, aspirational_tags, recent_topics, self_traits, ideal_traits, physical_boundary, self_intro)
           VALUES ($1, 'male', $2, $3, 'test topics', '{}', '{}', 2, 'test intro')"#,
        user_id,
        &vec!["sports".to_string(), "basketball".to_string()],
        &vec!["music".to_string()]
    )
    .execute(&db_pool)
    .await
    .unwrap();

    // Test tags with stats endpoint
    let app = common::spawn_admin_app(db_pool.clone()).await;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/api/admin/tags", app.address))
        .send()
        .await
        .expect("Failed to execute request");

    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();

    // Should return an array of tag structures
    let tags = body.as_array().unwrap();
    assert!(!tags.is_empty());

    // Check tag structure
    let first_tag = &tags[0];
    assert!(first_tag.get("id").is_some());
    assert!(first_tag.get("name").is_some());
    assert!(first_tag.get("is_matchable").is_some());
    assert!(first_tag.get("user_count").is_some());
}

#[sqlx::test]
async fn test_admin_final_matches(db_pool: PgPool) {
    // Create test users
    let user1_email = "user1@mails.tsinghua.edu.cn";
    let user2_email = "user2@mails.tsinghua.edu.cn";

    let user1_id = sqlx::query_scalar!(
        r#"INSERT INTO users (email, status) VALUES ($1, 'matched') RETURNING id"#,
        user1_email
    )
    .fetch_one(&db_pool)
    .await
    .unwrap();

    let user2_id = sqlx::query_scalar!(
        r#"INSERT INTO users (email, status) VALUES ($1, 'matched') RETURNING id"#,
        user2_email
    )
    .fetch_one(&db_pool)
    .await
    .unwrap();

    // Create final match (ensure user_a_id < user_b_id for constraint)
    let (smaller_id, larger_id) = if user1_id < user2_id {
        (user1_id, user2_id)
    } else {
        (user2_id, user1_id)
    };

    sqlx::query!(
        r#"INSERT INTO final_matches (user_a_id, user_b_id, score) VALUES ($1, $2, 0.85)"#,
        smaller_id,
        larger_id
    )
    .execute(&db_pool)
    .await
    .unwrap();

    // Test final matches endpoint
    let app = common::spawn_admin_app(db_pool.clone()).await;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/api/admin/matches", app.address))
        .send()
        .await
        .expect("Failed to execute request");

    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();

    // Check pagination and data
    assert!(body.get("data").is_some());
    assert!(body.get("pagination").is_some());

    let matches = body["data"].as_array().unwrap();
    assert_eq!(matches.len(), 1);

    // Check match fields
    let match_record = &matches[0];
    assert!(match_record.get("id").is_some());
    assert!(match_record.get("user_a_id").is_some());
    assert!(match_record.get("user_a_email").is_some());
    assert!(match_record.get("user_b_id").is_some());
    assert!(match_record.get("user_b_email").is_some());
    assert_eq!(match_record["score"], 0.85);
}
