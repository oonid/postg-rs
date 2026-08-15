# Development Guide

This document covers the architectural details, internal engine workings, and how to build `postg-rs` binaries.

## 🏗️ Architecture

`postg-rs` utilizes a **managed child-process architecture**. Unlike SQLite which runs in-process, PostgreSQL's heavy reliance on `fork()` for background workers (autovacuum, WAL writers, logical replication) makes compiling to WASM or static linking impossible without crippling the database. 

Instead, `postg-rs` orchestrates a portable Postgres binary entirely in the background. It dynamically binds to ephemeral ports, writes isolated `postgresql.conf` and `pg_hba.conf` files, manages the initialization (`initdb`), and gracefully shuts down the engine via `pg_ctl` when your Rust application drops the handle.

## 📦 Binary Availability & Official Sources

We do not believe in forcing you to compile PostgreSQL yourself, nor do we trust unverified third-party binaries. 

Instead, our release pipeline automatically extracts binaries directly from the **official** upstream sources. 
- The standard engines are extracted directly from the official PGDG `postgres:17` and `postgres:18` Docker images.
- The Spock engines are extracted directly from the official `ghcr.io/pgedge/pgedge-postgres` Docker images.

This guarantees that the underlying engine behaves exactly as it would in a standard server environment, with 100% feature compatibility.

### 🗜️ Size Optimizations

An official PostgreSQL Docker image is typically hundreds of megabytes. To make `postg-rs` viable as an embedded solution, we run a custom `extract-pg-from-docker.sh` pipeline that:
1. Strips out unused documentation, headers, and debug symbols.
2. Extracts exactly the required shared object libraries (`.so`) using `ldd` and bundles them in an isolated `lib` directory.
3. Rewrites the dynamic linking paths using `patchelf` (`$ORIGIN/../lib`) so the binaries are entirely portable across any Linux distribution.
4. **Without-LLVM**: For users needing extremely small payloads, we offer a `without-llvm` engine variant. By stripping the LLVM JIT compilation libraries (which are rarely needed for embedded analytical queries), the final compressed `.tar.gz` is reduced to roughly **~60MB**.

## ⚙️ The Engines

You can choose between the following engines under the hood:

1. **Standard (`Engine::Postgresql`)**: Uses official, highly optimized PGDG standard Postgres binaries. Best for single-node apps or standard active-passive replication.
2. **Without LLVM (`Engine::PostgresqlWithoutLlvm`)**: The standard engine, but heavily stripped of LLVM JIT tooling for minimal binary size.
3. **PgVector (`Engine::PostgresqlPgvector`)**: Built from `pgvector/pgvector`, including the highly popular pgvector extension for AI and vector similarity search (fully verified in integration tests).
4. **Spock (`Engine::PostgresqlSpock`)**: Uses pgEdge's custom-patched Postgres binaries bundled with the Spock extension. Enables native active-active multi-master logical replication out-of-the-box (also verified via integration tests to natively include and support `pgvector`).

### ⚠️ Crucial Spock Limitations
If you are building a distributed active-active application using the Spock engine, please note:
* **Primary Keys:** Tables *must* have a `PRIMARY KEY` (or `REPLICA IDENTITY`) for `UPDATE` and `DELETE` replication to work.
* **DDL Changes:** Schema changes (like `ALTER TABLE`) require careful coordination using `spock.replicate_ddl` or pausing writes across all nodes.
* **Excluded Tables:** `UNLOGGED` and `TEMPORARY` tables are explicitly excluded from sync.

## 🛠️ CLI Usage with Different Engines

When using the `postg` CLI, you can easily switch between these engines using the `--engine` flag. This will automatically download and cache the correct binary for your system.

```bash
# Start with standard Postgres
postg --engine postgresql start

# Start with Spock (for active-active replication)
postg --engine postgresql-spock start

# Check the sync status of your Spock node
postg --engine postgresql-spock sync status

# Drop into a psql shell
postg --engine postgresql shell
```

## 🛠️ Building & Releasing Manually

We provide an automated GitHub Actions pipeline that builds and publishes the `.tar.gz` payloads on tag creation.

To build the binaries yourself locally (requires Docker & `patchelf`):
```bash
# Builds standard and without-llvm binaries for PG 18 and 17
PG_MAJORS="18 17" ./scripts/extract-pg-from-docker.sh

# Builds Spock binaries for PG 18 and 17
ENGINE=spock PG_MAJORS="18 17" ./scripts/extract-pg-from-docker.sh
```
The resulting portable binaries will be placed in the `dist/` folder.
