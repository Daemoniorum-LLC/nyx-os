//! Package download cache

use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};
use tracing::{info, debug};

use crate::package::RepoPackage;

/// Package cache
pub struct PackageCache {
    path: PathBuf,
}

impl PackageCache {
    pub fn open(path: &str) -> Result<Self> {
        let path = PathBuf::from(path);
        std::fs::create_dir_all(&path)?;

        Ok(Self { path })
    }

    /// Get cached package path or download
    pub async fn get_or_download(&self, pkg: &RepoPackage) -> Result<PathBuf> {
        let cache_path = self.package_path(pkg);

        if cache_path.exists() {
            // Verify cached package
            let hash = crate::package::hash_file(&cache_path)?;
            if hash == pkg.sha256 {
                debug!("Using cached: {:?}", cache_path);
                return Ok(cache_path);
            }

            // Hash mismatch, re-download
            std::fs::remove_file(&cache_path)?;
        }

        // Download
        self.download(pkg).await
    }

    fn package_path(&self, pkg: &RepoPackage) -> PathBuf {
        self.path.join(format!("{}-{}.nyx", pkg.name, pkg.version))
    }

    async fn download(&self, pkg: &RepoPackage) -> Result<PathBuf> {
        use futures::StreamExt;
        use indicatif::{ProgressBar, ProgressStyle};

        info!("Downloading {} {}", pkg.name, pkg.version);

        let client = reqwest::Client::new();
        let response = client.get(&pkg.url).send().await?;

        if !response.status().is_success() {
            return Err(anyhow!("Download failed: {}", response.status()));
        }

        let total_size = response.content_length().unwrap_or(pkg.download_size);

        // Progress bar
        let pb = ProgressBar::new(total_size);
        pb.set_style(ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")?
            .progress_chars("#>-"));

        let dest_path = self.package_path(pkg);
        let mut file = std::fs::File::create(&dest_path)?;

        let mut stream = response.bytes_stream();
        let mut hasher = sha2::Sha256::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            use sha2::Digest;
            use std::io::Write;

            hasher.update(&chunk);
            file.write_all(&chunk)?;
            pb.inc(chunk.len() as u64);
        }

        pb.finish_with_message("Downloaded");

        // Verify hash
        use sha2::Digest;
        let hash = hex::encode(hasher.finalize());
        if hash != pkg.sha256 {
            std::fs::remove_file(&dest_path)?;
            return Err(anyhow!(
                "Hash mismatch: expected {}, got {}",
                pkg.sha256,
                hash
            ));
        }

        Ok(dest_path)
    }

    /// Get total cache size
    pub fn size(&self) -> Result<u64> {
        let mut total = 0;

        for entry in std::fs::read_dir(&self.path)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                total += entry.metadata()?.len();
            }
        }

        Ok(total)
    }

    /// Clean old packages (keep only latest version of each)
    pub fn clean_old(&self) -> Result<u64> {
        use std::collections::HashMap;

        let mut packages: HashMap<String, Vec<PathBuf>> = HashMap::new();

        for entry in std::fs::read_dir(&self.path)? {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if let Some(pkg_name) = name_str.split('-').next() {
                packages.entry(pkg_name.to_string())
                    .or_default()
                    .push(entry.path());
            }
        }

        let mut freed = 0;

        for (_, mut versions) in packages {
            // Sort by modification time, newest first
            versions.sort_by(|a, b| {
                let a_time = a.metadata().and_then(|m| m.modified()).ok();
                let b_time = b.metadata().and_then(|m| m.modified()).ok();
                b_time.cmp(&a_time)
            });

            // Keep the newest, remove others
            for old in versions.into_iter().skip(1) {
                if let Ok(meta) = old.metadata() {
                    freed += meta.len();
                }
                let _ = std::fs::remove_file(&old);
            }
        }

        Ok(freed)
    }

    /// Clean all cached packages
    pub fn clean_all(&self) -> Result<u64> {
        let mut freed = 0;

        for entry in std::fs::read_dir(&self.path)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                if let Ok(meta) = entry.metadata() {
                    freed += meta.len();
                }
                let _ = std::fs::remove_file(entry.path());
            }
        }

        Ok(freed)
    }
}
