use postg::config::{Config, Engine};
use postg::engine::Postg;

// This test requires the `postgresql-pgvector` binary to be available or downloadable.
// We use #[ignore] so standard `cargo test` doesn't block on network downloads in CI.
#[tokio::test]
#[ignore]
async fn test_pgvector_engine_loads_vector_extension() {
    let tmp_path = std::path::PathBuf::from("/tmp/pgvector_test_engine");
    let _ = std::fs::remove_dir_all(&tmp_path);
    std::fs::create_dir_all(&tmp_path).unwrap();

    let config = Config {
        engine: Engine::PostgresqlPgvector,
        data_dir: tmp_path,
        temporary: false,
        ..Config::default()
    };
    
    let mut node = Postg::start(config).await.expect("Failed to start PgVector node");
    let pool = sqlx::PgPool::connect(&node.connection_string()).await.unwrap();

    // The extension should create successfully
    sqlx::query("CREATE EXTENSION vector;")
        .execute(&pool)
        .await
        .expect("Failed to create vector extension in pgvector engine");

    // Test a basic vector insert and similarity search
    sqlx::query("CREATE TABLE items (id bigserial PRIMARY KEY, embedding vector(3));")
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("INSERT INTO items (embedding) VALUES ('[1,2,3]'), ('[4,5,6]');")
        .execute(&pool)
        .await
        .unwrap();

    let row: (i64,) = sqlx::query_as("SELECT id FROM items ORDER BY embedding <-> '[3,1,2]' LIMIT 1;")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(row.0, 1, "Expected first vector to be closest to [3,1,2]");

    pool.close().await;
    node.stop().await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_spock_engine_loads_vector_extension() {
    let tmp_path = std::path::PathBuf::from("/tmp/spock_pgvector_test_engine");
    let _ = std::fs::remove_dir_all(&tmp_path);
    std::fs::create_dir_all(&tmp_path).unwrap();

    let config = Config {
        engine: Engine::PostgresqlSpock,
        data_dir: tmp_path,
        temporary: false,
        ..Config::default()
    };
    
    let mut node = Postg::start(config).await.expect("Failed to start Spock node");
    let pool = sqlx::PgPool::connect(&node.connection_string()).await.unwrap();

    // PgEdge Spock natively includes pgvector, so this should pass!
    sqlx::query("CREATE EXTENSION vector;")
        .execute(&pool)
        .await
        .expect("Failed to create vector extension in spock engine");

    pool.close().await;
    node.stop().await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_standard_engine_fails_to_load_vector_extension() {
    let tmp_path = std::path::PathBuf::from("/tmp/standard_pgvector_test_engine");
    let _ = std::fs::remove_dir_all(&tmp_path);
    std::fs::create_dir_all(&tmp_path).unwrap();

    let config = Config {
        engine: Engine::Postgresql,
        data_dir: tmp_path,
        temporary: false,
        ..Config::default()
    };
    
    let mut node = Postg::start(config).await.expect("Failed to start Standard node");
    let pool = sqlx::PgPool::connect(&node.connection_string()).await.unwrap();

    // The standard engine does NOT include pgvector, so this MUST fail.
    let result = sqlx::query("CREATE EXTENSION vector;")
        .execute(&pool)
        .await;

    assert!(result.is_err(), "Standard engine should NOT have the vector extension available");

    pool.close().await;
    node.stop().await.unwrap();
}
