use hilo::app;
use tokio::net::TcpListener;
// use tracing::info;
// use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    // tracing_subscriber::registry()
    //     .with(
    //         tracing_subscriber::EnvFilter::try_from_default_env()
    //             .unwrap_or_else(|_| "axum_project=debug".into()),
    //     )
    //     .with(tracing_subscriber::fmt::layer())
    //     .init();

    dotenvy::dotenv().ok();

    let app = app();
    // info!("Server starting at http://{}", addr);

    let listener = TcpListener::bind("0.0.0.0:8090").await.unwrap();

    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}
