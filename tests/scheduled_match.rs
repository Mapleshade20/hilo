use hilo::models::{
    CreateScheduledMatchRequest, CreateScheduledMatchesRequest, NextMatchTimeResponse,
    ScheduledFinalMatch,
};
use time::OffsetDateTime;

mod common;
use common::{get_access_token, spawn_app};

#[sqlx::test]
async fn test_create_and_get_scheduled_matches(pool: sqlx::PgPool) {
    let (address, _) = spawn_app(pool).await;
    let client = reqwest::Client::new();

    // Create future timestamps for testing
    let now = OffsetDateTime::now_utc();
    let future_time_1 = now + time::Duration::minutes(30);
    let future_time_2 = now + time::Duration::hours(2);

    // Test creating scheduled matches
    let create_request = CreateScheduledMatchesRequest {
        scheduled_times: vec![
            CreateScheduledMatchRequest {
                scheduled_time: future_time_1,
            },
            CreateScheduledMatchRequest {
                scheduled_time: future_time_2,
            },
        ],
    };

    let response = client
        .post(format!("{}/api/admin/scheduled-matches", &address))
        .json(&create_request)
        .send()
        .await
        .expect("Failed to execute request.");

    assert_eq!(response.status().as_u16(), 201);

    let created_matches: Vec<ScheduledFinalMatch> = response
        .json()
        .await
        .expect("Failed to parse response as JSON");

    assert_eq!(created_matches.len(), 2);
    assert_eq!(
        created_matches[0].status,
        hilo::models::ScheduleStatus::Pending
    );
    assert_eq!(
        created_matches[1].status,
        hilo::models::ScheduleStatus::Pending
    );

    // Test getting all scheduled matches
    let response = client
        .get(format!("{}/api/admin/scheduled-matches", &address))
        .send()
        .await
        .expect("Failed to execute request.");

    assert_eq!(response.status().as_u16(), 200);

    let all_matches: Vec<ScheduledFinalMatch> = response
        .json()
        .await
        .expect("Failed to parse response as JSON");

    assert_eq!(all_matches.len(), 2);

    // Test cancelling a scheduled match
    let match_to_cancel = &all_matches[0];
    let response = client
        .delete(format!(
            "{}/api/admin/scheduled-matches/{}",
            &address, match_to_cancel.id
        ))
        .send()
        .await
        .expect("Failed to execute request.");

    assert_eq!(response.status().as_u16(), 200);

    let cancel_response: serde_json::Value = response
        .json()
        .await
        .expect("Failed to parse response as JSON");

    assert_eq!(cancel_response["success"], true);

    // Verify only one match remains
    let response = client
        .get(format!("{}/api/admin/scheduled-matches", &address))
        .send()
        .await
        .expect("Failed to execute request.");

    let remaining_matches: Vec<ScheduledFinalMatch> = response
        .json()
        .await
        .expect("Failed to parse response as JSON");

    assert_eq!(remaining_matches.len(), 1);
}

#[sqlx::test]
async fn test_get_next_match_time(pool: sqlx::PgPool) {
    let (address, mock_emailer) = spawn_app(pool).await;
    let client = reqwest::Client::new();
    let test_email = "test@mails.tsinghua.edu.cn";
    let access_token = get_access_token(&client, &address, &mock_emailer, test_email).await;

    // Initially no scheduled matches
    let response = client
        .get(format!("{}/api/final-match/time", &address))
        .header("Authorization", format!("Bearer {access_token}"))
        .send()
        .await
        .expect("Failed to execute request.");

    assert_eq!(response.status().as_u16(), 200);

    let next_time_response: NextMatchTimeResponse = response
        .json()
        .await
        .expect("Failed to parse response as JSON");

    assert!(next_time_response.next.is_none());

    // Create a scheduled match
    let now = OffsetDateTime::now_utc();
    let future_time = now + time::Duration::hours(1);

    let create_request = CreateScheduledMatchesRequest {
        scheduled_times: vec![CreateScheduledMatchRequest {
            scheduled_time: future_time,
        }],
    };

    let response = client
        .post(format!("{}/api/admin/scheduled-matches", &address))
        .json(&create_request)
        .send()
        .await
        .expect("Failed to execute request.");

    assert_eq!(response.status().as_u16(), 201);

    // Now check next match time
    let response = client
        .get(format!("{}/api/final-match/time", &address))
        .header("Authorization", format!("Bearer {access_token}"))
        .send()
        .await
        .expect("Failed to execute request.");

    assert_eq!(response.status().as_u16(), 200);

    let next_time_response: NextMatchTimeResponse = response
        .json()
        .await
        .expect("Failed to parse response as JSON");

    assert!(next_time_response.next.is_some());
    let returned_time = next_time_response.next.unwrap();

    // Allow for small differences due to timing
    let time_diff = (returned_time - future_time).abs();
    assert!(time_diff < time::Duration::seconds(1));
}

#[sqlx::test]
async fn test_create_scheduled_match_past_time_fails(pool: sqlx::PgPool) {
    let (address, _) = spawn_app(pool).await;
    let client = reqwest::Client::new();

    // Try to create a match in the past
    let past_time = OffsetDateTime::now_utc() - time::Duration::hours(1);

    let create_request = CreateScheduledMatchesRequest {
        scheduled_times: vec![CreateScheduledMatchRequest {
            scheduled_time: past_time,
        }],
    };

    let response = client
        .post(format!("{}/api/admin/scheduled-matches", &address))
        .json(&create_request)
        .send()
        .await
        .expect("Failed to execute request.");

    assert_eq!(response.status().as_u16(), 400);
}

#[sqlx::test]
async fn test_cancel_nonexistent_scheduled_match(pool: sqlx::PgPool) {
    let (address, _) = spawn_app(pool).await;
    let client = reqwest::Client::new();

    // Try to cancel a non-existent match
    let fake_id = uuid::Uuid::new_v4();
    let response = client
        .delete(format!(
            "{}/api/admin/scheduled-matches/{}",
            &address, fake_id
        ))
        .send()
        .await
        .expect("Failed to execute request.");

    assert_eq!(response.status().as_u16(), 404);
}
