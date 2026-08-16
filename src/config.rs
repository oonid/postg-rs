use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum Engine {
    Postgresql,
    PostgresqlWithoutLlvm,
    PostgresqlSpock,
    PostgresqlPgvector,
}

impl std::fmt::Display for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Engine::Postgresql => write!(f, "postgresql"),
            Engine::PostgresqlWithoutLlvm => write!(f, "postgresql-without-llvm"),
            Engine::PostgresqlSpock => write!(f, "postgresql-spock"),
            Engine::PostgresqlPgvector => write!(f, "postgresql-pgvector"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub engine: Engine,
    pub data_dir: PathBuf,
    pub port: u16,
    pub host: String,
    pub username: String,
    pub database: String,
    pub temporary: bool,
    pub cache_dir: PathBuf,
}

use std::sync::atomic::{AtomicUsize, Ordering};

impl Default for Config {
    fn default() -> Self {
        static INSTANCE_ID: AtomicUsize = AtomicUsize::new(0);
        let home = std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir());
        Self {
            engine: Engine::Postgresql,
            data_dir: std::env::temp_dir().join(format!("postg-{}-{}", std::process::id(), INSTANCE_ID.fetch_add(1, Ordering::Relaxed))),
            port: 0,
            host: "127.0.0.1".to_string(),
            username: "postgres".to_string(),
            database: "postgres".to_string(),
            temporary: true,
            cache_dir: home.join(".cache").join("postg"),
        }
    }
}

impl Config {
    pub fn connection_string(&self) -> String {
        format!(
            "postgresql://{}@{}:{}/{}",
            self.username, self.host, self.port, self.database
        )
    }

    pub fn install_dir(&self) -> PathBuf {
        self.cache_dir.join(self.engine.to_string())
    }

    pub fn pg_bin(&self, name: &str) -> PathBuf {
        self.install_dir().join("bin").join(name)
    }
}
