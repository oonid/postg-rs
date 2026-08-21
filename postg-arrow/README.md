# postg-arrow

Arrow and Parquet integration for the [`postg`](https://crates.io/crates/postg) embedded PostgreSQL framework.

This crate provides native parsing and serialization for PostgreSQL's binary `COPY` protocol, converting directly to and from Apache Arrow memory formats without overhead. It powers the `--format parquet` dump and restore features in the main `postg` CLI.

## Usage

This crate is an internal dependency of `postg`. To use its features, install the `postg` CLI or depend on the `postg` crate with the `parquet` feature enabled.
