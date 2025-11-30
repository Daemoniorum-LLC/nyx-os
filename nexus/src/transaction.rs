//! Atomic package transactions

use anyhow::{Result, anyhow};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::{info, warn, debug, error};

use crate::package::{RepoPackage, InstalledPackage, hash_file};
use crate::store::PackageStore;
use crate::cache::PackageCache;
use crate::repository::RepositoryManager;

/// Transaction operation
#[derive(Debug, Clone)]
pub enum Operation {
    Install(RepoPackage),
    Remove(String),
    Upgrade(String, RepoPackage),
}

/// Package transaction
pub struct Transaction<'a> {
    store: &'a PackageStore,
    operations: Vec<Operation>,
    autoremove: bool,
}

impl<'a> Transaction<'a> {
    pub fn new(store: &'a PackageStore) -> Self {
        Self {
            store,
            operations: Vec::new(),
            autoremove: false,
        }
    }

    pub fn add_install(&mut self, pkg: RepoPackage) {
        self.operations.push(Operation::Install(pkg));
    }

    pub fn add_remove(&mut self, name: &str) {
        self.operations.push(Operation::Remove(name.to_string()));
    }

    pub fn add_upgrade(&mut self, name: &str, pkg: RepoPackage) {
        self.operations.push(Operation::Upgrade(name.to_string(), pkg));
    }

    pub fn add_autoremove(&mut self) {
        self.autoremove = true;
    }

    /// Commit transaction atomically
    pub async fn commit(self) -> Result<()> {
        if self.operations.is_empty() && !self.autoremove {
            info!("Nothing to do");
            return Ok(());
        }

        // Create new generation
        let generation = self.store.next_generation()?;
        info!("Creating generation {}", generation);

        let gen_path = self.store.generation_path(generation);
        std::fs::create_dir_all(&gen_path)?;

        // Copy current state
        if generation > 1 {
            let prev_path = self.store.generation_path(generation - 1);
            self.copy_generation(&prev_path, &gen_path)?;
        }

        // Execute operations
        let cache = PackageCache::open("/var/cache/nexus")?;

        for op in &self.operations {
            match op {
                Operation::Install(pkg) => {
                    self.execute_install(pkg, &gen_path, &cache).await?;
                }
                Operation::Remove(name) => {
                    self.execute_remove(name, &gen_path)?;
                }
                Operation::Upgrade(name, pkg) => {
                    self.execute_remove(name, &gen_path)?;
                    self.execute_install(pkg, &gen_path, &cache).await?;
                }
            }
        }

        // Handle autoremove
        if self.autoremove {
            // TODO: Find and remove orphans
        }

        // Activate new generation
        self.store.activate_generation(generation)?;

        info!("Transaction complete");
        Ok(())
    }

    fn copy_generation(&self, from: &Path, to: &Path) -> Result<()> {
        let db_file = from.join("packages.json");
        if db_file.exists() {
            std::fs::copy(&db_file, to.join("packages.json"))?;
        }
        Ok(())
    }

    async fn execute_install(
        &self,
        pkg: &RepoPackage,
        gen_path: &Path,
        cache: &PackageCache,
    ) -> Result<()> {
        info!("Installing {} {}", pkg.name, pkg.version);

        // Download if not cached
        let archive_path = cache.get_or_download(pkg).await?;

        // Extract to store
        let store_path = self.extract_package(&archive_path, pkg)?;

        // Create symlinks in system
        self.link_package(&store_path)?;

        // Record installation
        let installed = InstalledPackage {
            name: pkg.name.clone(),
            version: pkg.version.clone(),
            description: pkg.description.clone(),
            license: pkg.license.clone(),
            dependencies: pkg.dependencies.clone(),
            store_path: store_path.to_string_lossy().to_string(),
            installed_size: pkg.installed_size,
            install_time: chrono::Utc::now(),
            files: self.list_package_files(&store_path)?,
            file_hashes: self.hash_package_files(&store_path)?,
            explicit: true,
        };

        self.store.record_install(gen_path, &installed)?;

        Ok(())
    }

    fn extract_package(&self, archive_path: &Path, pkg: &RepoPackage) -> Result<PathBuf> {
        use flate2::read::GzDecoder;
        use tar::Archive;

        let store_path = PathBuf::from(format!(
            "/nyx/store/{}-{}-{}",
            &pkg.sha256[..12],
            pkg.name,
            pkg.version
        ));

        if store_path.exists() {
            debug!("Package already in store: {:?}", store_path);
            return Ok(store_path);
        }

        std::fs::create_dir_all(&store_path)?;

        let file = std::fs::File::open(archive_path)?;
        let decoder = GzDecoder::new(file);
        let mut archive = Archive::new(decoder);

        archive.unpack(&store_path)?;

        // Make store path read-only
        self.make_readonly(&store_path)?;

        Ok(store_path)
    }

    fn make_readonly(&self, path: &Path) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;

