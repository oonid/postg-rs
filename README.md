# postg

A Dual-Engine Embedded PostgreSQL Framework in Rust.

**Objective**: Provide a seamless "SQLite-like" developer experience by embedding PostgreSQL as a managed child process. It abstracts away binary acquisition, initialization, and lifecycle management while supporting both single-node and multi-master clustering.

## Engines

1. **Vanilla Engine**: Uses standard PostgreSQL binaries. Ideal for single-node deployments or standard logical replication (active/passive).
2. **Spock Engine**: Uses pgEdge-patched binaries with the Spock extension to enable **Active-Active Multi-Master Replication**. Writes on any node replicate everywhere with automatic conflict resolution (last-write-wins).

## Architecture

We use a **child-process architecture** instead of in-process (WASM/static linking). This is required because PostgreSQL's background workers (autovacuum, WAL writers) and replication workers require `fork()`, which is impossible in a single-process embedding.

An embedded REST API (Axum + SQLx) is planned to expose database access over HTTP, with future goals for auto-generated endpoints based on schema (similar to PostgREST).

## Crucial Spock Limitations

If using the Spock Engine for multi-master replication, keep these constraints in mind:
* **Patched Binaries Required**: Spock is not a drop-in extension. It requires specific C patches to PostgreSQL (e.g., logical commit clocks). We bundle pgEdge's official binaries for this reason.
* **Primary Keys**: Tables *must* have a `PRIMARY KEY` (or `REPLICA IDENTITY`) for `UPDATE`/`DELETE` replication.
* **Schema Changes (DDL)**: You must stop writes on all nodes before altering tables, or explicitly use `spock.replicate_ddl`.
* **Exclusions**: `UNLOGGED` and `TEMPORARY` tables are not replicated.
