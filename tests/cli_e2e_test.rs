use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

#[test]
fn test_cli_query_and_dump_sql() {
    let tmp = tempfile::TempDir::new().unwrap();
    let data_dir = tmp.path().to_str().unwrap();

    // 1. Start and create a table via query
    Command::cargo_bin("postg")
        .unwrap()
        .args(["--data-dir", data_dir, "query", "CREATE TABLE my_cli_test (id INT)"])
        .assert()
        .success();

    // 2. Insert data
    Command::cargo_bin("postg")
        .unwrap()
        .args(["--data-dir", data_dir, "query", "INSERT INTO my_cli_test VALUES (99)"])
        .assert()
        .success();

    // 3. Dump to SQL
    let dump_file = tmp.path().join("out.sql");
    Command::cargo_bin("postg")
        .unwrap()
        .args(["--data-dir", data_dir, "dump", "--file", dump_file.to_str().unwrap()])
        .assert()
        .success();

    let sql = fs::read_to_string(&dump_file).unwrap();
    assert!(sql.contains("CREATE TABLE public.my_cli_test"));

    // 4. Restore SQL (to the same DB)
    Command::cargo_bin("postg")
        .unwrap()
        .args(["--data-dir", data_dir, "restore", dump_file.to_str().unwrap()])
        .assert()
        .success();
}

#[test]
fn test_cli_sync_commands() {
    let tmp = tempfile::TempDir::new().unwrap();
    let data_dir = tmp.path().to_str().unwrap();

    // 1. Sync init
    Command::cargo_bin("postg")
        .unwrap()
        .args([
            "--engine", "postgresql-spock",
            "--data-dir", data_dir,
            "sync", "init",
            "--node-name", "test_node",
            "--dsn", "host=127.0.0.1 port=5432 user=postgres dbname=postgres"
        ])
        .assert()
        .success();

    // 2. Sync publish
    Command::cargo_bin("postg")
        .unwrap()
        .args([
            "--engine", "postgresql-spock",
            "--data-dir", data_dir,
            "sync", "publish",
            "--schema", "public"
        ])
        .assert()
        .success();

    // 3. Sync status
    Command::cargo_bin("postg")
        .unwrap()
        .args([
            "--engine", "postgresql-spock",
            "--data-dir", data_dir,
            "sync", "status"
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Sync Status"));
}
