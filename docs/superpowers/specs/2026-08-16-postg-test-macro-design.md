# Design Spec: postg-rs Testing Ergonomics (`#[postg::test]`)

## 1. Overview
The goal is to provide a seamless, magical developer experience for testing against `postg-rs`. We will implement a `#[postg::test]` attribute macro that functions similarly to `#[tokio::test]`, but automatically spins up an ephemeral PostgreSQL instance in the background, cleans it up upon test completion, and injects the connection parameters directly into the test function.

## 2. Architecture: One Engine Per Test
To ensure perfect isolation and zero "zombie" child processes, we will use a "One Engine Per Test" approach. 
Each `#[postg::test]` invocation will rewrite the underlying async function to instantiate a fresh `postg::engine::Postg` with `temporary = true`. At the end of the test execution, the `Postg` variable will naturally drop out of scope, triggering its `Drop` implementation to run `pg_ctl stop` and automatically delete the temporary data directory.

## 3. The `postg-macros` Crate
Due to Rust's macro constraints, we will scaffold a new proc-macro crate.
- **Name**: `postg-macros`
- **Type**: `proc-macro = true`
- **Dependencies**: `syn`, `quote`, `proc-macro2`
- **Integration**: The main `postg` crate will add `postg-macros` as an optional dependency (gated behind a new `macros` feature, enabled by default) and re-export the macro so users can simply `use postg::test;`.

## 4. Macro Behavior & Type Injection
The macro will parse the user's test function signature and inject the appropriate variables based on the requested parameter type. 
Supported parameter injections:
1. **`async fn my_test(url: String)`**
   - The macro injects `let url = _db.connection_string();` and passes it.
2. **`async fn my_test(db: postg::engine::Postg)`**
   - The macro passes the raw `_db` instance directly to the user (granting them access to `.port()` or other internal metrics).

*(Note: We deliberately do not support parameter-less tests that rely on injecting a `POSTGRES_URL` environment variable, because `std::env::set_var` is not thread-safe and would cause race conditions when Rust runs multiple tests in parallel.)*

## 5. Scope & Limitations
- This macro is intended for integration tests where the overhead of spinning up a real Postgres instance per test is acceptable.
- It relies on `tokio::test` under the hood. Therefore, users must have `tokio` in their `dev-dependencies`.
