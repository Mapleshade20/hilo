mod common;

use common::*;

/// Comprehensive integration test for final match partner workflow
/// Tests the complete flow: view profile -> partner image access control -> accept/reject match
#[sqlx::test]
async fn test_partner_workflow_complete(pool: sqlx::PgPool) {
    let (address, mock_emailer) = spawn_app(pool).await;
    let client = reqwest::Client::new();

    // Setup two matched users
    let (male_token, female_token) =
        setup_two_matched_users(&client, &address, &mock_emailer).await;

    // Test 1: View profiles with final match info
    test_view_profiles(&client, &address, &male_token, &female_token).await;

    // Test 2: Partner image access control
    test_partner_image_access_control(&client, &address, &male_token, &female_token).await;

    // Test 3: Accept final match scenario
    test_accept_final_match(&client, &address, &male_token).await;
}

/// Test that both users can view their profiles and get partner info
async fn test_view_profiles(
    client: &reqwest::Client,
    address: &str,
    male_token: &str,
    female_token: &str,
) {
    // Male user views profile
    let response = client
        .get(format!("{address}/api/profile"))
        .header("Authorization", format!("Bearer {male_token}"))
        .send()
        .await
        .expect("Failed to get male profile");

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let male_profile: serde_json::Value = response.json().await.expect("Failed to parse profile");

    // Verify male profile has final_match with partner info
    assert_eq!(male_profile["status"], "matched");
    assert!(
        male_profile["final_match"].is_object(),
        "Male should have final_match"
    );
    let final_match = &male_profile["final_match"];
    assert!(
        final_match["email_domain"].is_string(),
        "Should have partner email domain"
    );
    assert!(
        final_match["familiar_tags"].is_array(),
        "Should have partner familiar tags"
    );
    assert!(
        final_match["aspirational_tags"].is_array(),
        "Should have partner aspirational tags"
    );
    assert!(
        final_match["self_intro"].is_string(),
        "Should have partner self intro"
    );
    assert!(
        final_match["photo_url"].is_string(),
        "Should have partner photo URL"
    );

    // Female user views profile
    let response = client
        .get(format!("{address}/api/profile"))
        .header("Authorization", format!("Bearer {female_token}"))
        .send()
        .await
        .expect("Failed to get female profile");

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let female_profile: serde_json::Value = response.json().await.expect("Failed to parse profile");

    // Verify female profile has final_match with partner info
    assert_eq!(female_profile["status"], "matched");
    assert!(
        female_profile["final_match"].is_object(),
        "Female should have final_match"
    );
    let final_match = &female_profile["final_match"];
    assert!(
        final_match["email_domain"].is_string(),
        "Should have partner email domain"
    );
    assert!(
        final_match["familiar_tags"].is_array(),
        "Should have partner familiar tags"
    );
    assert!(
        final_match["aspirational_tags"].is_array(),
        "Should have partner aspirational tags"
    );
    assert!(
        final_match["self_intro"].is_string(),
        "Should have partner self intro"
    );
    assert!(
        final_match["photo_url"].is_string(),
        "Should have partner photo URL"
    );

    println!("✓ Both users can view profiles with final match info");
}

/// Test partner image access control
async fn test_partner_image_access_control(
    client: &reqwest::Client,
    address: &str,
    male_token: &str,
    female_token: &str,
) {
    // First get the partner photo URLs from profiles
    let male_response = client
        .get(format!("{address}/api/profile"))
        .header("Authorization", format!("Bearer {male_token}"))
        .send()
        .await
        .expect("Failed to get male profile");
    let male_profile: serde_json::Value =
        male_response.json().await.expect("Failed to parse profile");
    let male_partner_photo_url = male_profile["final_match"]["photo_url"]
        .as_str()
        .expect("Male should have partner photo URL");

    let female_response = client
        .get(format!("{address}/api/profile"))
        .header("Authorization", format!("Bearer {female_token}"))
        .send()
        .await
        .expect("Failed to get female profile");
    let female_profile: serde_json::Value = female_response
        .json()
        .await
        .expect("Failed to parse profile");
    let female_partner_photo_url = female_profile["final_match"]["photo_url"]
        .as_str()
        .expect("Female should have partner photo URL");

    // Test 1: Male user can access female's image
    let status = access_partner_image(client, address, male_token, male_partner_photo_url).await;
    assert_eq!(
        status,
        reqwest::StatusCode::OK,
        "Male should access partner image"
    );

    // Test 2: Female user can access male's image
    let status =
        access_partner_image(client, address, female_token, female_partner_photo_url).await;
    assert_eq!(
        status,
        reqwest::StatusCode::OK,
        "Female should access partner image"
    );

    // Test 3: Unauthorized access - create a fake UUID-based filename
    let fake_image_url = "/api/images/partner/550e8400-e29b-41d4-a716-446655440000.png";
    let status = access_partner_image(client, address, male_token, fake_image_url).await;
    assert_eq!(
        status,
        reqwest::StatusCode::FORBIDDEN,
        "Should deny access to non-partner image"
    );

    println!("✓ Partner image access control working correctly");
}

