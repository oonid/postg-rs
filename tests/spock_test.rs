use postg::config::{Config, Engine};
use postg::engine::Postg;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
#[ignore]
async fn test_spock_multi_master_replication() {
    // Hardcoded directories for debugging
    let tmp_path = std::path::PathBuf::from("/tmp/spock_test_debug");
    let _ = std::fs::remove_dir_all(&tmp_path);
    std::fs::create_dir_all(&tmp_path).unwrap();
    let data_dir_a = tmp_path.join("node_a");
    let data_dir_b = tmp_path.join("node_b");

    // 1. Start Node A and Node B with Spock engine
    let config_a = Config {
        engine: Engine::PostgresqlSpock,
        data_dir: data_dir_a,
        temporary: false,
        ..Config::default()
    };
    let mut node_a = Postg::start(config_a)
        .await
        .expect("Failed to start Node A");

    let config_b = Config {
        engine: Engine::PostgresqlSpock,
        data_dir: data_dir_b,
        temporary: false,
        ..Config::default()
    };
    let mut node_b = Postg::start(config_b.clone())
        .await
        .expect("Failed to start Node B");

    let pool_a = sqlx::PgPool::connect(&node_a.connection_string())
        .await
        .unwrap();
    let pool_b = sqlx::PgPool::connect(&node_b.connection_string())
        .await
        .unwrap();

    // 2. Setup schema (Must have PRIMARY KEY for Spock)
    let schema_sql = "CREATE TABLE messages (id serial PRIMARY KEY, content text NOT NULL);";
    sqlx::query(schema_sql).execute(&pool_a).await.unwrap();
    sqlx::query(schema_sql).execute(&pool_b).await.unwrap();

    // 3. Initialize Spock Extension
    sqlx::query("CREATE EXTENSION IF NOT EXISTS spock;")
        .execute(&pool_a)
        .await
        .unwrap();
    sqlx::query("CREATE EXTENSION IF NOT EXISTS spock;")
        .execute(&pool_b)
        .await
        .unwrap();

    // 4. Create Spock Nodes
    let dsn_a = node_a.connection_string();
    let dsn_b = node_b.connection_string();

    sqlx::query("SELECT spock.node_create(node_name := 'node_a'::name, dsn := $1)")
        .bind(&dsn_a)
        .execute(&pool_a)
        .await
        .unwrap();

    sqlx::query("SELECT spock.node_create(node_name := 'node_b'::name, dsn := $1)")
        .bind(&dsn_b)
        .execute(&pool_b)
        .await
        .unwrap();

    // 5. Add table to default replication set on both nodes
    // spock.repset_add_table(set_name name, relation regclass, synchronize_data boolean)
    sqlx::query("SELECT spock.repset_add_table('default', 'messages', true);")
        .execute(&pool_a)
        .await
        .unwrap();

    sqlx::query("SELECT spock.repset_add_table('default', 'messages', true);")
        .execute(&pool_b)
        .await
        .unwrap();

    // 6. Create Subscriptions (Active-Active)
    // Node A subscribes to Node B
    sqlx::query(
        "SELECT spock.sub_create(subscription_name := 'sub_a_to_b'::name, provider_dsn := $1)",
    )
    .bind(&dsn_b)
    .execute(&pool_a)
    .await
    .unwrap();

    // Node B subscribes to Node A
    sqlx::query(
        "SELECT spock.sub_create(subscription_name := 'sub_b_to_a'::name, provider_dsn := $1)",
    )
    .bind(&dsn_a)
    .execute(&pool_b)
    .await
    .unwrap();

    // Give Spock time to initialize the logical replication workers
    sleep(Duration::from_millis(2000)).await;

    // 7. Test Active-Active Replication

    // Insert on A -> verify it replicates to B
    sqlx::query("INSERT INTO messages (id, content) VALUES (1, 'hello from A')")
        .execute(&pool_a)
        .await
        .unwrap();

    let mut replicated_to_b = false;
    for _ in 0..20 {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM messages WHERE id = 1")
            .fetch_one(&pool_b)
            .await
            .unwrap();
        if count.0 == 1 {
            replicated_to_b = true;
            break;
        }
        sleep(Duration::from_millis(200)).await;
    }
    assert!(
        replicated_to_b,
        "Data from Node A did not replicate to Node B"
    );

    // Insert on B -> verify it replicates to A
    sqlx::query("INSERT INTO messages (id, content) VALUES (2, 'hello from B')")
        .execute(&pool_b)
        .await
        .unwrap();

    let mut replicated_to_a = false;
    for _ in 0..20 {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM messages WHERE id = 2")
            .fetch_one(&pool_a)
            .await
            .unwrap();
        if count.0 == 1 {
            replicated_to_a = true;
            break;
        }
        sleep(Duration::from_millis(200)).await;
    }
    assert!(
        replicated_to_a,
        "Data from Node B did not replicate to Node A"
    );

    // 8. Test UPDATE propagation (Node A -> Node B)
    sqlx::query("UPDATE messages SET content = 'updated from A' WHERE id = 1")
        .execute(&pool_a)
        .await
        .unwrap();

    let mut update_replicated = false;
    for _ in 0..20 {
        let content: (String,) = sqlx::query_as("SELECT content FROM messages WHERE id = 1")
            .fetch_one(&pool_b)
            .await
            .unwrap();
        if content.0 == "updated from A" {
            update_replicated = true;
            break;
        }
        sleep(Duration::from_millis(200)).await;
    }
    assert!(
        update_replicated,
        "UPDATE from Node A did not replicate to Node B"
    );

    // 9. Test DELETE propagation (Node B -> Node A)
    sqlx::query("DELETE FROM messages WHERE id = 2")
        .execute(&pool_b)
        .await
        .unwrap();

    let mut delete_replicated = false;
    for _ in 0..20 {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM messages WHERE id = 2")
            .fetch_one(&pool_a)
            .await
            .unwrap();
        if count.0 == 0 {
            delete_replicated = true;
            break;
        }
        sleep(Duration::from_millis(200)).await;
    }
    assert!(
        delete_replicated,
        "DELETE from Node B did not replicate to Node A"
    );

    // 10. Test Offline Node Recovery
    // Stop Node B entirely
    pool_b.close().await;
    node_b.stop().await.unwrap();

    // Insert data while Node B is offline
    sqlx::query("INSERT INTO messages (id, content) VALUES (3, 'inserted while B was offline')")
        .execute(&pool_a)
        .await
        .unwrap();

    // Bring Node B back online
    let config_b_restarted = node_b.config().clone();
    node_b = Postg::start(config_b_restarted)
        .await
        .expect("Failed to restart Node B");
    let pool_b = sqlx::PgPool::connect(&node_b.connection_string())
        .await
        .unwrap();

    // Verify Node B catches up
    let mut offline_sync = false;
    for _ in 0..40 {
        // Give it a bit more time to startup and sync
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM messages WHERE id = 3")
            .fetch_one(&pool_b)
            .await
            .unwrap_or((0,));
        if count.0 == 1 {
            offline_sync = true;
            break;
        }
        sleep(Duration::from_millis(200)).await;
    }
    assert!(
        offline_sync,
        "Node B did not sync missed data after coming back online"
    );

    // 11. Test Conflict Resolution
    // We update the exact same row concurrently on both nodes. Spock should use commit timestamps to resolve it.
    sqlx::query("UPDATE messages SET content = 'conflict A' WHERE id = 3")
        .execute(&pool_a)
        .await
        .unwrap();

    sqlx::query("UPDATE messages SET content = 'conflict B' WHERE id = 3")
        .execute(&pool_b)
        .await
        .unwrap();

    // Wait for the conflict to propagate (though it will cause divergence by default)
    sleep(Duration::from_millis(1000)).await;

    // TODO: Phase 3/4 - Configure Spock's conflict resolution (e.g. last_update_wins).
    // By default, it rejects the remote conflict, causing divergence.

    // 12. Test DDL Replication (Schema Changes)
    // Spock supports DDL replication if spock.enable_ddl_replication is on.
    sqlx::query("ALTER TABLE messages ADD COLUMN created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP")
        .execute(&pool_a)
        .await
        .unwrap();

    // Wait for DDL to propagate
    let mut ddl_replicated = false;
    for _ in 0..20 {
        // We check if the column exists on Node B by attempting to select it
        let result = sqlx::query("SELECT created_at FROM messages LIMIT 1")
            .execute(&pool_b)
            .await;
        
        if result.is_ok() {
            ddl_replicated = true;
            break;
        }
        sleep(Duration::from_millis(500)).await;
    }
    // We expect this to fail because we haven't invoked spock.replicate_ddl_command()!
    // Regular ALTER TABLE doesn't get replicated automatically in basic Spock without trigger or function call.
    // However, it's good to know the limitations. We won't assert it strictly if it fails.
    // Actually, spock.enable_ddl_replication = on intercepts DDLs. Let's see if it works!
    assert!(
        ddl_replicated,
        "DDL (ALTER TABLE) did not replicate from Node A to Node B"
    );

    pool_a.close().await;
    pool_b.close().await;

    node_a.stop().await.unwrap();
    node_b.stop().await.unwrap();
}
