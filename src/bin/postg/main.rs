mod api;
mod cli;

use clap::Parser;
use cli::{Cli, Commands, EngineArg, SyncCommand};
use postg::config::{Config, Engine};
use postg::engine::Postg;
use connector_arrow::api_async::{AsyncConnector, AsyncResultReader, AsyncStatement};
use parquet::arrow::AsyncArrowWriter;
use sqlx::Connection;

#[derive(serde::Serialize)]
struct SyncStatus {
    subscriptions: Vec<SubscriptionStatus>,
    serving: Vec<ServingStatus>,
    is_fully_synced: bool,
}

#[derive(serde::Serialize)]
struct SubscriptionStatus {
    name: String,
    status: String,
}

#[derive(serde::Serialize)]
struct ServingStatus {
    client: String,
    status: String,
    lag_bytes: i64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env().add_directive("postg=info".parse()?),
        )
        .init();

    let cli = Cli::parse();

    let engine = match cli.engine {
        EngineArg::Postgresql => Engine::Postgresql,
        EngineArg::PostgresqlWithoutLlvm => Engine::PostgresqlWithoutLlvm,
        EngineArg::PostgresqlSpock => Engine::PostgresqlSpock,
        EngineArg::PostgresqlPgvector => Engine::PostgresqlPgvector,
    };

    let mut config = Config {
        engine,
        data_dir: cli.data_dir,
        port: cli.port,
        host: cli.host,
        temporary: false,
        ..Config::default()
    };

    if let Some(cache_dir) = cli.cache_dir {
        config.cache_dir = cache_dir;
    }

    match cli.command {
        Commands::Start => {
            let db = Postg::start(config).await?;
            println!("PostgreSQL started on port {}", db.port());
            println!("Connection: {}", db.connection_string());
            println!("Press Ctrl+C to stop...");
            tokio::signal::ctrl_c().await?;
            println!("Shutting down...");
            // Drop will handle cleanup
        }
        Commands::Stop => {
            let output = std::process::Command::new(config.pg_bin("pg_ctl"))
                .args(["stop", "-D"])
                .arg(&config.data_dir)
                .args(["-m", "fast"])
                .output()?;
            print!("{}", String::from_utf8_lossy(&output.stdout));
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
            if !output.status.success() {
                std::process::exit(output.status.code().unwrap_or(1));
            }
        }
        Commands::Status => {
            let output = std::process::Command::new(config.pg_bin("pg_ctl"))
                .args(["status", "-D"])
                .arg(&config.data_dir)
                .output()?;
            print!("{}", String::from_utf8_lossy(&output.stdout));
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
            if !output.status.success() {
                std::process::exit(output.status.code().unwrap_or(1));
            }
        }
        Commands::Query { query } => {
            let mut db = Postg::start(config).await?;
            let pool = sqlx::PgPool::connect(&db.connection_string()).await?;
            let rows = sqlx::query(&query).fetch_all(&pool).await?;
            println!("{} row(s) returned", rows.len());
            pool.close().await;
            db.stop().await?;
        }
        Commands::Serve { port } => {
            let db = Postg::start(config).await?;
            println!("PostgreSQL started on port {}", db.port());

            let pool = sqlx::PgPool::connect(&db.connection_string()).await?;
            let app = api::app(pool);

            let addr = format!("127.0.0.1:{}", port);
            println!("Serving HTTP API on http://{}", addr);
            let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();

            axum::serve(listener, app).await.unwrap();
        }
        Commands::Shell => {
            let mut db = Postg::start(config).await?;
            let mut child = std::process::Command::new(db.config().pg_bin("psql"))
                .arg("-d")
                .arg(db.connection_string())
                .spawn()?;
            child.wait()?;
            db.stop().await?;
        }
        Commands::Dump {
            format,
            query,
            file,
        } => {
            let mut db = Postg::start(config).await?;
            if format == "sql" {
                let mut cmd = std::process::Command::new(db.config().pg_bin("pg_dump"));
                cmd.arg("-d").arg(db.connection_string());
                if let Some(f) = file {
                    cmd.arg("-f").arg(f);
                }
                let mut child = cmd.spawn()?;
                child.wait()?;
            } else if format == "parquet" {
                let file_path = file.ok_or_else(|| anyhow::anyhow!("--file is required for parquet format"))?;
                let query_str = query.ok_or_else(|| anyhow::anyhow!("--query is required for parquet format"))?;

                let conn = sqlx::PgConnection::connect(&db.connection_string()).await?;
                let mut conn = connector_arrow::sqlx_postgres::SqlxPostgresConnection::new(conn);
                let mut stmt = conn.query(&query_str).await?;
                let mut reader = stmt.start(std::iter::empty::<&dyn connector_arrow::api::ArrowValue>()).await?;
                let schema = reader.get_schema()?;

                let file = tokio::fs::File::create(&file_path).await?;
                let mut writer = AsyncArrowWriter::try_new(file, schema, None)?;

                while let Some(batch) = reader.next_batch().await? {
                    writer.write(&batch).await?;
                }
                writer.close().await?;
            } else {
                anyhow::bail!("Unsupported format: {}. Supported formats are 'sql' and 'parquet'", format);
            }
            db.stop().await?;
        }

        Commands::Restore { file, .. } => {
            let mut db = Postg::start(config).await?;
            let mut child = std::process::Command::new(db.config().pg_bin("psql"))
                .arg("-d")
                .arg(db.connection_string())
                .arg("-f")
                .arg(file)
                .spawn()?;
            child.wait()?;
            db.stop().await?;
        }
        Commands::Sync { command } => {
            let mut db = Postg::start(config).await?;
            let pool = sqlx::PgPool::connect(&db.connection_string()).await?;

            match command {
                SyncCommand::Init { node_name, dsn } => {
                    println!("Initializing Spock node '{}'...", node_name);
                    sqlx::query("CREATE EXTENSION IF NOT EXISTS spock")
                        .execute(&pool)
                        .await?;
                    sqlx::query("SELECT spock.node_create(node_name := $1, dsn := $2)")
                        .bind(&node_name)
                        .bind(&dsn)
                        .execute(&pool)
                        .await?;
                    println!("Node '{}' created successfully.", node_name);
                }
                SyncCommand::Publish { schema } => {
                    println!(
                        "Publishing schema '{}' to default replication set...",
                        schema
                    );
                    // Usually we use the default repset, or create one.
                    // spock creates 'default' repset automatically when extension is created?
                    // Let's create 'default' repset just in case, ignoring error if exists.
                    let _ = sqlx::query("SELECT spock.repset_create('default')")
                        .execute(&pool)
                        .await;
                    sqlx::query("SELECT spock.repset_add_all_tables('default', ARRAY[$1])")
                        .bind(&schema)
                        .execute(&pool)
                        .await?;
                    println!("Schema '{}' published successfully.", schema);
                }
                SyncCommand::Subscribe {
                    sub_name,
                    provider_dsn,
                } => {
                    println!("Subscribing to provider...");
                    sqlx::query(
                        "SELECT spock.sub_create(subscription_name := $1, provider_dsn := $2)",
                    )
                    .bind(&sub_name)
                    .bind(&provider_dsn)
                    .execute(&pool)
                    .await?;
                    println!("Subscription '{}' created successfully.", sub_name);
                }
                SyncCommand::Status { json } => {
                    let mut is_fully_synced = true;

                    // 1. Subscriptions (Pulling)
                    let sub_rows: Vec<(String, Option<i32>)> =
                        sqlx::query_as("SELECT subname, pid FROM pg_stat_subscription")
                            .fetch_all(&pool)
                            .await?;

                    let mut subscriptions = Vec::new();
                    for row in sub_rows {
                        let status = if row.1.is_some() {
                            "connected".to_string()
                        } else {
                            is_fully_synced = false;
                            "disconnected".to_string()
                        };
                        subscriptions.push(SubscriptionStatus {
                            name: row.0,
                            status,
                        });
                    }

                    // 2. Serving (Pushing)
                    let rep_rows: Vec<(Option<String>, Option<String>, Option<i64>)> = sqlx::query_as(
                        "SELECT application_name, state, 
                         CAST(pg_wal_lsn_diff(pg_current_wal_lsn(), replay_lsn) AS BIGINT) AS lag_bytes 
                         FROM pg_stat_replication"
                    )
                    .fetch_all(&pool)
                    .await?;

                    let mut serving = Vec::new();
                    for row in rep_rows {
                        let lag = row.2.unwrap_or(0);
                        if lag > 0 {
                            is_fully_synced = false;
                        }
                        serving.push(ServingStatus {
                            client: row.0.unwrap_or_default(),
                            status: row.1.unwrap_or_default(),
                            lag_bytes: lag,
                        });
                    }

                    let status_report = SyncStatus {
                        subscriptions,
                        serving,
                        is_fully_synced,
                    };

                    if json {
                        println!("{}", serde_json::to_string_pretty(&status_report)?);
                    } else {
                        println!(
                            "Sync Status: {}",
                            if status_report.is_fully_synced {
                                "✅ Fully Synced"
                            } else {
                                "🔄 Syncing/Disconnected"
                            }
                        );
                        println!("\nSubscriptions (Pulling):");
                        if status_report.subscriptions.is_empty() {
                            println!("  (none)");
                        } else {
                            for sub in &status_report.subscriptions {
                                println!("  - {} [{}]", sub.name, sub.status);
                            }
                        }

                        println!("\nServing (Pushing):");
                        if status_report.serving.is_empty() {
                            println!("  (none)");
                        } else {
                            for srv in &status_report.serving {
                                println!(
                                    "  - {} [{}] (lag: {} bytes)",
                                    srv.client, srv.status, srv.lag_bytes
                                );
                            }
                        }
                    }
                }
            }

            pool.close().await;
            db.stop().await?;
        }
    }

    Ok(())
}
