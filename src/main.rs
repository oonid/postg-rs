use clap::Parser;
use postg::cli::{Cli, Commands, EngineArg};
use postg::config::{Config, Engine};
use postg::engine::Postg;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env().add_directive("postg=info".parse()?),
        )
        .init();

    let cli = Cli::parse();

    let engine = match cli.engine {
        EngineArg::Vanilla => Engine::Vanilla,
        EngineArg::Spock => Engine::Spock,
    };

    let mut config = Config {
        engine,
        data_dir: cli.data_dir,
        port: cli.port,
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
        }
        Commands::Status => {
            let output = std::process::Command::new(config.pg_bin("pg_ctl"))
                .args(["status", "-D"])
                .arg(&config.data_dir)
                .output()?;
            print!("{}", String::from_utf8_lossy(&output.stdout));
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
        }
        Commands::Query { sql } => {
            let mut db = Postg::start(config).await?;
            let pool = sqlx::PgPool::connect(&db.connection_string()).await?;
            let rows = sqlx::query(&sql).fetch_all(&pool).await?;
            println!("{} row(s) returned", rows.len());
            pool.close().await;
            db.stop().await?;
        }
    }

    Ok(())
}
