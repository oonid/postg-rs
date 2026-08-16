# Design Spec: postg-rs Testing Ergonomics (`#[postg::test]`)

## 1. Overview
The goal is to provide a seamless, magical developer experience for testing against `postg-rs`. We will implement a `#[postg::test]` attribute macro that functions similarly to `#[tokio::test]`, but automatically spins up an ephemeral PostgreSQL instance in the background, cleans it up upon test completion, and injects the connection parameters directly into the test function.

## 2. Architecture: One Engine Per Test
To ensure perfect isolation and zero "zombie" child processes, we will use a "One Engine Per Test" approach. 
Each `#[postg::test]` invocation will rewrite the underlying async function to instantiate a fresh `postg::engine::Postg` with `temporary = true`. At the end of the test execution, the `Postg` variable will naturally drop out of scope, triggering its `Drop` implementation to run `pg_ctl stop` and automatically delete the temporary data directory.

## 3. The `postg-macros` Crate & Project Structure
Due to Rust's macro constraints, we will scaffold a new proc-macro crate within the workspace (or as a subdirectory).
- **Directory**: `postg-macros/`
- **Type**: `proc-macro = true` in `Cargo.toml`
- **Dependencies**: `syn` (with `full` feature), `quote`, `proc-macro2`.
- **Integration**: 
  - The main `postg` crate will add `postg-macros = { version = "...", path = "postg-macros", optional = true }`.
  - A new `macros` feature will be added to `postg`'s `Cargo.toml` (enabled by default).
  - The macro will be re-exported in `postg::src::lib.rs` as `pub use postg_macros::test;` when the feature is enabled.

## 4. Macro AST Transformation (Before / After)

The core job of the macro is to rewrite the AST. It consumes a test with arguments and emits a zero-argument `#[tokio::test]` compatible function.

**Before (What the user writes):**
```rust
#[postg::test]
async fn my_test(url: String) {
    println!("Connecting to {}", url);
}
```

**After (What the macro generates):**
```rust
#[tokio::test]
async fn my_test() {
    // 1. Setup isolated ephemeral instance
    let mut _postg_config = postg::config::Config::default();
    _postg_config.temporary = true;
    let _postg_db = postg::engine::Postg::start(_postg_config)
        .await
        .expect("Failed to start embedded postg-rs instance");
        
    // 2. Synthesize requested parameters
    let url: String = _postg_db.connection_string();
    
    // 3. User's original code wrapped in a block
    {
        println!("Connecting to {}", url);
    }
}
```

## 5. Parameter Validation & Type Injection
The macro must strictly validate the input parameters using `syn`.
- **Constraint**: The function must have exactly `0` or `1` parameter. More than one parameter throws a clear compile error (`syn::Error`).
- **Supported Parameter Types**:
  1. `url: String` -> Injects `let <ident> = _postg_db.connection_string();`
  2. `db: postg::engine::Postg` -> Injects `let <ident> = _postg_db;`
- If an unsupported type (like `sqlx::PgPool`) is requested, the macro will emit a compile error suggesting they request `String` and build the pool themselves.
- If `0` parameters are requested, the macro simply runs the setup and user block (keeping the `_postg_db` handle alive in the outer scope until the end).

## 6. Future-Proofing: Configuration Attributes
To make the macro robust, we should design it to parse optional attributes. 
For example, allowing the user to select the engine:
```rust
#[postg::test(engine = "postgresql-spock")]
async fn test_spock(url: String) { ... }
```
*If present*, the macro will parse the `name = "value"` pairs and inject the corresponding overrides into `_postg_config` before starting the engine. For V1, we can implement basic parsing and ignore/error on unknown fields, or just support `engine`.

## 7. Scope & Limitations
- This macro is intended for integration tests where the overhead of spinning up a real Postgres instance per test (usually ~1-2 seconds) is acceptable.
- It hardcodes the emission of `#[tokio::test]`, meaning it assumes the user is using the Tokio runtime and has it in their `dev-dependencies`.
