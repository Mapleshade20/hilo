#![allow(dead_code)]

use async_trait::async_trait;
use hilo::services::email::{EmailError, EmailService};
use sqlx::PgPool;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use uuid::Uuid;

/// A mock email service that stores sent emails for testing purposes.
/// This is ideal for integration tests as it doesn't produce console output.
#[derive(Debug, Default)]
pub struct MockEmailer {
    sent_emails: Mutex<Vec<SentEmail>>,
}

#[derive(Debug, Clone)]
pub struct SentEmail {
    pub recipient: String,
    pub subject: String,
    pub body_html: String,
}

impl MockEmailer {
    pub fn new() -> Self {
        Self {
            sent_emails: Mutex::new(Vec::new()),
        }
    }

    /// Get all sent emails for testing verification
    pub fn get_sent_emails(&self) -> Vec<SentEmail> {
        self.sent_emails.lock().unwrap().clone()
    }

    /// Clear all stored emails
    pub fn clear(&self) {
        self.sent_emails.lock().unwrap().clear();
    }

    /// Get the count of sent emails
    pub fn sent_count(&self) -> usize {
        self.sent_emails.lock().unwrap().len()
    }

    /// Get the last sent email
    pub fn last_sent_email(&self) -> Option<SentEmail> {
        self.sent_emails.lock().unwrap().last().cloned()
    }
}

#[async_trait]
impl EmailService for MockEmailer {
    async fn send_email(
        &self,
        recipient: &str,
        subject: &str,
        body_html: &str,
    ) -> Result<(), EmailError> {
        let email = SentEmail {
            recipient: recipient.to_string(),
            subject: subject.to_string(),
            body_html: body_html.to_string(),
        };

        self.sent_emails.lock().unwrap().push(email);
        Ok(())
    }
}

/// Configure a test database with a unique name and run migrations
/// Returns (PgPool, database_name) for later cleanup
async fn configure_test_database() -> (PgPool, String) {
    dotenvy::dotenv().ok();

    // Get the base database URL
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for tests");

    // Parse the URL to extract the base connection info and connect to 'postgres' database
    let mut base_url = database_url.clone();
    if let Some(pos) = base_url.rfind('/') {
        base_url.truncate(pos);
    }
    let admin_url = format!("{}/postgres", base_url);

    // Create a unique test database name
    let test_db_name = format!("test_db_{}", Uuid::new_v4().simple());

    // Connect to the postgres database to create the test database
    let admin_pool = PgPool::connect(&admin_url)
        .await
        .expect("Failed to connect to admin database");

    // Create the test database
    sqlx::query(&format!("CREATE DATABASE \"{}\"", test_db_name))
        .execute(&admin_pool)
        .await
        .expect("Failed to create test database");

    admin_pool.close().await;

    // Connect to the test database
    let test_db_url = format!("{}/{}", base_url, test_db_name);
    let test_pool = PgPool::connect(&test_db_url)
        .await
        .expect("Failed to connect to test database");

    // Run migrations on the test database
    sqlx::migrate!("./migrations")
        .run(&test_pool)
        .await
        .expect("Failed to run migrations on test database");

    (test_pool, test_db_name)
}

/// Drop a test database to clean up after tests  
/// Note: The database name should be passed as it's difficult to extract from the pool
pub async fn cleanup_test_database(database_name: &str) {
    // Get the base database URL for admin connection
    let base_database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for tests");

    let mut base_url = base_database_url.clone();
    if let Some(pos) = base_url.rfind('/') {
        base_url.truncate(pos);
    }
    let admin_url = format!("{}/postgres", base_url);

    // Connect to the postgres database to drop the test database
    let admin_pool = PgPool::connect(&admin_url)
        .await
        .expect("Failed to connect to admin database");

    // Terminate any existing connections to the test database
    sqlx::query(&format!(
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{}' AND pid <> pg_backend_pid()",
        database_name
    ))
    .execute(&admin_pool)
    .await
    .ok(); // Ignore errors here

    // Drop the test database
    sqlx::query(&format!("DROP DATABASE IF EXISTS \"{}\"", database_name))
        .execute(&admin_pool)
        .await
        .expect("Failed to drop test database");

    admin_pool.close().await;
}

/// Spawns the application and returns its address and mock emailer for testing.
pub async fn spawn_app() -> (String, Arc<MockEmailer>) {
    let (test_db_pool, _test_db_name) = configure_test_database().await;

    let mock_emailer = Arc::new(MockEmailer::new());
    let mock_cloned = mock_emailer.clone();

    // Randomly choose an available port
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind random port at localhost");

    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        let app = hilo::app_with_email_service(test_db_pool, Some(mock_cloned));
        axum::serve(listener, app).await.unwrap();
    });

    let address = format!("http://127.0.0.1:{}", port);
    (address, mock_emailer)
}

/// Spawns the application and returns its address, mock emailer, and database name for cleanup.
pub async fn spawn_app_with_cleanup() -> (String, Arc<MockEmailer>, String) {
    let (test_db_pool, test_db_name) = configure_test_database().await;

    let mock_emailer = Arc::new(MockEmailer::new());
    let mock_cloned = mock_emailer.clone();

    // Randomly choose an available port
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind random port at localhost");

    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        let app = hilo::app_with_email_service(test_db_pool, Some(mock_cloned));
        axum::serve(listener, app).await.unwrap();
    });

    let address = format!("http://127.0.0.1:{}", port);
    (address, mock_emailer, test_db_name)
}
