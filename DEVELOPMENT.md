# Development Guide

Architecture, project structure, testing, and build instructions for `postg-rs`.

## Architecture

`postg-rs` uses a managed child-process model. PostgreSQL relies on `fork()` for background workers (autovacuum, WAL, logical replication), so in-process embedding via WASM or static linking isn't viable without crippling the database.

Instead, `postg-rs` orchestrates a portable Postgres binary in the background: ephemeral port binding, isolated `postgresql.conf` and `pg_hba.conf`, `initdb` on first use, and graceful shutdown via `pg_ctl` on drop.

## Project Structure

```text
postg-rs/
├── src/
│   ├── lib.rs           # Public API: config, engine, payload, #[postg::test]
│   ├── config.rs        # Engine variants, Config, connection string
│   ├── engine.rs        # Postg lifecycle (start/stop/Drop)
│   ├── payload.rs       # Binary download, caching, extraction
│   ├── error.rs         # Error types
│   └── bin/postg/
│       ├── main.rs      # CLI entry point
│       ├── cli.rs       # Clap definitions
│       └── api.rs       # Axum REST handlers
├── postg-arrow/         # Native Arrow/Parquet integration via Postgres binary COPY protocol
├── postg-macros/        # Proc-macro crate for #[postg::test]
├── tests/               # Integration tests
├── scripts/             # PG binary extraction pipeline
└── dist/                # Built PG binaries (gitignored)
```

## Engines

1. **Standard** (`Engine::Postgresql`) — official PGDG binaries.
2. **Without LLVM** (`Engine::PostgresqlWithoutLlvm`) — stripped of LLVM JIT for smaller payload (~60MB compressed).
3. **PgVector** (`Engine::PostgresqlPgvector`) — includes the pgvector extension.
4. **Spock** (`Engine::PostgresqlSpock`) — pgEdge Spock for active-active multi-master replication. Also includes pgvector.

### Spock Limitations

- Tables must have a `PRIMARY KEY` (or `REPLICA IDENTITY`) for `UPDATE`/`DELETE` replication.
- DDL changes require coordination via `spock.replicate_ddl`.
- `UNLOGGED` and `TEMPORARY` tables are excluded from sync.

## Binary Sources

Binaries are extracted from official upstream Docker images:
- Standard/PgVector: PGDG `postgres:17` and `postgres:18`
- Spock: `ghcr.io/pgedge/pgedge-postgres`

The `extract-pg-from-docker.sh` script strips docs, headers, and debug symbols, bundles required `.so` files, and rewrites rpaths with `patchelf` for full portability.

## Parquet Import/Export

The `dump` and `restore` commands support Parquet using the `postg-arrow` crate. Instead of going through `pg_dump`/`psql`, the Parquet path connects directly via `sqlx` and streams Arrow record batches through the native PostgreSQL binary `COPY TO STDOUT` and `COPY FROM STDIN` protocols.

This allows zero-copy, highly efficient conversion of Postgres OID types directly into `apache-arrow` memory representations without string formatting overhead. 

Dependencies: `arrow` and `parquet` v59, `sqlx` 0.8.

## `#[postg::test]` Macro

The `postg-macros` crate provides a proc-macro that transforms async test functions:

1. Strips function parameters (`db: Postg` or `url: String`).
2. Injects `Postg::start()` with `temporary = true`.
3. Binds either the `Postg` instance or connection string based on parameter type.
4. Wraps in `#[tokio::test]`.
5. Cleanup via `Drop` — calls `pg_ctl stop` and removes the temp data directory.

## Running Tests

```bash
# Fast tests (no PG binary needed)
cargo test

# Full integration tests (needs cached PG binary)
./scripts/fetch-postgres.sh vanilla
cargo test -- --ignored

# Parquet round-trip only
cargo test --test parquet_cli_test

# Lint
cargo clippy --all-targets --all-features
```

Note: don't run `cargo test --all-features` in the `connector_arrow` workspace — it needs `libduckdb`.

## CLI with Different Engines

```bash
postg --engine postgresql start
postg --engine postgresql-spock start
postg --engine postgresql-spock sync status
postg --engine postgresql shell
```

## Building PG Binaries

Requires Docker and `patchelf`:

```bash
PG_MAJORS="18 17" ./scripts/extract-pg-from-docker.sh
ENGINE=spock PG_MAJORS="18 17" ./scripts/extract-pg-from-docker.sh
```

Output goes to `dist/`.
