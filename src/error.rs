use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("extraction failed: {0}")]
    Extract(String),

    #[error("initdb failed: {0}")]
    InitDb(String),

    #[error("postgres start failed: {0}")]
    Start(String),

    #[error("postgres stop failed: {0}")]
    Stop(String),

    #[error("query error: {0}")]
    Query(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("sqlx error: {0}")]
    Sqlx(#[from] sqlx::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
