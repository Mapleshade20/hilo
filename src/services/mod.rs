//! # Business Logic Services
//!
//! This module contains the core business logic services for the Hilo application.
//! Services encapsulate domain-specific functionality and provide clean interfaces
//! for use by HTTP handlers and other application components.
//!
//! ## Available Services
//!
//! - **Email** (`email`) - Email delivery service with multiple implementations
//! - **JWT** (`jwt`) - JSON Web Token creation, validation, and management

pub mod email;
pub mod jwt;
