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
