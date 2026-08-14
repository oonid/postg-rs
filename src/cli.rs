use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "postg", about = "Embedded PostgreSQL manager")]
pub struct Cli {
    /// Engine variant
    #[arg(long, default_value = "postgresql")]
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
    Postgresql,
    PostgresqlWithoutLlvm,
    PostgresqlSpock,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start the embedded PostgreSQL instance
    Start,
    /// Stop the embedded PostgreSQL instance
    Stop,
    /// Get the status of the embedded PostgreSQL instance
    Status,
    /// Run the PostgreSQL engine and execute a query
    Query { query: String },
    /// Start the PostgreSQL engine and the embedded HTTP API
    Serve {
        #[arg(long, default_value = "8080")]
        port: u16,
    },
    /// Start an interactive psql shell
    Shell,
    /// Dump the database to a file or stdout
    Dump {
        /// File to dump to (stdout if not specified)
        #[arg(short, long)]
        file: Option<PathBuf>,
    },
    /// Restore the database from a file
    Restore {
        /// File to restore from
        file: PathBuf,
    },
}
