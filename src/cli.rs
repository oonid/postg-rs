use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "postg", about = "Embedded PostgreSQL manager")]
pub struct Cli {
    /// Engine variant
    #[arg(long, default_value = "vanilla")]
    pub engine: EngineArg,

    /// Data directory
    #[arg(long, default_value = "./pgdata")]
    pub data_dir: PathBuf,

    /// TCP port (0 = ephemeral)
    #[arg(long, default_value = "0")]
    pub port: u16,

    /// Cache directory for PostgreSQL binaries
    #[arg(long)]
    pub cache_dir: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Clone, ValueEnum)]
pub enum EngineArg {
    Vanilla,
    Spock,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start the embedded PostgreSQL instance
    Start,
    /// Stop the embedded PostgreSQL instance
    Stop,
    /// Get the status of the embedded PostgreSQL instance
    Status,
    /// Run a SQL query
    Query {
        /// SQL to execute
        sql: String,
    },
}
