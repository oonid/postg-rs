# Postg Parquet Import/Export Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add high-throughput Parquet export and import capabilities to the `postg dump` and `postg restore` commands using `connector_arrow`.

**Architecture:** We will extend the existing `postg` CLI arguments using `clap`. In `main.rs`, we will intercept `--format parquet`, bypassing `pg_dump`/`psql`, and instead use `tokio`, `parquet`, and `connector_arrow` to stream data back and forth to PostgreSQL via the binary `COPY` protocol.

**Tech Stack:** Rust, Clap, Tokio, sqlx, parquet, connector_arrow

**Spec:** `docs/superpowers/specs/2026-08-17-postg-parquet-design.md`

## Global Constraints
- `postg-rs` edition 2021, MSRV 1.75+
- No modifications to the `connector_arrow` codebase; treat it as an external dependency.
- Existing `sql` format dumps/restores MUST continue to work identically as before.

---

### Task 1: Update Dependencies and CLI Definitions

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/bin/postg/cli.rs`

**Interfaces:**
- Produces: Updated `Commands::Dump` and `Commands::Restore` enums with `format`, `query`, `table`, and `create_table` fields.

- [ ] **Step 1: Add dependencies to Cargo.toml**
      Add `connector_arrow = { path = "../connector_arrow/connector_arrow", features = ["src_sqlx_postgres"] }`.
      Add `parquet = { version = "58.0.0", features = ["async", "arrow"] }` (must match the Arrow version used by `connector_arrow` which is 58).
      Add `arrow = { version = "58.0.0" }`.

- [ ] **Step 2: Update `cli.rs` `Dump` arguments**
      Modify the `Dump` variant:
      - Add `#[arg(long, default_value = "sql")] format: String`
      - Add `#[arg(long)] query: Option<String>`
      - Change `file: Option<PathBuf>` to be optional, but required for parquet.

- [ ] **Step 3: Update `cli.rs` `Restore` arguments**
      Modify the `Restore` variant:
      - Add `#[arg(long, default_value = "sql")] format: String`
      - Add `#[arg(long)] table: Option<String>`
      - Add `#[arg(long)] create_table: bool`
      - Keep `file: PathBuf`.

---

### Task 2: Implement Parquet Export (`Dump`)

**Files:**
- Modify: `src/bin/postg/main.rs`

**Interfaces:**
- Consumes: `Commands::Dump` from Task 1.

- [ ] **Step 1: Add conditional logic in `main.rs` for `Dump`**
      In the `Commands::Dump` match arm, check if `format == "sql"`. If so, execute the existing `pg_dump` logic.
      If `format == "parquet"`, proceed to Step 2.

- [ ] **Step 2: Implement Parquet Export logic**
      Ensure `file` and `query` are provided (return error if not).
      Connect to the database via `sqlx::PgPool` or `PgConnection`.
      Wrap the connection in `connector_arrow::sqlx_postgres::SqlxPostgresConnection::new(conn)`.
      Call `conn.query(&query).await`.
      Start the result reader.
      Get the schema from the reader.
      Open the `file` using `tokio::fs::File`.
      Initialize `parquet::arrow::AsyncArrowWriter::try_new(file, schema, None)`.
      Loop over `reader.next_batch().await` and write each batch via `writer.write(&batch).await`.
      Call `writer.close().await`.
      Stop the `db`.

---

### Task 3: Implement Parquet Import (`Restore`)

**Files:**
- Modify: `src/bin/postg/main.rs`

**Interfaces:**
- Consumes: `Commands::Restore` from Task 1.

- [ ] **Step 1: Add conditional logic in `main.rs` for `Restore`**
      In the `Commands::Restore` match arm, check if `format == "sql"`. If so, execute the existing `psql` logic.
      If `format == "parquet"`, proceed to Step 2.

- [ ] **Step 2: Implement Parquet Import logic**
      Ensure `table` is provided (return error if not).
      Open the `file` using `tokio::fs::File`.
      Initialize `parquet::arrow::async_reader::ParquetRecordBatchStreamBuilder::new(file).await`.
      Extract the schema from the builder.
      Build the stream: `let mut stream = builder.build().unwrap();`.
      Connect to the database and wrap in `SqlxPostgresConnection`.
      If `create_table` is true, call `tokio::task::block_in_place(|| conn.table_create(&table, schema))`.
      Call `conn.append(&table).await`.
      Loop over `stream.next().await`, and for each batch call `appender.append(batch).await`.
      Call `appender.finish().await`.
      Stop the `db`.