/// Test accepting final match
async fn test_accept_final_match(client: &reqwest::Client, address: &str, user_token: &str) {
    // Accept final match
    let response = client
        .post(format!("{address}/api/final-match/accept"))
        .header("Authorization", format!("Bearer {user_token}"))
        .send()
        .await
        .expect("Failed to accept final match");

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    // Verify user status changed to confirmed
    let response = client
        .get(format!("{address}/api/profile"))
        .header("Authorization", format!("Bearer {user_token}"))
        .send()
        .await
        .expect("Failed to get profile after accept");

    let profile: serde_json::Value = response.json().await.expect("Failed to parse profile");
    assert_eq!(
        profile["status"], "confirmed",
        "User status should be confirmed after acceptance"
    );

    println!("✓ User can accept final match and status updates correctly");
}

/// Test rejecting final match (separate test with fresh users)
#[sqlx::test]
async fn test_reject_final_match(pool: sqlx::PgPool) {
    let (address, mock_emailer) = spawn_app(pool).await;
    let client = reqwest::Client::new();

    // Setup two matched users
    let (male_token, female_token) =
        setup_two_matched_users(&client, &address, &mock_emailer).await;

    // Get the partner photo URL from profile
    let male_response = client
        .get(format!("{address}/api/profile"))
        .header("Authorization", format!("Bearer {male_token}"))
        .send()
        .await
        .expect("Failed to get male profile");
    let male_profile: serde_json::Value =
        male_response.json().await.expect("Failed to parse profile");
    let male_partner_photo_url = male_profile["final_match"]["photo_url"]
        .as_str()
        .expect("Male should have partner photo URL");

    // Test that photo is accessible before rejection
    let status = access_partner_image(&client, &address, &male_token, male_partner_photo_url).await;
    assert_eq!(
        status,
        reqwest::StatusCode::OK,
        "Should access partner image"
    );

    // Female user rejects the match
    let response = client
        .post(format!("{address}/api/final-match/reject"))
        .header("Authorization", format!("Bearer {female_token}"))
        .send()
        .await
        .expect("Failed to reject final match");

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    // Verify both users' statuses reverted to form_completed
    let male_response = client
        .get(format!("{address}/api/profile"))
        .header("Authorization", format!("Bearer {male_token}"))
        .send()
        .await
        .expect("Failed to get male profile after reject");

    let male_profile: serde_json::Value = male_response
        .json()
        .await
        .expect("Failed to parse male profile");
    assert_eq!(
        male_profile["status"], "form_completed",
        "Male status should revert to form_completed"
    );
    assert!(
        male_profile["final_match"].is_null(),
        "Male should not have final_match after rejection"
    );

    let female_response = client
        .get(format!("{address}/api/profile"))
        .header("Authorization", format!("Bearer {female_token}"))
        .send()
        .await
        .expect("Failed to get female profile after reject");

    let female_profile: serde_json::Value = female_response
        .json()
        .await
        .expect("Failed to parse female profile");
    assert_eq!(
        female_profile["status"], "form_completed",
        "Female status should revert to form_completed"
    );
    assert!(
        female_profile["final_match"].is_null(),
        "Female should not have final_match after rejection"
    );

    // Test that partner image access is now denied
    let status = access_partner_image(&client, &address, &male_token, male_partner_photo_url).await;
    assert_eq!(
        status,
        reqwest::StatusCode::FORBIDDEN,
        "Should deny image access after match rejection"
    );

    println!("✓ User can reject final match and both users revert to form_completed status");
}

/// Test edge case: unauthorized profile viewing for unmatched users
#[sqlx::test]
async fn test_unmatched_user_profile(pool: sqlx::PgPool) {
    let (address, mock_emailer) = spawn_app(pool).await;
    let client = reqwest::Client::new();

    // Create a user that is not matched
    let unmatched_email = "unmatched@mails.tsinghua.edu.cn";
    let unmatched_token = get_access_token(&client, &address, &mock_emailer, unmatched_email).await;

    // Upload card and get verified
    upload_card(&client, &address, &unmatched_token).await;
    assert!(admin_verify_user(&client, &address, unmatched_email, "verified").await);

    // Submit form but don't trigger matching
    let response = client
        .post(format!("{address}/api/form"))
        .header("Authorization", format!("Bearer {unmatched_token}"))
        .json(&create_male_form_submission())
        .send()
        .await
        .expect("Failed to submit form");
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    // Check profile - should have no final_match
    let response = client
        .get(format!("{address}/api/profile"))
        .header("Authorization", format!("Bearer {unmatched_token}"))
        .send()
        .await
        .expect("Failed to get unmatched user profile");

    let profile: serde_json::Value = response.json().await.expect("Failed to parse profile");
    assert_eq!(profile["status"], "form_completed");
    assert!(
        profile["final_match"].is_null(),
        "Unmatched user should not have final_match"
    );

    println!("✓ Unmatched user profile correctly shows no final match info");
}