        for entry in walkdir::WalkDir::new(path) {
            let entry = entry?;
            let metadata = entry.metadata()?;
            let mut perms = metadata.permissions();

            if metadata.is_file() {
                // Remove write permission
                let mode = perms.mode() & !0o222;
                perms.set_mode(mode);
                std::fs::set_permissions(entry.path(), perms)?;
            }
        }

        Ok(())
    }

    fn link_package(&self, store_path: &Path) -> Result<()> {
        // Link binaries
        let bin_dir = store_path.join("bin");
        if bin_dir.exists() {
            for entry in std::fs::read_dir(&bin_dir)? {
                let entry = entry?;
                let dest = PathBuf::from("/usr/bin").join(entry.file_name());
                let _ = std::fs::remove_file(&dest);
                std::os::unix::fs::symlink(entry.path(), &dest)?;
            }
        }

        // Link libraries
        let lib_dir = store_path.join("lib");
        if lib_dir.exists() {
            for entry in std::fs::read_dir(&lib_dir)? {
                let entry = entry?;
                let dest = PathBuf::from("/usr/lib").join(entry.file_name());
                let _ = std::fs::remove_file(&dest);
                std::os::unix::fs::symlink(entry.path(), &dest)?;
            }
        }

        // Link headers
        let include_dir = store_path.join("include");
        if include_dir.exists() {
            let dest = PathBuf::from("/usr/include").join(
                store_path.file_name().unwrap()
            );
            let _ = std::fs::remove_file(&dest);
            std::os::unix::fs::symlink(&include_dir, &dest)?;
        }

        Ok(())
    }

    fn execute_remove(&self, name: &str, gen_path: &Path) -> Result<()> {
        info!("Removing {}", name);

        // Get installed package info
        let pkg = self.store.get_installed(name)?
            .ok_or_else(|| anyhow!("Package not installed: {}", name))?;

        // Remove symlinks
        self.unlink_package(&pkg)?;

        // Record removal (don't delete from store - garbage collect later)
        self.store.record_remove(gen_path, name)?;

        Ok(())
    }

    fn unlink_package(&self, pkg: &InstalledPackage) -> Result<()> {
        let store_path = PathBuf::from(&pkg.store_path);

        // Unlink binaries
        let bin_dir = store_path.join("bin");
        if bin_dir.exists() {
            for entry in std::fs::read_dir(&bin_dir)? {
                let entry = entry?;
                let link = PathBuf::from("/usr/bin").join(entry.file_name());
                if link.is_symlink() {
                    if let Ok(target) = std::fs::read_link(&link) {
                        if target.starts_with(&store_path) {
                            std::fs::remove_file(&link)?;
                        }
                    }
                }
            }
        }

        // Unlink libraries
        let lib_dir = store_path.join("lib");
        if lib_dir.exists() {
            for entry in std::fs::read_dir(&lib_dir)? {
                let entry = entry?;
                let link = PathBuf::from("/usr/lib").join(entry.file_name());
                if link.is_symlink() {
                    if let Ok(target) = std::fs::read_link(&link) {
                        if target.starts_with(&store_path) {
                            std::fs::remove_file(&link)?;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn list_package_files(&self, store_path: &Path) -> Result<Vec<String>> {
        let mut files = Vec::new();

        for entry in walkdir::WalkDir::new(store_path) {
            let entry = entry?;
            if entry.file_type().is_file() {
                let rel_path = entry.path()
                    .strip_prefix(store_path)?
                    .to_string_lossy()
                    .to_string();
                files.push(rel_path);
            }
        }

        Ok(files)
    }

    fn hash_package_files(&self, store_path: &Path) -> Result<std::collections::HashMap<String, String>> {
        let mut hashes = std::collections::HashMap::new();

        for entry in walkdir::WalkDir::new(store_path) {
            let entry = entry?;
            if entry.file_type().is_file() {
                let rel_path = entry.path()
                    .strip_prefix(store_path)?
                    .to_string_lossy()
                    .to_string();
                let hash = hash_file(entry.path())?;
                hashes.insert(rel_path, hash);
            }
        }

        Ok(hashes)
    }
}

/// Garbage collect unreferenced store paths
pub fn garbage_collect(store: &PackageStore) -> Result<u64> {
    let store_path = PathBuf::from("/nyx/store");
    let mut freed = 0u64;

    // Get all referenced paths
    let referenced: HashSet<String> = store.all_store_paths()?
        .into_iter()
        .collect();

    // Find unreferenced
    for entry in std::fs::read_dir(&store_path)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let path_str = path.to_string_lossy().to_string();
            if !referenced.contains(&path_str) {
                info!("Collecting garbage: {:?}", path);
                let size = dir_size(&path)?;
                std::fs::remove_dir_all(&path)?;
                freed += size;
            }
        }
    }

    Ok(freed)
}

fn dir_size(path: &Path) -> Result<u64> {
    let mut size = 0;

    for entry in walkdir::WalkDir::new(path) {
        let entry = entry?;
        if entry.file_type().is_file() {
            size += entry.metadata()?.len();
        }
    }

    Ok(size)
}
