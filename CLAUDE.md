# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Hilo** is a Rust-based social pairing backend for Project Encontrar that matches university students based on interests and preferences. It uses email verification with university domains, collects user forms (like questionnaires), and provides a matching algorithm to pair compatible users.

### Core Architecture

- **Framework**: Axum web framework with tokio async runtime
- **Database**: PostgreSQL with SQLx for compile-time checked queries
- **Authentication**: JWT tokens with refresh token support
- **Email**: Configurable email service (ExternalEmailer for production, LogEmailer for development)
- **File Storage**: Local filesystem for user-uploaded images
- **Matching System**: Background service with configurable scoring algorithm

### Key Components

- `AppState`: Shared application state containing all services and configuration
- `TagSystem`: Hierarchical tag structure loaded from `tags.json` with O(1) lookup maps
- `MatchingService`: Background service for generating match previews and final matching
- `JwtService`: JWT token generation and validation
- Email services with trait-based abstraction

## Development Commands

- **Build**: `cargo build` (debug profile only)
- **Test all**: `cargo test`
- **Test single**: `cargo test --test test_name`
- **Migrations**: `sqlx migrate run`
- **SQLx offline prepare**: `cargo sqlx prepare -- --all-targets`
- **Start postgres**: `podman-compose up -d db`
- **Reset postgres**: `podman-compose down -v`

## Environment Configuration

Required environment variables are documented in `.env`. Key variables:

- `DATABASE_URL`: PostgreSQL connection string
- `ADDRESS`: Server bind address
- `JWT_SECRET`: JWT signing secret
- `EMAIL_PROVIDER`: "external" or "log"
- `ALLOWED_DOMAINS`: Colon-separated list of allowed university email domains
- Matching system parameters

## Testing Guidelines

- All integration tests use `#[sqlx::test]` macro for automatic test database setup
- Test utilities in `tests/common/mod.rs` provide `spawn_app()` and `MockEmailer`
- Tests spawn server at random port, use reqwest client for API testing
- Valid test emails must use domains from `ALLOWED_DOMAINS` (e.g., `mails.tsinghua.edu.cn`)
- Tests use special env and json files under `tests/data/`

## Code Style

- Import order: std library, external crates, local modules
- Use `tracing` for telemetry (not println or log)
- Constants in `utils/constant.rs`, major config in `.env`
- Custom errors use `thiserror`
- Database queries use sqlx macros for compile-time checking
- Global objects use `AppState` or `std::sync::LazyLock`
- It's cheap to clone `PgPool` since it internally holds an Arc, but do not clone heavy objects

## Database Schema

- `users`: User accounts with verification status flow (`unverified` → `verification_pending` → `verified`)
- `forms`: User questionnaire responses with tag selections
- `refresh_tokens`: JWT refresh token storage
- `match_previews`: Pre-computed match suggestions
- `final_matches`: Admin-triggered final matching results

## File Structure

- `src/handlers/`: HTTP endpoint handlers grouped by functionality
- `src/services/`: Business logic (email, JWT, matching algorithm)
- `src/models/`: Data structures and database models
- `src/middleware.rs`: Authentication middleware
- `src/utils/`: Constants, validators, upload utilities
- `migrations/`: SQLx database migrations
- `tests/`: Integration tests with common test utilities
