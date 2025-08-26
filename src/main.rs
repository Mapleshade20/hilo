//! # Hilo Application Entry Point
//!
//! ## Environment Variables
//!
//! - `DATABASE_URL` - PostgreSQL connection string (required)
//! - `APP_ENV` - Application environment ("production" or "development")
//!
//! ## Server Configuration
//!
//! - **Production**: Binds to `0.0.0.0:8090` for external access
//! - **Development**: Binds to `127.0.0.1:8090` for local access only

use std::borrow::Cow;
use std::env;

use hilo::app;
use sqlx::PgPool;
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

    let db_pool = PgPool::connect(
        &env::var("DATABASE_URL").expect("Env variable `DATABASE_URL` should be set"),
    )
    .await
    .expect("Failed to connect to Postgres");
    let app = app(db_pool);

    let addr = match env::var("APP_ENV")
        .map(Cow::Owned)
        .unwrap_or_else(|_| Cow::Borrowed("development"))
        .as_ref()
    {
        "production" => "0.0.0.0:8090",
        _ => "127.0.0.1:8090",
    };
    let listener = TcpListener::bind(addr).await.unwrap();
    // info!("Server starting at http://{}", addr);

    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}
