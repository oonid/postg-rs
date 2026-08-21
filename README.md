<div align="center">
  
# postg-rs

Embedded PostgreSQL for Rust.

[![Rust](https://img.shields.io/badge/Rust-1.75%2B-orange.svg)](#)
[![PostgreSQL](https://img.shields.io/badge/PostgreSQL-17%20%7C%2018-blue.svg)](#)
[![License](https://img.shields.io/badge/License-MIT-green.svg)](#)

</div>

`postg-rs` gives you a SQLite-like developer experience with full PostgreSQL. No containers, no system installs. On first run, it downloads a portable PostgreSQL binary and manages it as a child process.

Supports Spock multi-master replication, pgvector, and Parquet import/export.

---

## Features

- **Zero-Install PostgreSQL** — auto-downloads portable binaries (Linux x86/ARM, macOS) from GitHub Releases
- **Spock Multi-Master** — active-active replication with last-write-wins conflict resolution via [pgEdge Spock](https://github.com/pgEdge/spock)
- **pgvector** — swap to the `postgresql-pgvector` engine for vector similarity search
- **Parquet Import/Export** — stream data between PostgreSQL and Parquet via zero-copy Arrow and the binary `COPY` protocol
- **`#[postg::test]` Macro** — integration tests with real PostgreSQL, no Docker
- **Built-in REST API** — serve SQL queries over HTTP
- **CLI** — `shell`, `query`, `dump`/`restore`, `sync`

---

## Quick Start

```toml
[dependencies]
postg = "0.1.7"
```

### Library Usage

```rust
use postg::config::{Config, Engine};
use postg::engine::Postg;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::default();
    let mut db = Postg::start(config).await?;

    println!("Connection string: {}", db.connection_string());
    // Use sqlx, diesel, tokio-postgres, etc.

    db.stop().await?;
    Ok(())
}
```

### Testing

The `#[postg::test]` macro spins up an ephemeral PostgreSQL instance per test:

```rust
#[postg::test]
async fn my_test(db: postg::engine::Postg) {
    let pool = sqlx::PgPool::connect(&db.connection_string()).await.unwrap();
    sqlx::query("CREATE TABLE t (id SERIAL PRIMARY KEY)").execute(&pool).await.unwrap();
    // DB is torn down automatically when the test ends.
}
```

Engine selection:
```rust
#[postg::test(engine = "postgresql-spock")]
async fn spock_test(db: postg::engine::Postg) { /* ... */ }
```

---

## CLI

```bash
# Install
cargo install postg
```

### Basic

```bash
postg shell                        # psql shell
postg query "SELECT version();"    # single query
postg serve --port 8080            # REST API
```

### Backup & Restore

```bash
# SQL
postg dump > backup.sql
postg restore backup.sql

# Parquet
postg dump --format parquet --query "SELECT * FROM users" --file users.parquet
postg restore --format parquet --table users --create-table users.parquet
```

### Spock Replication

```bash
# Node A
postg --engine postgresql-spock sync init \
  --node-name "node_a" \
  --dsn "host=10.0.0.1 port=5432 dbname=postgres"
postg --engine postgresql-spock sync publish --schema "public"

# Node B
postg --engine postgresql-spock sync init \
  --node-name "node_b" \
  --dsn "host=10.0.0.2 port=5432 dbname=postgres"
postg --engine postgresql-spock sync subscribe \
  --sub-name "sub_to_a" \
  --provider-dsn "postgresql://postgres@10.0.0.1:5432/postgres"

postg --engine postgresql-spock sync status
```

---

## Platform Support

| Platform | Architecture | Status |
|---|---|---|
| Linux | x86_64 | ✅ |
| Linux | aarch64 | ✅ |
| macOS | x86_64 / Apple Silicon | ✅ |
| Windows | — | ❌ |

---

## Development

See [DEVELOPMENT.md](DEVELOPMENT.md) for architecture, project structure, testing, and build instructions.

## License

[MIT License](LICENSE)
