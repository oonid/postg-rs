use crate::config::Config;
use crate::error::{Error, Result};
use std::fs;
use std::path::Path;

const MARKER_FILE: &str = ".extracted";

pub fn is_extracted(config: &Config) -> bool {
    config.install_dir().join(MARKER_FILE).exists()
}

pub async fn extract_payload(config: &Config, archive_path: Option<&Path>) -> Result<()> {
    let install_dir = config.install_dir();

    if is_extracted(config) {
        tracing::debug!("payload already extracted to {}", install_dir.display());
        return Ok(());
    }

    let cache_dir = config.cache_dir.clone();
    fs::create_dir_all(&cache_dir).map_err(|e| {
        Error::Extract(format!(
            "failed to create cache dir {}: {e}",
            cache_dir.display()
        ))
    })?;

    let archive_path_opt = archive_path.map(|p| p.to_path_buf());

    tokio::task::spawn_blocking(move || {
        if install_dir.join(MARKER_FILE).exists() {
            return Ok(());
        }

        let archive_path = match archive_path_opt {
            Some(path) => path,
            None => {
                // Determine architecture and OS
                let target = format!("{}-unknown-{}-gnu", std::env::consts::ARCH, std::env::consts::OS);
                let engine_str = match config.engine {
                    crate::config::Engine::Postgresql => "postgresql",
                    crate::config::Engine::PostgresqlSpock => "postgresql-spock",
                };
                let pg_major = 17; // Default to 17
                let file_name = format!("{}-{}-{}.tar.gz", engine_str, pg_major, target);
                let download_url = format!("https://github.com/oonid/postg-rs/releases/download/v0.1.0/{}", file_name);
                
                let local_archive_path = cache_dir.join(&file_name);
                if !local_archive_path.exists() {
                    tracing::info!("Downloading postgres binary from {}", download_url);
                    let mut response = reqwest::blocking::get(&download_url).map_err(|e| {
                        Error::Extract(format!("failed to download {}: {}", download_url, e))
                    })?;
                    
                    if !response.status().is_success() {
                        return Err(Error::Extract(format!("failed to download {}: HTTP {}", download_url, response.status())));
                    }

                    let mut dest = fs::File::create(&local_archive_path).map_err(|e| {
                        Error::Extract(format!("failed to create cache file {}: {}", local_archive_path.display(), e))
                    })?;
                    
                    std::io::copy(&mut response, &mut dest).map_err(|e| {
                        Error::Extract(format!("failed to write cache file {}: {}", local_archive_path.display(), e))
                    })?;
                    tracing::info!("Download complete: {}", local_archive_path.display());
                } else {
                    tracing::info!("Using cached postgres binary: {}", local_archive_path.display());
                }
                local_archive_path
            }
        };

        tracing::info!("extracting payload to {}", install_dir.display());

        let tmp_dir = tempfile::Builder::new()
            .prefix(".extract-")
            .tempdir_in(&cache_dir)
            .map_err(|e| Error::Extract(format!("failed to create temp extraction dir: {e}")))?;

        let tmp_path = tmp_dir.path();

        let file = fs::File::open(&archive_path).map_err(|e| {
            Error::Extract(format!("failed to open {}: {e}", archive_path.display()))
        })?;

        let decoder = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        archive
            .unpack(tmp_path)
            .map_err(|e| Error::Extract(format!("failed to unpack archive: {e}")))?;

        // Write marker file inside temporary directory so it gets renamed atomically
        fs::write(tmp_path.join(MARKER_FILE), b"")
            .map_err(|e| Error::Extract(format!("failed to write marker: {e}")))?;

        if install_dir.exists() {
            let _ = fs::remove_dir_all(&install_dir);
        }

        fs::rename(tmp_path, &install_dir).map_err(|e| {
            Error::Extract(format!(
                "failed to move extracted payload from {} to {}: {e}",
                tmp_path.display(),
                install_dir.display()
            ))
        })?;

        // Prevent tempdir destructor from deleting the renamed directory
        let _ = tmp_dir.keep();

        tracing::info!("payload extracted successfully");
        Ok(())
    })
    .await
    .map_err(|e| Error::Extract(format!("extraction task panicked: {e}")))?
}
