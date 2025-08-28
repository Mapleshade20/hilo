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

mod auth;
mod health_check;
mod profile;

pub use auth::*;
pub use health_check::*;
pub use profile::*;
