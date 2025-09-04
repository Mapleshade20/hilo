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
use tokio_util::sync::CancellationToken;
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

    // 3. Create a shared cancellation token
    let shutdown_token = CancellationToken::new();
    let main_shutdown = shutdown_token.child_token();
    let admin_shutdown = shutdown_token.child_token();
    let shutdown_signal = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to listen for ctrl_c signal");
    };

    // 4. Start main server
    LazyLock::force(&EMAIL_REGEX); // ensure panic happens at startup
    LazyLock::force(&TAG_SYSTEM);
    let main_db = db_pool.clone();
    let mut main_server = tokio::spawn(async move {
        let router = app(main_db);
        let addr = env::var("ADDRESS").expect("Env variable `ADDRESS` should be set");
        let listener = TcpListener::bind(&addr).await.unwrap();
        info!("Main server starting at http://{}", addr);

        axum::serve(listener, router.into_make_service())
            .with_graceful_shutdown(main_shutdown.cancelled_owned())
            .await
            .unwrap();
    });

    // 5. Start admin server
    // Admin server is protected by Cloudflare Access, so no additional auth is needed
    let mut admin_server = tokio::spawn(async move {
        let router = admin_router(db_pool);
        let addr = env::var("ADMIN_ADDRESS").expect("Env variable `ADMIN_ADDRESS` should be set");
        let listener = TcpListener::bind(&addr).await.unwrap();
        info!("Admin server starting at http://{}", addr);

        axum::serve(listener, router.into_make_service())
            .with_graceful_shutdown(admin_shutdown.cancelled_owned())
            .await
            .unwrap();
    });

    // 6. Wait for either server to complete or shutdown signal
    tokio::select! {
        _ = &mut main_server => {
            warn!("Main server terminated unexpectedly");
            shutdown_token.cancel();
        }
        _ = &mut admin_server => {
            warn!("Admin server terminated unexpectedly");
            shutdown_token.cancel();
        }
        _ = shutdown_signal => {
            info!("Ctrl-C received - initiating graceful shutdown");
            shutdown_token.cancel();
        }
    }
    let _ = main_server.await;
    let _ = admin_server.await;

    info!("Shutdown complete");
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
