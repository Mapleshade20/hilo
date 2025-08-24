# Agent Guidelines for Hilo Project

## Build/Test Commands

- **Build**: `cargo build`
- **Test all**: `cargo test`
- **Test single**: `cargo test --test test_name`
- **Migrations**: `sqlx migrate run` (requires `DATABASE_URL` env var)

## Code Style

- Import standard library first, then external crates, then local modules
- Use `tracing` crate's macros `info!`, `error!`, `debug!`, `warning` for logging instead of println
- Prefer `Arc<T>` for shared state's child fields, `DashMap` for concurrent hashmaps
- Use `async-trait` for async traits
- Constants go in `utils/constant.rs` with UPPER_SNAKE_CASE
- Use `thiserror` for custom error types, `secrecy` for sensitive data
- Postgres database queries use sqlx macros for compile-time checking

## Notes for integration tests

- Take a look at existing tests before writing new tests
- Test utilities in `tests/common.rs`, use the already existing `MockEmailer` for email testing
- Tests use `#[sqlx::test]` macro, because it automatically creates temporary test databases and feeds `PgPool` to the first parameter of the test function
- Tests start with `let (address, mock_emailer) = spawn_app(pool).await;`, which spawns a server at a random available port. Then, set up a `reqwest` client for interacting with server, rather than using `tower`
- Valid example emails should have domain `mails.tsinghua.edu.cn`, which is defined in `.env`
- When debugging a timeout problem, write timeout command with cargo test command
