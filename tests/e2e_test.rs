//! End-to-end tests. Require PostgreSQL binaries in the cache.
//! Run: ./scripts/fetch-postgres.sh vanilla && cargo test -- --ignored

use postg::config::Config;
use postg::engine::EmbeddedPg;

#[tokio::test]
#[ignore]
async fn full_lifecycle_create_insert_query_restart() {
    let tmp = tempfile::TempDir::new().unwrap();
    let data_dir = tmp.path().join("e2e_data");

    // Start, create schema, insert data
    let _port = {
        let config = Config {
            data_dir: data_dir.clone(),
            temporary: false,
            ..Config::default()
        };
        let mut db = EmbeddedPg::start(config).await.expect("start failed");
        let port = db.port();

        let pool = sqlx::PgPool::connect(&db.connection_string())
            .await
            .expect("connect failed");

        sqlx::query("CREATE TABLE users (id serial PRIMARY KEY, name text NOT NULL, email text)")
            .execute(&pool)
            .await
            .expect("create table failed");

        sqlx::query("INSERT INTO users (name, email) VALUES ($1, $2)")
            .bind("Alice")
            .bind("alice@example.com")
            .execute(&pool)
            .await
            .expect("insert failed");

        sqlx::query("INSERT INTO users (name, email) VALUES ($1, $2)")
            .bind("Bob")
            .bind("bob@example.com")
            .execute(&pool)
            .await
            .expect("insert failed");

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(&pool)
            .await
            .expect("count failed");
        assert_eq!(count.0, 2);

        pool.close().await;
        db.stop().await.expect("stop failed");
        port
    };

    // Restart on a new port, verify data survived
    {
        let config = Config {
            data_dir: data_dir.clone(),
            temporary: false,
            ..Config::default()
        };
        let mut db = EmbeddedPg::start(config).await.expect("restart failed");
        // Port might differ — that's fine, we use ephemeral

        let pool = sqlx::PgPool::connect(&db.connection_string())
            .await
            .expect("reconnect failed");

        let users: Vec<(i32, String, Option<String>)> =
            sqlx::query_as("SELECT id, name, email FROM users ORDER BY id")
                .fetch_all(&pool)
                .await
                .expect("query failed");

        assert_eq!(users.len(), 2);
        assert_eq!(users[0].1, "Alice");
        assert_eq!(users[1].1, "Bob");

        pool.close().await;
        db.stop().await.expect("stop failed");
    }

    let _ = std::fs::remove_dir_all(&data_dir);
}
