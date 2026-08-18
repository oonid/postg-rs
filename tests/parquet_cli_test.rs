use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn test_parquet_dump_and_restore_e2e() {
    let tmp = TempDir::new().unwrap();
    let data_dir = tmp.path().join("pgdata");
    let parquet_file = tmp.path().join("output.parquet");

    // 1. Create table via query
    let mut cmd = Command::cargo_bin("postg").unwrap();
    cmd.arg("--data-dir")
        .arg(&data_dir)
        .arg("query")
        .arg("CREATE TABLE test_parquet (id SERIAL PRIMARY KEY, name TEXT, age INT)")
        .assert()
        .success();

    // 1b. Insert data via query
    let mut cmd = Command::cargo_bin("postg").unwrap();
    cmd.arg("--data-dir")
        .arg(&data_dir)
        .arg("query")
        .arg("INSERT INTO test_parquet (name, age) VALUES ('Alice', 30), ('Bob', 25), ('Charlie', 35)")
        .assert()
        .success();

    // 2. Dump to Parquet
    let mut cmd = Command::cargo_bin("postg").unwrap();
    cmd.arg("--data-dir")
        .arg(&data_dir)
        .arg("dump")
        .arg("--format")
        .arg("parquet")
        .arg("--query")
        .arg("SELECT id, name, age FROM test_parquet ORDER BY id")
        .arg("--file")
        .arg(&parquet_file)
        .assert()
        .success();

    assert!(parquet_file.exists(), "Parquet file should be created");

    // 3. Drop the table
    let mut cmd = Command::cargo_bin("postg").unwrap();
    cmd.arg("--data-dir")
        .arg(&data_dir)
        .arg("query")
        .arg("DROP TABLE test_parquet;")
        .assert()
        .success();

    // 4. Restore from Parquet
    let mut cmd = Command::cargo_bin("postg").unwrap();
    cmd.arg("--data-dir")
        .arg(&data_dir)
        .arg("restore")
        .arg("--format")
        .arg("parquet")
        .arg("--table")
        .arg("test_parquet")
        .arg("--create-table")
        .arg(&parquet_file)
        .assert()
        .success();

    // 5. Verify restored data
    let mut cmd = Command::cargo_bin("postg").unwrap();
    cmd.arg("--data-dir")
        .arg(&data_dir)
        .arg("query")
        .arg("SELECT * FROM test_parquet;")
        .assert()
        .success()
        .stdout(predicate::str::contains("3 row(s) returned"));
}
