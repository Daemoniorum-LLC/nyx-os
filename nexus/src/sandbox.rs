//! Sandboxed package building

use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tracing::{info, debug, warn};

use crate::package::{PackageDefinition, BuiltPackage, PackageFile, hash_file, hash_data};

/// Build sandbox
pub struct BuildSandbox {
    work_dir: PathBuf,
    root_dir: PathBuf,
}

impl BuildSandbox {
    pub fn new() -> Result<Self> {
        let work_dir = tempfile::tempdir()?.into_path();
        let root_dir = work_dir.join("root");

        std::fs::create_dir_all(&root_dir)?;

        Ok(Self { work_dir, root_dir })
    }

    /// Build a package from definition
    pub async fn build(&self, path: &str) -> Result<BuiltPackage> {
        let def = PackageDefinition::from_file(Path::new(path))?;

        info!("Building {} {}", def.package.name, def.package.version);

        // Fetch source
        let source_dir = self.fetch_source(&def).await?;

        // Set up build environment
        self.setup_environment(&def)?;

        // Run build
        self.run_build(&def, &source_dir).await?;

        // Install to destdir
        let dest_dir = self.work_dir.join("dest");
        std::fs::create_dir_all(&dest_dir)?;
        self.run_install(&def, &source_dir, &dest_dir).await?;

        // Package result
        self.create_package(&def, &dest_dir)
    }

    async fn fetch_source(&self, def: &PackageDefinition) -> Result<PathBuf> {
        let source_dir = self.work_dir.join("src");
        std::fs::create_dir_all(&source_dir)?;

        if let Some(ref url) = def.source.url {
            info!("Fetching source from {}", url);

            let client = reqwest::Client::new();
            let response = client.get(url).send().await?;
            let bytes = response.bytes().await?;

            // Verify hash
            if let Some(ref expected) = def.source.sha256 {
                let actual = hash_data(&bytes);
                if &actual != expected {
                    return Err(anyhow!("Source hash mismatch"));
                }
            }

            // Extract based on extension
            let archive_path = source_dir.join("source.tar.gz");
            std::fs::write(&archive_path, &bytes)?;

            self.extract_archive(&archive_path, &source_dir)?;

        } else if let Some(ref git_url) = def.source.git {
            info!("Cloning from {}", git_url);

            let status = tokio::process::Command::new("git")
                .args(["clone", "--depth", "1", git_url, "repo"])
                .current_dir(&source_dir)
                .status()
                .await?;

            if !status.success() {
                return Err(anyhow!("Git clone failed"));
            }

            return Ok(source_dir.join("repo"));
        }

        // Apply patches
        for patch in &def.source.patches {
            let patch_path = PathBuf::from(patch);
            self.apply_patch(&source_dir, &patch_path).await?;
        }

        // Find source directory (usually first subdir after extraction)
        for entry in std::fs::read_dir(&source_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                return Ok(entry.path());
            }
        }

        Ok(source_dir)
    }

    fn extract_archive(&self, archive: &Path, dest: &Path) -> Result<()> {
        use flate2::read::GzDecoder;
        use tar::Archive;

        let file = std::fs::File::open(archive)?;
        let decoder = GzDecoder::new(file);
        let mut ar = Archive::new(decoder);

        ar.unpack(dest)?;

        Ok(())
    }

    async fn apply_patch(&self, source_dir: &Path, patch: &Path) -> Result<()> {
        let status = tokio::process::Command::new("patch")
            .args(["-p1", "-i"])
            .arg(patch)
            .current_dir(source_dir)
            .status()
            .await?;

        if !status.success() {
            return Err(anyhow!("Patch failed: {:?}", patch));
        }

        Ok(())
    }

    fn setup_environment(&self, def: &PackageDefinition) -> Result<()> {
        // Set up minimal filesystem in sandbox
        let dirs = ["bin", "lib", "include", "share"];
        for dir in dirs {
            std::fs::create_dir_all(self.root_dir.join(dir))?;
        }

        Ok(())
    }

    async fn run_build(&self, def: &PackageDefinition, source_dir: &Path) -> Result<()> {
        let build_dir = self.work_dir.join("build");
        std::fs::create_dir_all(&build_dir)?;

        // Configure
        for cmd in &def.build.configure {
            self.run_command(cmd, source_dir).await?;
        }

        // Build
        for cmd in &def.build.build {
            self.run_command(cmd, source_dir).await?;
        }

        // Check (optional)
        for cmd in &def.build.check {
            if let Err(e) = self.run_command(cmd, source_dir).await {
                warn!("Check failed: {}", e);
            }
        }

        Ok(())
    }

    async fn run_install(
        &self,
        def: &PackageDefinition,
        source_dir: &Path,
        dest_dir: &Path,
    ) -> Result<()> {
        // Set DESTDIR for install
        std::env::set_var("DESTDIR", dest_dir);

        for cmd in &def.install.commands {
            self.run_command(cmd, source_dir).await?;
        }

        // Copy explicit files
        for (src, dst) in &def.install.files {
            let src_path = source_dir.join(src);
            let dst_path = dest_dir.join(dst);

            if let Some(parent) = dst_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            std::fs::copy(&src_path, &dst_path)?;
        }

        Ok(())
    }

    async fn run_command(&self, cmd: &str, cwd: &Path) -> Result<()> {
        debug!("Running: {}", cmd);

        let output = tokio::process::Command::new("sh")
            .args(["-c", cmd])
            .current_dir(cwd)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()
            .await?;

        if !output.status.success() {
            return Err(anyhow!("Command failed: {}", cmd));
        }

        Ok(())
    }

    fn create_package(&self, def: &PackageDefinition, dest_dir: &Path) -> Result<BuiltPackage> {
        let mut files = Vec::new();
        let mut content_hash = sha2::Sha256::new();

        for entry in walkdir::WalkDir::new(dest_dir) {
            let entry = entry?;

            if entry.file_type().is_file() {
                let rel_path = entry.path()
                    .strip_prefix(dest_dir)?
                    .to_string_lossy()
                    .to_string();

                let hash = hash_file(entry.path())?;
                let size = entry.metadata()?.len();
                let mode = entry.metadata()?.permissions().mode();

                use sha2::Digest;
                content_hash.update(hash.as_bytes());

                files.push(PackageFile {
                    path: rel_path,
                    hash,
                    size,
                    mode,
                });
            }
        }

        use sha2::Digest;
        let store_hash = hex::encode(&content_hash.finalize()[..6]);

        Ok(BuiltPackage {
            name: def.package.name.clone(),
            version: def.package.version.clone(),
            description: def.package.description.clone(),
            license: def.package.license.clone(),
            dependencies: def.package.dependencies.clone(),
            files,
            store_hash,
        })
    }
}

impl Drop for BuildSandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.work_dir);
    }
}

use std::os::unix::fs::PermissionsExt;
