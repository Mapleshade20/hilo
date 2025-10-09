//! # HTTP Request Handlers
//!
//! This module contains all HTTP request handlers for the Hilo application.
//! Each handler is responsible for processing specific HTTP requests and returning
//! appropriate responses.
//!
//! ## Available Handlers
//!
//! - **Authentication** (`auth`) - Email verification and JWT token management
//! - **Health Check** (`health_check`) - Application health monitoring
//! - **Profile** (`profile`) - User profile information retrieval
//! - **Form** (`form`) - User form submission and retrieval
//! - **Upload Card** (`upload_card`) - File upload functionality for student card verification
//! - **Upload Profile Photo** (`upload_profile_photo`) - Profile photo upload for verified users
//! - **Veto** (`veto`) - Match preview and veto functionality
//! - **Admin** (`admin`) - Administrative endpoints for final matching

mod admin;
mod auth;
mod final_match;
mod form;
mod partner_image;
mod profile;
mod upload_card;
mod upload_profile_photo;
mod veto;

pub use admin::admin_router;
pub use auth::*;
use axum::http::StatusCode;
pub use final_match::*;
pub use form::*;
pub use partner_image::*;
pub use profile::*;
use tracing::{instrument, trace};
pub use upload_card::*;
pub use upload_profile_photo::*;
pub use veto::*;

/// Health check endpoint that returns 200 OK.
///
/// GET /health-check
#[instrument]
pub async fn health_check() -> StatusCode {
    trace!("Health check endpoint accessed");
    StatusCode::OK
}
