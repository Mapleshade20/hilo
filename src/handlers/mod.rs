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

mod auth;
mod form;
mod health_check;
mod profile;
mod upload_card;
mod upload_profile_photo;

pub use auth::*;
pub use form::*;
pub use health_check::*;
pub use profile::*;
pub use upload_card::*;
pub use upload_profile_photo::*;
