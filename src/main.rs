//! # Hilo Application Entry Point
//!
//! ## Environment Variables
//!
//! - `DATABASE_URL` - PostgreSQL connection string (required)
//! - `ADDRESS` - Server bind address (required)
//! - `RUST_LOG` - Logging level (optional, defaults to `info`)
//! - `LOG_FORMAT` - Log format, either `json` or `plain` (optional, defaults to `plain`)
//! - `NO_COLOR` - If set, disables colored log output (optional)

use std::env;

use hilo::app;
use sqlx::PgPool;
use tokio::net::TcpListener;
use tracing::{info, instrument, warn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[tokio::main]
#[instrument]
async fn main() {
    dotenvy::dotenv().ok(); // doesn't override existing env vars

    // 1. Set up tracing subscriber for logging
    init_tracing();

    // 2. Connect to PostgreSQL database and construct app
    let db_pool = PgPool::connect(
        &env::var("DATABASE_URL").expect("Env variable `DATABASE_URL` should be set"),
    )
    .await
    .expect("Failed to connect to Postgres");

    info!("Connected to PostgreSQL database");
    let app = app(db_pool);

    // 3. Start server at specified address
    let addr = env::var("ADDRESS").expect("Env variable `ADDRESS` should be set");
    let listener = TcpListener::bind(&addr).await.unwrap();
    info!("Server starting at http://{}", addr);

    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}

/// Initialize tracing with environment-based configuration
///
/// Supports both structured JSON logging and human-readable console output
/// based on environment variables.
fn init_tracing() {
    let env_filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(tracing::Level::INFO.into())
        .from_env_lossy();

    let format_layer = match env::var("LOG_FORMAT").as_deref() {
        Ok("json") => {
            let formatting_layer = tracing_bunyan_formatter::BunyanFormattingLayer::new(
                "hilo".into(),
                std::io::stdout,
            );
            Some(Box::new(formatting_layer) as Box<dyn tracing_subscriber::Layer<_> + Send + Sync>)
        }
        _ => {
            let formatting_layer = tracing_subscriber::fmt::layer()
                .with_file(true)
                .with_line_number(true)
                .with_target(false);
            Some(Box::new(formatting_layer) as Box<dyn tracing_subscriber::Layer<_> + Send + Sync>)
        }
    };

    let registry = tracing_subscriber::registry().with(env_filter);

    if let Some(layer) = format_layer {
        registry.with(layer).init();
    } else {
        registry.init();
    }

    info!("Tracing initialized");
}
