use postg::config::Config;
use postg::error::Error;
use postg::payload;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn create_test_archive(archive_path: &Path) {
    let file = fs::File::create(archive_path).unwrap();
    let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut tar = tar::Builder::new(enc);
    let content = b"#!/bin/sh\necho hello\n";
    let mut header = tar::Header::new_gnu();
    header.set_path("bin/postgres").unwrap();
    header.set_size(content.len() as u64);
    header.set_mode(0o755);
    header.set_cksum();
    tar.append(&header, &content[..]).unwrap();
    tar.into_inner().unwrap().finish().unwrap();
}

#[tokio::test]
async fn extract_creates_bin_directory() {
    let tmp = TempDir::new().unwrap();
    let archive_path = tmp.path().join("test.tar.gz");
    create_test_archive(&archive_path);

    let config = Config {
        cache_dir: tmp.path().join("cache"),
        ..Config::default()
    };

    assert!(!payload::is_extracted(&config));
    payload::extract_payload(&config, Some(&archive_path))
        .await
        .unwrap();
    assert!(payload::is_extracted(&config));
    assert!(config.pg_bin("postgres").exists());
}

#[tokio::test]
async fn extract_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    let archive_path = tmp.path().join("test.tar.gz");
    create_test_archive(&archive_path);

    let config = Config {
        cache_dir: tmp.path().join("cache"),
        ..Config::default()
    };

    payload::extract_payload(&config, Some(&archive_path))
        .await
        .unwrap();
    // Second call should succeed without error (idempotent)
    payload::extract_payload(&config, Some(&archive_path))
        .await
        .unwrap();
    assert!(payload::is_extracted(&config));
}

#[tokio::test]
async fn extract_returns_error_on_missing_file() {
    let tmp = TempDir::new().unwrap();
    let archive_path = tmp.path().join("nonexistent.tar.gz");

    let config = Config {
        cache_dir: tmp.path().join("cache"),
        ..Config::default()
    };

    let result = payload::extract_payload(&config, Some(&archive_path)).await;
    assert!(matches!(result, Err(Error::Extract(_))));
}

#[tokio::test]
async fn extract_returns_error_on_corrupt_archive() {
    let tmp = TempDir::new().unwrap();
    let archive_path = tmp.path().join("corrupt.tar.gz");
    fs::write(&archive_path, b"not a real gzip file").unwrap();

    let config = Config {
        cache_dir: tmp.path().join("cache"),
        ..Config::default()
    };

    let result = payload::extract_payload(&config, Some(&archive_path)).await;
    assert!(matches!(result, Err(Error::Extract(_))));
    assert!(!config.install_dir().exists());
}

#[tokio::test]
async fn extract_partial_unpack_failure_leaves_no_install_dir() {
    let tmp = TempDir::new().unwrap();
    let archive_path = tmp.path().join("partial.tar.gz");

    // Create an archive header claiming 1000 bytes but write fewer bytes to cause unpack failure
    let file = fs::File::create(&archive_path).unwrap();
    let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut tar = tar::Builder::new(enc);
    let content = b"short content";
    let mut header = tar::Header::new_gnu();
    header.set_path("bin/postgres").unwrap();
    header.set_size(1000);
    header.set_cksum();
    let _ = tar.append(&header, &content[..]);

    let config = Config {
        cache_dir: tmp.path().join("cache"),
        ..Config::default()
    };

    let result = payload::extract_payload(&config, Some(&archive_path)).await;
    assert!(matches!(result, Err(Error::Extract(_))));
    assert!(!config.install_dir().exists());
}

#[tokio::test]
async fn extract_downloads_from_url_success() {
    let tmp = TempDir::new().unwrap();
    let mut server = mockito::Server::new_async().await;

    // Create a valid tiny tar.gz in memory
    let mut archive_bytes = Vec::new();
    {
        let enc = flate2::write::GzEncoder::new(&mut archive_bytes, flate2::Compression::default());
        let mut tar = tar::Builder::new(enc);
        let content = b"#!/bin/sh\necho hello\n";
        let mut header = tar::Header::new_gnu();
        header.set_path("bin/postgres").unwrap();
        header.set_size(content.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        tar.append(&header, &content[..]).unwrap();
        tar.into_inner().unwrap().finish().unwrap();
    }

    let mock = server.mock("GET", mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/gzip")
        .with_body(archive_bytes)
        .create_async().await;

    std::env::set_var("POSTG_DOWNLOAD_URL", server.url());

    let config = Config {
        cache_dir: tmp.path().join("cache"),
        ..Config::default()
    };

    payload::extract_payload(&config, None).await.unwrap();

    mock.assert_async().await;
    assert!(payload::is_extracted(&config));
    assert!(config.pg_bin("postgres").exists());
}

#[tokio::test]
async fn extract_downloads_http_error() {
    let tmp = TempDir::new().unwrap();
    let mut server = mockito::Server::new_async().await;

    let mock = server.mock("GET", mockito::Matcher::Any)
        .with_status(404)
        .create_async().await;

    std::env::set_var("POSTG_DOWNLOAD_URL", server.url());

    let config = Config {
        cache_dir: tmp.path().join("cache"),
        ..Config::default()
    };

    let result = payload::extract_payload(&config, None).await;
    assert!(matches!(result, Err(Error::Extract(_))));
    
    mock.assert_async().await;
}

#[tokio::test]
async fn extract_downloads_network_drop() {
    let tmp = TempDir::new().unwrap();
    
    // We point to a URL that refuses connection
    std::env::set_var("POSTG_DOWNLOAD_URL", "http://127.0.0.1:1"); 

    let config = Config {
        cache_dir: tmp.path().join("cache"),
        ..Config::default()
    };

    let result = payload::extract_payload(&config, None).await;
    assert!(matches!(result, Err(Error::Extract(_))));
}
