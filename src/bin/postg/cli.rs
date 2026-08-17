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

    /// Host to bind to
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

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
    PostgresqlPgvector,
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
        /// Format to export (sql or parquet)
        #[arg(long, default_value = "sql")]
        format: String,

        /// SQL query to dump (required for parquet)
        #[arg(long)]
        query: Option<String>,

        /// File to dump to (stdout if not specified)
        #[arg(short, long)]
        file: Option<PathBuf>,
    },
    /// Restore the database from a file
    Restore {
        /// Format to restore from (sql or parquet)
        #[arg(long, default_value = "sql")]
        format: String,

        /// Destination table to restore to (required for parquet)
        #[arg(long)]
        table: Option<String>,

        /// Create the destination table from the Parquet schema before restoring
        #[arg(long)]
        create_table: bool,

        /// File to restore from
        file: PathBuf,
    },
    /// Manage Spock replication
    Sync {
        #[command(subcommand)]
        command: SyncCommand,
    },
}

#[derive(Subcommand)]
pub enum SyncCommand {
    /// Initialize the local database as a Spock node
    Init {
        /// Name of the local node
        #[arg(long)]
        node_name: String,
        /// DSN that remote nodes can use to reach this node (e.g. host=10.0.0.5 port=5432 user=postgres dbname=postgres)
        #[arg(long)]
        dsn: String,
    },
    /// Publish tables to the replication set
    Publish {
        /// Name of the schema to replicate
        #[arg(long, default_value = "public")]
        schema: String,
    },
    /// Subscribe to a remote provider node
    Subscribe {
        /// Name of the subscription
        #[arg(long)]
        sub_name: String,
        /// DSN of the remote provider node
        #[arg(long)]
        provider_dsn: String,
    },
    /// Check the status of Spock subscriptions and replication
    Status {
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },
}
