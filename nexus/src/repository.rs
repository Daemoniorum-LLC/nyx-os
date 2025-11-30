//! Repository management

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{info, warn, debug};

use crate::package::{RepoPackage, PackageSpec};

/// Repository configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoConfig {
    pub name: String,
    pub url: String,
    pub enabled: bool,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub gpg_key: Option<String>,
}

/// Repository manager
pub struct RepositoryManager {
    config_dir: PathBuf,
    repos: Vec<Repository>,
    cache_dir: PathBuf,
}

/// Single repository
pub struct Repository {
    pub config: RepoConfig,
    pub packages: HashMap<String, Vec<RepoPackage>>,
}

/// Search result
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub name: String,
    pub version: semver::Version,
    pub description: String,
    pub repo: String,
    pub installed: bool,
}

impl RepositoryManager {
    pub fn load(config_dir: &str) -> Result<Self> {
        let config_dir = PathBuf::from(config_dir);
        let cache_dir = PathBuf::from("/var/cache/nexus/repos");

        std::fs::create_dir_all(&cache_dir)?;

        let mut repos = Vec::new();

        if config_dir.exists() {
            for entry in std::fs::read_dir(&config_dir)? {
                let entry = entry?;
                let path = entry.path();

                if path.extension().and_then(|e| e.to_str()) == Some("repo") {
                    match Self::load_repo_config(&path) {
                        Ok(config) => {
                            if config.enabled {
                                let packages = Self::load_repo_cache(&cache_dir, &config.name)
                                    .unwrap_or_default();

                                repos.push(Repository { config, packages });
                            }
                        }
                        Err(e) => warn!("Failed to load repo config {:?}: {}", path, e),
                    }
                }
            }
        }

        // Sort by priority
        repos.sort_by_key(|r| -r.config.priority);

        Ok(Self {
            config_dir,
            repos,
            cache_dir,
        })
    }

    fn load_repo_config(path: &Path) -> Result<RepoConfig> {
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }

    fn load_repo_cache(cache_dir: &Path, name: &str) -> Result<HashMap<String, Vec<RepoPackage>>> {
        let cache_file = cache_dir.join(format!("{}.json", name));

        if !cache_file.exists() {
            return Ok(HashMap::new());
        }

        let content = std::fs::read_to_string(&cache_file)?;
        let packages: Vec<RepoPackage> = serde_json::from_str(&content)?;

        let mut map: HashMap<String, Vec<RepoPackage>> = HashMap::new();
        for pkg in packages {
            map.entry(pkg.name.clone())
                .or_default()
                .push(pkg);
        }

        // Sort versions descending
        for versions in map.values_mut() {
            versions.sort_by(|a, b| b.version.cmp(&a.version));
        }

        Ok(map)
    }

    /// Synchronize all repositories
    pub async fn sync_all(&mut self) -> Result<()> {
        for repo in &mut self.repos {
            info!("Syncing repository: {}", repo.config.name);

            match Self::sync_repo(&repo.config, &self.cache_dir).await {
                Ok(packages) => {
                    repo.packages = packages;
                    info!("  {} packages", repo.packages.len());
                }
                Err(e) => {
                    warn!("Failed to sync {}: {}", repo.config.name, e);
                }
            }
        }

        Ok(())
    }

    async fn sync_repo(
        config: &RepoConfig,
        cache_dir: &Path,
    ) -> Result<HashMap<String, Vec<RepoPackage>>> {
        let index_url = format!("{}/index.json", config.url);

        let client = reqwest::Client::new();
        let response = client.get(&index_url).send().await?;

        if !response.status().is_success() {
            return Err(anyhow!("Failed to fetch index: {}", response.status()));
        }

        let packages: Vec<RepoPackage> = response.json().await?;

        // Save to cache
        let cache_file = cache_dir.join(format!("{}.json", config.name));
        let content = serde_json::to_string(&packages)?;
        std::fs::write(&cache_file, &content)?;

        // Build map
        let mut map: HashMap<String, Vec<RepoPackage>> = HashMap::new();
        for pkg in packages {
            map.entry(pkg.name.clone())
                .or_default()
                .push(pkg);
        }

        for versions in map.values_mut() {
            versions.sort_by(|a, b| b.version.cmp(&a.version));
        }

        Ok(map)
    }

    /// Search for packages
    pub async fn search(&self, query: &str) -> Result<Vec<SearchResult>> {
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        for repo in &self.repos {
            for (name, versions) in &repo.packages {
                if let Some(pkg) = versions.first() {
                    if name.to_lowercase().contains(&query_lower)
                        || pkg.description.to_lowercase().contains(&query_lower)
                    {
                        results.push(SearchResult {
                            name: name.clone(),
                            version: pkg.version.clone(),
                            description: pkg.description.clone(),
                            repo: repo.config.name.clone(),
                            installed: false, // Caller should check
                        });
                    }
                }
            }
        }

        // Sort by relevance (exact name match first)
        results.sort_by(|a, b| {
            let a_exact = a.name.to_lowercase() == query_lower;
            let b_exact = b.name.to_lowercase() == query_lower;
            b_exact.cmp(&a_exact).then_with(|| a.name.cmp(&b.name))
        });

        Ok(results)
    }

    /// Get a specific package
    pub async fn get_package(&self, name: &str) -> Result<Option<RepoPackage>> {
        for repo in &self.repos {
            if let Some(versions) = repo.packages.get(name) {
                if let Some(pkg) = versions.first() {
                    return Ok(Some(pkg.clone()));
                }
            }
        }

        Ok(None)
    }

    /// Get package matching spec
    pub fn get_matching(&self, spec: &PackageSpec) -> Option<RepoPackage> {
        for repo in &self.repos {
            if let Some(versions) = repo.packages.get(&spec.name) {
                for pkg in versions {
                    if let Some(ref req) = spec.version_req {
                        if req.matches(&pkg.version) {
                            return Some(pkg.clone());
                        }
                    } else {
                        return Some(pkg.clone());
                    }
                }
            }
        }

        None
    }

    /// Download a package
    pub async fn download(&self, pkg: &RepoPackage, dest: &Path) -> Result<PathBuf> {
        let client = reqwest::Client::new();

        info!("Downloading {} {}", pkg.name, pkg.version);

        let response = client.get(&pkg.url).send().await?;

        if !response.status().is_success() {
            return Err(anyhow!("Download failed: {}", response.status()));
        }

        let filename = format!("{}-{}.nyx", pkg.name, pkg.version);
        let dest_path = dest.join(&filename);

        let bytes = response.bytes().await?;

        // Verify hash
        let hash = crate::package::hash_data(&bytes);
        if hash != pkg.sha256 {
            return Err(anyhow!(
                "Hash mismatch: expected {}, got {}",
                pkg.sha256,
                hash
            ));
        }

        std::fs::write(&dest_path, &bytes)?;
        debug!("Downloaded to {:?}", dest_path);

        Ok(dest_path)
    }

    /// Get all package names
    pub fn all_packages(&self) -> Vec<String> {
        let mut names: Vec<String> = self.repos.iter()
            .flat_map(|r| r.packages.keys().cloned())
            .collect();

        names.sort();
        names.dedup();
        names
    }
}
