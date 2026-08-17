# Postg Parquet Import/Export Design

## Context
The `postg` CLI tool provides an embedded PostgreSQL management system, including capabilities for starting/stopping the database, Spock replication, and dumping/restoring data. 
Currently, the `postg dump` and `postg restore` commands wrap standard PostgreSQL utilities (like `pg_dump` and `pg_restore`). 
We are looking to expand this tooling to support ultra-fast, zero-copy data migration out of and into Postgres using Parquet files, powered by the new `sqlx_postgres` asynchronous binary `COPY` protocol developed in `connector_arrow`.

## Goals
- Add Parquet format support to `postg dump` to seamlessly export a database query's results into a highly compressed Parquet file.
- Add Parquet format support to `postg restore` to seamlessly bulk-load a Parquet file back into a PostgreSQL table.
- Provide high-throughput execution leveraging `tokio`, `parquet`, and `connector_arrow` streams without blowing up system memory.
- Offer developers working with distributed Spock nodes an easy way to pull down specific data subsets as standard analytical Parquet files.

## CLI Architecture Changes

### `postg dump`
We will introduce new CLI arguments to the `Dump` command:
- `--format [sql|parquet]` (Defaults to `sql` to preserve backwards compatibility).
- `--query <SQL>`: Required when `--format parquet` is used. Specifies the exact subset of data to extract.

**Execution Flow:**
1. If `--format parquet` is selected, validate that `--query` is provided.
2. Initialize an asynchronous database connection to the embedded engine using `connector_arrow::sqlx_postgres::SqlxPostgresConnection`.
3. Execute `AsyncConnector::query(query)` to begin reading.
4. Open the target local file and initialize an `AsyncArrowWriter` (from the `parquet` crate).
5. Continuously `.next_batch().await` from Postgres, streaming it directly to the Parquet file until completion.

### `postg restore`
We will introduce new CLI arguments to the `Restore` command:
- `--format [sql|parquet]` (Defaults to `sql`).
- `--table <NAME>`: Required when `--format parquet` is used. Specifies the destination table.
- `--create-table`: Optional boolean flag. If provided, `postg` will read the Arrow schema from the Parquet file and issue a `CREATE TABLE` statement using `connector_arrow::api::SchemaEdit`.

**Execution Flow:**
1. If `--format parquet` is selected, validate that `--table` is provided.
2. Initialize a `ParquetRecordBatchStreamBuilder` reading from the source local file.
3. If `--create-table` is flagged, extract the schema and create the target table in Postgres.
4. Initialize `AsyncConnector::append(table)`.
5. Stream batches iteratively into `appender.append(batch)`.
6. Finalize with `appender.finish()`.

## Dependencies
This enhancement will require modifying `postg-rs/Cargo.toml` to depend on:
- `connector_arrow` (with `src_sqlx_postgres` feature enabled).
- `parquet` (with `async` and `arrow` features enabled).
- `tokio` (existing, but necessary for driving the streams).

## Limitations & Considerations
- **Metadata Loss**: Unlike a standard `pg_dump`, dumping to Parquet only preserves raw columnar data. Complex SQL constraints, foreign keys, triggers, and Spock specific replication triggers will not be backed up or restored when using the `--format parquet` pathway. The primary use case is purely high-throughput raw data bulk loading/extraction.
