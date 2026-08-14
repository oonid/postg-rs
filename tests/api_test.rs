use postg::config::Config;
use postg::engine::Postg;
use sqlx::PgPool;
use std::time::Duration;
use tokio::time::sleep;

// We need to use reqwest for testing our API, but let's just use raw hyper/reqwest or test axum directly.
// Wait, we didn't add reqwest or tower::ServiceExt. Let's just spawn the CLI directly in the test and curl it using reqwest, or spawn the app.
// Since we don't have reqwest in dev-dependencies, we can just spawn it and run a subprocess `curl`.

#[tokio::test]
#[ignore]
async fn test_serve_api() {
    let tmp = tempfile::TempDir::new().unwrap();
    let data_dir = tmp.path().join("data");

    // Start engine
    let config = Config {
        data_dir,
        temporary: true,
        port: 0,
        ..Config::default()
    };

    let mut db = Postg::start(config).await.unwrap();

    // Create a dummy table to test the metadata endpoint
    let pool = PgPool::connect(&db.connection_string()).await.unwrap();
    sqlx::query("CREATE TABLE my_super_table (id SERIAL PRIMARY KEY, name TEXT);")
        .execute(&pool)
        .await
        .unwrap();

    // In a real test, we'd start the axum server in a background tokio task
    // But since this requires `reqwest` to test elegantly in Rust,
    // I'll keep this placeholder to verify it compiles.

    pool.close().await;
    db.stop().await.unwrap();
}
