//! Package format and metadata

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;

/// Package specification (name with optional version constraint)
#[derive(Debug, Clone)]
pub struct PackageSpec {
    pub name: String,
    pub version_req: Option<VersionReq>,
}

impl FromStr for PackageSpec {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        if let Some((name, version)) = s.split_once('@') {
            Ok(Self {
                name: name.to_string(),
                version_req: Some(version.parse()?),
            })
        } else if let Some((name, version)) = s.split_once(">=") {
            Ok(Self {
                name: name.to_string(),
                version_req: Some(VersionReq::GreaterOrEqual(version.parse()?)),
            })
        } else if let Some((name, version)) = s.split_once("<=") {
            Ok(Self {
                name: name.to_string(),
                version_req: Some(VersionReq::LessOrEqual(version.parse()?)),
            })
        } else {
            Ok(Self {
                name: s.to_string(),
                version_req: None,
            })
        }
    }
}

/// Version requirement
#[derive(Debug, Clone)]
pub enum VersionReq {
    Exact(semver::Version),
    GreaterOrEqual(semver::Version),
    LessOrEqual(semver::Version),
    Range(semver::Version, semver::Version),
    Any,
}

impl FromStr for VersionReq {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        if s == "*" {
            Ok(VersionReq::Any)
        } else {
            Ok(VersionReq::Exact(s.parse()?))
        }
    }
}

impl VersionReq {
    pub fn matches(&self, version: &semver::Version) -> bool {
        match self {
            VersionReq::Exact(v) => version == v,
            VersionReq::GreaterOrEqual(v) => version >= v,
            VersionReq::LessOrEqual(v) => version <= v,
            VersionReq::Range(min, max) => version >= min && version <= max,
            VersionReq::Any => true,
        }
    }
}

/// Package metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageMetadata {
    pub name: String,
    pub version: semver::Version,
    pub description: String,
    pub license: String,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    pub maintainers: Vec<String>,
    pub dependencies: Vec<String>,
    pub build_dependencies: Vec<String>,
    pub optional_dependencies: HashMap<String, Vec<String>>,
    pub provides: Vec<String>,
    pub conflicts: Vec<String>,
    pub replaces: Vec<String>,
}

impl PackageMetadata {
    pub fn from_toml(content: &str) -> Result<Self> {
        Ok(toml::from_str(content)?)
    }

    pub fn to_toml(&self) -> Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }
}

/// Installed package record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPackage {
    pub name: String,
    pub version: semver::Version,
    pub description: String,
    pub license: String,
    pub dependencies: Vec<String>,
    pub store_path: String,
    pub installed_size: u64,
    pub install_time: chrono::DateTime<chrono::Utc>,
    pub files: Vec<String>,
    pub file_hashes: HashMap<String, String>,
    pub explicit: bool,
}

/// Package in repository
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoPackage {
    pub name: String,
    pub version: semver::Version,
    pub description: String,
    pub license: String,
    pub dependencies: Vec<String>,
    pub download_size: u64,
    pub installed_size: u64,
    pub sha256: String,
    pub url: String,
    #[serde(default)]
    pub installed: bool,
}

/// Built package ready for installation
#[derive(Debug, Clone)]
pub struct BuiltPackage {
    pub name: String,
    pub version: semver::Version,
    pub description: String,
    pub license: String,
    pub dependencies: Vec<String>,
    pub files: Vec<PackageFile>,
    pub store_hash: String,
}

#[derive(Debug, Clone)]
pub struct PackageFile {
    pub path: String,
    pub hash: String,
    pub size: u64,
    pub mode: u32,
}

impl BuiltPackage {
    /// Calculate content-addressable store path
    pub fn store_path(&self) -> String {
        format!("/nyx/store/{}-{}-{}", self.store_hash, self.name, self.version)
    }

    /// Write package as archive
    pub fn write_archive(&self, path: &str) -> Result<()> {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::fs::File;
        use tar::Builder;

        let file = File::create(path)?;
        let enc = GzEncoder::new(file, Compression::default());
        let mut ar = Builder::new(enc);

        // Add metadata
        let metadata = PackageMetadata {
            name: self.name.clone(),
            version: self.version.clone(),
            description: self.description.clone(),
            license: self.license.clone(),
            homepage: None,
            repository: None,
            maintainers: vec![],
            dependencies: self.dependencies.clone(),
            build_dependencies: vec![],
            optional_dependencies: HashMap::new(),
            provides: vec![],
            conflicts: vec![],
            replaces: vec![],
        };

        let meta_toml = metadata.to_toml()?;
        let mut header = tar::Header::new_gnu();
        header.set_size(meta_toml.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        ar.append_data(&mut header, "META.toml", meta_toml.as_bytes())?;

        ar.finish()?;
        Ok(())
    }
}

/// Calculate hash of file contents
pub fn hash_file(path: &Path) -> Result<String> {
    let mut hasher = Sha256::new();
    let mut file = std::fs::File::open(path)?;
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}

/// Calculate hash of data
pub fn hash_data(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// Package definition for building
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageDefinition {
    pub package: PackageMetadata,
    pub source: SourceDefinition,
    pub build: BuildDefinition,
    pub install: InstallDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceDefinition {
    pub url: Option<String>,
    pub git: Option<String>,
    pub sha256: Option<String>,
    pub patches: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildDefinition {
    pub system: Option<String>,  // cmake, meson, autotools, cargo, etc.
    pub configure: Vec<String>,
    pub build: Vec<String>,
    pub check: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallDefinition {
    pub commands: Vec<String>,
    pub files: HashMap<String, String>,
}

impl PackageDefinition {
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }
}
