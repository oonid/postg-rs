use postg::config::Config;
use postg::engine::EmbeddedPg;

/// This test requires PostgreSQL binaries to be available in the cache dir.
/// Run with: cargo test -- --ignored
/// First, download and extract PG binaries to ~/.cache/postg/vanilla/
#[tokio::test]
#[ignore]
async fn start_and_stop_lifecycle() {
    let tmp = tempfile::TempDir::new().unwrap();
    let config = Config {
        data_dir: tmp.path().join("data"),
        temporary: true,
        ..Config::default()
    };

    let mut db = EmbeddedPg::start(config).await.unwrap();
    assert!(db.port() > 0);

    // Verify we can connect
    let pool = sqlx::PgPool::connect(&db.connection_string())
        .await
        .unwrap();
    let row: (i32,) = sqlx::query_as("SELECT 1").fetch_one(&pool).await.unwrap();
    assert_eq!(row.0, 1);
    pool.close().await;

    db.stop().await.unwrap();
}

#[tokio::test]
#[ignore]
async fn data_persists_across_restarts() {
    let tmp = tempfile::TempDir::new().unwrap();
    let data_dir = tmp.path().join("persistent_data");

    // First run: create table and insert data
    {
        let config = Config {
            data_dir: data_dir.clone(),
            temporary: false,
            ..Config::default()
        };
        let mut db = EmbeddedPg::start(config).await.unwrap();
        let pool = sqlx::PgPool::connect(&db.connection_string())
            .await
            .unwrap();
        sqlx::query("CREATE TABLE test_persist (id serial PRIMARY KEY, name text)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO test_persist (name) VALUES ('hello')")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
        db.stop().await.unwrap();
    }

    // Second run: verify data is still there
    {
        let config = Config {
            data_dir: data_dir.clone(),
            temporary: false,
            ..Config::default()
        };
        let mut db = EmbeddedPg::start(config).await.unwrap();
        let pool = sqlx::PgPool::connect(&db.connection_string())
            .await
            .unwrap();
        let row: (String,) = sqlx::query_as("SELECT name FROM test_persist WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.0, "hello");
        pool.close().await;
        db.stop().await.unwrap();
    }

    // Cleanup
    let _ = std::fs::remove_dir_all(&data_dir);
}
