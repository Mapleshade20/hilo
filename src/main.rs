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
use std::sync::LazyLock;

use hilo::{
    app,
    handlers::admin_router,
    utils::static_object::{EMAIL_REGEX, TAG_SYSTEM},
};
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

    // 2. Connect to PostgreSQL database
    let db_pool = PgPool::connect(
        &env::var("DATABASE_URL").expect("Env variable `DATABASE_URL` should be set"),
    )
    .await
    .expect("Failed to connect to Postgres");

    info!("Connected to PostgreSQL database");

    // 3. Create a future that resolves on Ctrl+C
    let shutdown_signal = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to listen for ctrl_c signal");
        info!("Ctrl+C received, shutting down both servers");
    };

    // 4. Start main server
    LazyLock::force(&EMAIL_REGEX); // ensure panic happens at startup
    LazyLock::force(&TAG_SYSTEM);
    let main_db_pool = db_pool.clone();
    let main_server = tokio::spawn(async move {
        let app = app(main_db_pool);
        let addr = env::var("ADDRESS").expect("Env variable `ADDRESS` should be set");
        let listener = TcpListener::bind(&addr).await.unwrap();
        info!("Main server starting at http://{}", addr);

        axum::serve(listener, app.into_make_service())
            .await
            .unwrap();
    });

    // 5. Start admin server
    // Admin server is protected by Cloudflare Access, so no additional auth is needed
    let admin_server = tokio::spawn(async move {
        let app = admin_router(db_pool);
        let addr = env::var("ADMIN_ADDRESS").expect("Env variable `ADMIN_ADDRESS` should be set");
        let listener = TcpListener::bind(&addr).await.unwrap();
        info!("Admin server starting at http://{}", addr);

        axum::serve(listener, app.into_make_service())
            .await
            .unwrap();
    });

    // 6. Wait for either server to complete or shutdown signal
    tokio::select! {
        _ = main_server => {
            warn!("Main server terminated unexpectedly");
        }
        _ = admin_server => {
            warn!("Admin server terminated unexpectedly");
        }
        _ = shutdown_signal => {
            info!("Graceful shutdown initiated");
        }
    }
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
            Box::new(formatting_layer) as Box<dyn tracing_subscriber::Layer<_> + Send + Sync>
        }
        _ => {
            let formatting_layer = tracing_subscriber::fmt::layer().with_line_number(true);
            Box::new(formatting_layer) as Box<dyn tracing_subscriber::Layer<_> + Send + Sync>
        }
    };

    tracing_subscriber::registry()
        .with(env_filter)
        .with(format_layer)
        .init();

    info!("Tracing initialized");
}
