<div align="center">
  
# 🐘 postg-rs

**The seamless, embedded PostgreSQL framework for Rust.**

[![Rust](https://img.shields.io/badge/Rust-1.75%2B-orange.svg)](#)
[![PostgreSQL](https://img.shields.io/badge/PostgreSQL-17%20%7C%2018-blue.svg)](#)
[![License](https://img.shields.io/badge/License-MIT-green.svg)](#)

</div>

`postg-rs` gives you the magical **"SQLite-like" developer experience** you've always wanted, but with the full, uncompromised power of **PostgreSQL**. No complex container orchestrations, no manual database installations, and no system-level dependencies. 

Ship a tiny ~10MB Rust binary to your users. On the first run, `postg-rs` seamlessly downloads a highly optimized, portable PostgreSQL engine in the background and runs it as a managed child process.

And for the first time ever: **True Embedded Multi-Master Replication**. 

---

## ✨ Key Features

- 🚀 **Zero-Install PostgreSQL:** Your users don't need to install Postgres. `postg-rs` auto-downloads deterministic, pre-compiled portable binaries (Linux x86/ARM, macOS) from GitHub Releases.
- 🧬 **Spock Multi-Master (Active-Active):** Spin up multiple nodes globally. Write to any node. Replicate everywhere. Automatic last-write-wins conflict resolution via [pgEdge Spock](https://github.com/pgEdge/spock).
- 🛠️ **Swiss-Army CLI:** Built-in commands to easily drop into a `shell`, perform `dump`/`restore`, or orchestrate `sync` status.
- 🌐 **Built-in REST API:** Serve a high-performance HTTP API directly from the engine to execute raw SQL or introspect your schema.
- ⚡ **Lightweight & Fast:** The standard engine payload is highly optimized for size, extracting only the necessary libraries to keep the bundle small.

---

## 🚀 Quick Start

Add `postg` to your `Cargo.toml`:

```toml
[dependencies]
postg = { git = "https://github.com/oonid/postg-rs.git" }
```

### Rust API Usage

```rust
use postg::config::{Config, Engine};
use postg::engine::Postg;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut config = Config::default();
    config.engine = Engine::Postgresql; // Or Engine::PostgresqlSpock
    config.temporary = false; // Set to true for ephemeral testing

    // Will auto-download the portable binary if missing, initdb, and start the engine!
    let mut db = Postg::start(config).await?;

    println!("Postgres is running on port {}", db.port());
    println!("Connection string: {}", db.connection_string());

    // Connect using your favorite SQL driver (e.g., SQLx, Diesel, tokio-postgres)
    // ... execute queries ...

    // Graceful shutdown
    db.stop().await?;
    Ok(())
}
```

---

## 🧰 The CLI Tool

`postg-rs` can be compiled as a standalone CLI to manage embedded databases directly from your terminal.

```bash
# Start an ephemeral database and drop into a psql shell
postg shell

# Start the built-in HTTP REST API Server
postg serve --port 8080

# Backup and Restore
postg dump > backup.sql
postg restore < backup.sql
```

### 🌍 Multi-Master Syncing (Spock)

Setting up global Active-Active replication between two isolated `postg-rs` nodes is now trivial:

```bash
# On Node A
postg sync init --node-name "node_a"
postg sync publish --schema "public"

# On Node B
postg sync init --node-name "node_b"
postg sync subscribe --sub-name "sub_to_a" --provider-dsn "postgresql://postgres@<NODE_A_IP>:<PORT>/postgres"

# Check the real-time sync status and lag!
postg sync status
```

*Status Output Example:*
```text
Sync Status: ✅ Fully Synced

Subscriptions (Pulling):
  - sub_to_a [applying]

Serving (Pushing):
  - node_a [streaming] (lag: 0 bytes)
```

---

## 📘 Development & Architecture

Curious about how `postg-rs` achieves this without WASM or static linking? Need to understand how we optimize the binaries to just ~60MB, or how we extract them directly from the **official upstream Docker images** for 100% compatibility?

👉 **[Read the Development Guide](DEVELOPMENT.md)**

---

## License

This project is licensed under the [MIT License](LICENSE).
