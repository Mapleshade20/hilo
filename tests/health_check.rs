mod common;

use sqlx::PgPool;

use common::spawn_app;

#[sqlx::test]
async fn health_check_works(pool: PgPool) {
    let (address, _) = spawn_app(pool).await;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{address}/health-check"))
        .send()
        .await
        .expect("Failed to execute request");

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(Some(0), response.content_length());
}
