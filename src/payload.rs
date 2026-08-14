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

    let archive_path = match archive_path {
        Some(path) => path.to_path_buf(),
        None => return Err(Error::Extract("postgres binaries not found and no archive path provided".to_string())),
    };
    let cache_dir = config.cache_dir.clone();

    tokio::task::spawn_blocking(move || {
        if install_dir.join(MARKER_FILE).exists() {
            return Ok(());
        }

        tracing::info!("extracting payload to {}", install_dir.display());

        fs::create_dir_all(&cache_dir).map_err(|e| {
            Error::Extract(format!("failed to create cache dir {}: {e}", cache_dir.display()))
        })?;

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
        archive.unpack(tmp_path).map_err(|e| {
            Error::Extract(format!("failed to unpack archive: {e}"))
        })?;

        // Write marker file inside temporary directory so it gets renamed atomically
        fs::write(tmp_path.join(MARKER_FILE), b"").map_err(|e| {
            Error::Extract(format!("failed to write marker: {e}"))
        })?;

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

