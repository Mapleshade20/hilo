mod common;
use common::spawn_app;
// use tower::ServiceExt; // for `oneshot`

#[tokio::test]
async fn health_check_works() {
    let (address, _) = spawn_app().await;
    let client = reqwest::Client::new();

    let response = client
        .get(&format!("{}/health-check", &address))
        .send()
        .await
        .expect("Failed to execute request");

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(Some(0), response.content_length());
}
