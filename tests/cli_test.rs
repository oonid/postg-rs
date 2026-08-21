use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn cli_help_works() {
    let mut cmd = Command::cargo_bin("postg").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage:"));
}

#[test]
fn cli_status_fails_on_empty_dir() {
    let tmp = tempfile::TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("postg").unwrap();
    cmd.arg("--data-dir")
        .arg(tmp.path())
        .arg("status")
        .assert()
        .failure(); // pg_ctl should fail because there is no DB
}

#[test]
fn cli_stop_fails_on_empty_dir() {
    let tmp = tempfile::TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("postg").unwrap();
    cmd.arg("--data-dir")
        .arg(tmp.path())
        .arg("stop")
        .assert()
        .failure(); // pg_ctl should fail because there is no DB
}
#[test]
fn cli_dump_help_shows_parquet_flags() {
    let mut cmd = Command::cargo_bin("postg").unwrap();
    cmd.args(["dump", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--format"))
        .stdout(predicate::str::contains("--query"));
}

#[test]
fn cli_restore_help_shows_parquet_flags() {
    let mut cmd = Command::cargo_bin("postg").unwrap();
    cmd.args(["restore", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--format"))
        .stdout(predicate::str::contains("--table"))
        .stdout(predicate::str::contains("--create-table"));
}

#[test]
fn cli_engine_variants_work() {
    let tmp = tempfile::TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("postg").unwrap();
    cmd.arg("--data-dir").arg(tmp.path().join("pg1"))
       .arg("--engine").arg("postgresql-without-llvm")
       .arg("status").assert().failure();
        
    let mut cmd2 = Command::cargo_bin("postg").unwrap();
    cmd2.arg("--data-dir").arg(tmp.path().join("pg2"))
        .arg("--engine").arg("postgresql-pgvector")
        .arg("status").assert().failure();
}
