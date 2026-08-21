# postg-macros

Procedural macros for the [`postg`](https://crates.io/crates/postg) embedded PostgreSQL framework.

Provides the `#[postg::test]` macro for automatically injecting ephemeral, isolated PostgreSQL instances into Rust async tests.

## Usage

This crate is not meant to be used directly. Please use the `postg` crate with the `macros` feature enabled.

```rust
#[postg::test]
async fn my_test(db: postg::engine::Postg) {
    // db is an ephemeral PostgreSQL instance that is automatically cleaned up
}
```
