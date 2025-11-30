//! Content-addressable package store

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{info, debug};

use crate::package::{InstalledPackage, RepoPackage, hash_file};
use crate::repository::RepositoryManager;

/// Package store with generations
pub struct PackageStore {
    root: PathBuf,
    store_path: PathBuf,
    generations_path: PathBuf,
}

/// Generation metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Generation {
    pub number: u32,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub package_count: usize,
}

impl PackageStore {
    pub fn open(root: &str) -> Result<Self> {
        let root = PathBuf::from(root);
        let store_path = root.clone();
        let generations_path = root.join("generations");

        std::fs::create_dir_all(&store_path)?;
        std::fs::create_dir_all(&generations_path)?;

        Ok(Self {
            root,
            store_path,
            generations_path,
        })
    }

    /// Get current generation number
    pub fn current_generation(&self) -> u32 {
        let current_link = self.generations_path.join("current");

        if let Ok(target) = std::fs::read_link(&current_link) {
            target.file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.parse().ok())
                .unwrap_or(0)
        } else {
            0
        }
    }

    /// Get next generation number
    pub fn next_generation(&self) -> Result<u32> {
        Ok(self.current_generation() + 1)
    }

    /// Get path to generation
    pub fn generation_path(&self, gen: u32) -> PathBuf {
        self.generations_path.join(gen.to_string())
    }

    /// Activate a generation
    pub fn activate_generation(&self, gen: u32) -> Result<()> {
        let gen_path = self.generation_path(gen);
        if !gen_path.exists() {
            return Err(anyhow!("Generation {} does not exist", gen));
        }

        let current_link = self.generations_path.join("current");
        let _ = std::fs::remove_file(&current_link);
        std::os::unix::fs::symlink(&gen_path, &current_link)?;

        info!("Activated generation {}", gen);
        Ok(())
    }

    /// List all generations
    pub fn list_generations(&self) -> Result<Vec<Generation>> {
        let mut generations = Vec::new();

        for entry in std::fs::read_dir(&self.generations_path)? {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if name_str == "current" {
                continue;
            }

            if let Ok(number) = name_str.parse::<u32>() {
                let meta = entry.metadata()?;
                let packages = self.load_generation_packages(&entry.path())
                    .unwrap_or_default();

                generations.push(Generation {
                    number,
                    timestamp: chrono::DateTime::from(meta.modified()?),
                    package_count: packages.len(),
                });
            }
        }

        generations.sort_by_key(|g| g.number);
        Ok(generations)
    }

    /// Load packages for a generation
    fn load_generation_packages(&self, path: &Path) -> Result<Vec<InstalledPackage>> {
        let db_file = path.join("packages.json");

        if !db_file.exists() {
            return Ok(Vec::new());
        }

        let content = std::fs::read_to_string(&db_file)?;
        Ok(serde_json::from_str(&content)?)
    }

    /// Get installed packages in current generation
    pub fn list_installed(&self) -> Result<Vec<InstalledPackage>> {
        let gen = self.current_generation();
        if gen == 0 {
            return Ok(Vec::new());
        }

        let gen_path = self.generation_path(gen);
        self.load_generation_packages(&gen_path)
    }

    /// Get explicitly installed packages
    pub fn list_explicit(&self) -> Result<Vec<InstalledPackage>> {
        Ok(self.list_installed()?
            .into_iter()
            .filter(|p| p.explicit)
            .collect())
    }

    /// Get a specific installed package
    pub fn get_installed(&self, name: &str) -> Result<Option<InstalledPackage>> {
        Ok(self.list_installed()?
            .into_iter()
            .find(|p| p.name == name))
    }

    /// Check if package is installed
    pub fn is_installed(&self, name: &str) -> Result<bool> {
        Ok(self.get_installed(name)?.is_some())
    }

    /// Record package installation
    pub fn record_install(&self, gen_path: &Path, pkg: &InstalledPackage) -> Result<()> {
        let mut packages = self.load_generation_packages(gen_path)
            .unwrap_or_default();

        // Remove existing entry
        packages.retain(|p| p.name != pkg.name);
        packages.push(pkg.clone());

        let db_file = gen_path.join("packages.json");
        let content = serde_json::to_string_pretty(&packages)?;
        std::fs::write(&db_file, &content)?;

        Ok(())
    }

    /// Record package removal
    pub fn record_remove(&self, gen_path: &Path, name: &str) -> Result<()> {
        let mut packages = self.load_generation_packages(gen_path)
            .unwrap_or_default();

        packages.retain(|p| p.name != name);

        let db_file = gen_path.join("packages.json");
        let content = serde_json::to_string_pretty(&packages)?;
        std::fs::write(&db_file, &content)?;

        Ok(())
    }

    /// Find package that owns a file
    pub fn find_owner(&self, path: &str) -> Result<Option<String>> {
        let packages = self.list_installed()?;

        for pkg in packages {
            for file in &pkg.files {
                if file == path || path.ends_with(file) {
                    return Ok(Some(pkg.name.clone()));
                }
            }
        }

        Ok(None)
    }

    /// Verify package integrity
    pub fn verify(&self, pkg: &InstalledPackage) -> Result<bool> {
        let store_path = PathBuf::from(&pkg.store_path);

        for (file, expected_hash) in &pkg.file_hashes {
            let file_path = store_path.join(file);

            if !file_path.exists() {
                return Ok(false);
            }

            let actual_hash = hash_file(&file_path)?;
            if &actual_hash != expected_hash {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Find available upgrades
    pub async fn find_upgrades(
        &self,
        repos: &RepositoryManager,
    ) -> Result<Vec<(InstalledPackage, RepoPackage)>> {
        let mut upgrades = Vec::new();

        for pkg in self.list_installed()? {
            if let Some(repo_pkg) = repos.get_package(&pkg.name).await? {
                if repo_pkg.version > pkg.version {
                    upgrades.push((pkg, repo_pkg));
                }
            }
        }

        Ok(upgrades)
    }

    /// Find upgrades for specific packages
    pub async fn find_upgrades_for(
        &self,
        repos: &RepositoryManager,
        names: &[String],
    ) -> Result<Vec<(InstalledPackage, RepoPackage)>> {
        let mut upgrades = Vec::new();

        for name in names {
            if let Some(pkg) = self.get_installed(name)? {
                if let Some(repo_pkg) = repos.get_package(name).await? {
                    if repo_pkg.version > pkg.version {
                        upgrades.push((pkg, repo_pkg));
                    }
                }
            }
        }

        Ok(upgrades)
    }

    /// Get all store paths referenced by any generation
    pub fn all_store_paths(&self) -> Result<Vec<String>> {
        let mut paths = Vec::new();

        for gen in self.list_generations()? {
            let gen_path = self.generation_path(gen.number);
            let packages = self.load_generation_packages(&gen_path)?;

            for pkg in packages {
                paths.push(pkg.store_path);
            }
        }

        Ok(paths)
    }
}
