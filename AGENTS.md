# Agent Guidelines for Hilo Project

## Build/Test Commands

- **Build**: `cargo build` (only in debug profile, do not build release)
- **Test all**: `cargo test`
- **Test single**: `cargo test --test test_name`
- **Migrations**: `sqlx migrate run`/`sqlx migrate revert` (requires `DATABASE_URL` env var set)
- **SQLx offline prepare**: `cargo sqlx prepare -- --all-targets`
- **Start postgres**: `podman machine start` (only needed on MacOS) -> `podman-compose up -d db`
- **Reset postgres**: `podman-compose down -v`

## Code Style

- Import standard library first, then external crates, then local modules
- Use `tracing`, `tracing-subscriber` and `tracing-bunyan-formatter` for telemetry instead of println, log, or tracing-log
- Constants go in `utils/constant.rs`, use `constant::*` to import; major configurable values go in `.env`
- Use `thiserror` for custom error types
- Postgres database queries use sqlx macros for compile-time checking
- Make sure to follow good and idiomatic Rust coding style of the community

## Notes for integration tests

- Always take a look at existing tests, especially `common`, before writing new tests
- Test utilities in `tests/common/mod.rs`, use the existing `MockEmailer` for email testing
- Tests use `#[sqlx::test]` macro, which automatically creates temporary test databases and feeds `PgPool` to the first parameter of the test function
- Tests start with `let (address, mock_emailer) = spawn_app(pool).await;`, which spawns a server at a random available port. Then, set up a `reqwest` client to interact with server
- Valid example emails should have domain `mails.tsinghua.edu.cn`, which is defined in `.env`
- When debugging a timeout problem, write timeout command along with cargo test
