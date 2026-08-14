use postg::config::Config;
use postg::engine::Postg;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
#[ignore]
async fn test_vanilla_logical_replication() {
    let tmp = tempfile::TempDir::new().unwrap();
    let data_dir_a = tmp.path().join("node_a");
    let data_dir_b = tmp.path().join("node_b");

    // 1. Start Node A and Node B
    let config_a = Config {
        data_dir: data_dir_a,
        temporary: true,
        ..Config::default()
    };
    let mut node_a = Postg::start(config_a).await.unwrap();

    let config_b = Config {
        data_dir: data_dir_b,
        temporary: true,
        ..Config::default()
    };
    let mut node_b = Postg::start(config_b).await.unwrap();

    let pool_a = sqlx::PgPool::connect(&node_a.connection_string())
        .await
        .unwrap();
    let pool_b = sqlx::PgPool::connect(&node_b.connection_string())
        .await
        .unwrap();

    // 2. Setup schema on both nodes
    let schema_sql = "CREATE TABLE messages (id serial PRIMARY KEY, content text NOT NULL);";
    sqlx::query(schema_sql).execute(&pool_a).await.unwrap();
    sqlx::query(schema_sql).execute(&pool_b).await.unwrap();

    // 3. Setup Publication on Node A
    sqlx::query("CREATE PUBLICATION pub_messages FOR TABLE messages;")
        .execute(&pool_a)
        .await
        .unwrap();

    // 4. Setup Subscription on Node B pointing to Node A
    let sub_sql = format!(
        "CREATE SUBSCRIPTION sub_messages CONNECTION '{}' PUBLICATION pub_messages;",
        node_a.connection_string()
    );
    sqlx::query(&sub_sql).execute(&pool_b).await.unwrap();

    // Give replication a moment to initialize
    sleep(Duration::from_millis(500)).await;

    // 5. Insert on A -> verify it replicates to B
    sqlx::query("INSERT INTO messages (content) VALUES ('hello from A')")
        .execute(&pool_a)
        .await
        .unwrap();

    // Wait for replication
    let mut replicated = false;
    for _ in 0..20 {
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM messages WHERE content = 'hello from A'")
                .fetch_one(&pool_b)
                .await
                .unwrap();
        if count.0 == 1 {
            replicated = true;
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }
    assert!(replicated, "Data from Node A did not replicate to Node B");

    // 6. Insert on B -> verify it does NOT replicate to A (one-directional)
    // We use an explicit ID because the serial sequence on B didn't advance from replication
    sqlx::query("INSERT INTO messages (id, content) VALUES (100, 'hello from B')")
        .execute(&pool_b)
        .await
        .unwrap();

    // Wait a bit to ensure it doesn't replicate
    sleep(Duration::from_millis(500)).await;

    let count_a: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM messages")
        .fetch_one(&pool_a)
        .await
        .unwrap();
    // Node A should only have the first message
    assert_eq!(
        count_a.0, 1,
        "Node A received a row from Node B, but replication should be one-way"
    );

    pool_a.close().await;
    pool_b.close().await;

    node_a.stop().await.unwrap();
    node_b.stop().await.unwrap();
}
