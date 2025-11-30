//! Dependency resolution

use anyhow::{Result, anyhow};
use std::collections::{HashMap, HashSet, VecDeque};
use tracing::{debug, warn};

use crate::package::{PackageSpec, RepoPackage};
use crate::repository::RepositoryManager;
use crate::store::PackageStore;

/// Dependency resolver
pub struct DependencyResolver<'a> {
    store: &'a PackageStore,
    repos: &'a RepositoryManager,
}

/// Resolution plan
#[derive(Debug, Clone)]
pub struct ResolutionPlan {
    pub to_install: Vec<RepoPackage>,
    pub to_remove: Vec<String>,
    pub download_size: u64,
    pub install_size: u64,
}

/// Resolution state
struct ResolverState {
    /// Packages selected for installation
    selected: HashMap<String, RepoPackage>,
    /// Already installed packages
    installed: HashSet<String>,
    /// Packages being processed (cycle detection)
    processing: HashSet<String>,
    /// Resolution errors
    errors: Vec<String>,
}

impl<'a> DependencyResolver<'a> {
    pub fn new(store: &'a PackageStore, repos: &'a RepositoryManager) -> Self {
        Self { store, repos }
    }

    /// Resolve dependencies for requested packages
    pub async fn resolve(&self, specs: &[PackageSpec]) -> Result<ResolutionPlan> {
        let installed: HashSet<String> = self.store
            .list_installed()?
            .into_iter()
            .map(|p| p.name)
            .collect();

        let mut state = ResolverState {
            selected: HashMap::new(),
            installed,
            processing: HashSet::new(),
            errors: Vec::new(),
        };

        // Resolve each requested package
        for spec in specs {
            self.resolve_package(spec, &mut state)?;
        }

        if !state.errors.is_empty() {
            return Err(anyhow!(
                "Resolution failed:\n  {}",
                state.errors.join("\n  ")
            ));
        }

        // Build installation order (topological sort)
        let ordered = self.topological_sort(&state.selected)?;

        let download_size: u64 = ordered.iter().map(|p| p.download_size).sum();
        let install_size: u64 = ordered.iter().map(|p| p.installed_size).sum();

        Ok(ResolutionPlan {
            to_install: ordered,
            to_remove: vec![],
            download_size,
            install_size,
        })
    }

    fn resolve_package(
        &self,
        spec: &PackageSpec,
        state: &mut ResolverState,
    ) -> Result<()> {
        // Skip if already selected or installed
        if state.selected.contains_key(&spec.name) {
            return Ok(());
        }

        if state.installed.contains(&spec.name) {
            // Check if version satisfies requirement
            if spec.version_req.is_none() {
                return Ok(());
            }
            // TODO: Check version constraint
        }

        // Cycle detection
        if state.processing.contains(&spec.name) {
            state.errors.push(format!(
                "Circular dependency detected: {}",
                spec.name
            ));
            return Ok(());
        }

        state.processing.insert(spec.name.clone());

        // Find package in repositories
        let pkg = self.repos.get_matching(spec)
            .ok_or_else(|| anyhow!("Package not found: {}", spec.name))?;

        debug!("Selected {} {}", pkg.name, pkg.version);

        // Resolve dependencies
        for dep_str in &pkg.dependencies {
            let dep_spec: PackageSpec = dep_str.parse()?;
            self.resolve_package(&dep_spec, state)?;
        }

        state.processing.remove(&spec.name);
        state.selected.insert(spec.name.clone(), pkg);

        Ok(())
    }

    fn topological_sort(
        &self,
        packages: &HashMap<String, RepoPackage>,
    ) -> Result<Vec<RepoPackage>> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut temp_visited = HashSet::new();

        fn visit(
            name: &str,
            packages: &HashMap<String, RepoPackage>,
            visited: &mut HashSet<String>,
            temp_visited: &mut HashSet<String>,
            result: &mut Vec<RepoPackage>,
        ) -> Result<()> {
            if visited.contains(name) {
                return Ok(());
            }

            if temp_visited.contains(name) {
                return Err(anyhow!("Circular dependency: {}", name));
            }

            temp_visited.insert(name.to_string());

            if let Some(pkg) = packages.get(name) {
                for dep in &pkg.dependencies {
                    let dep_name = dep.split(['@', '>', '<', '=']).next().unwrap();
                    if packages.contains_key(dep_name) {
                        visit(dep_name, packages, visited, temp_visited, result)?;
                    }
                }

                temp_visited.remove(name);
                visited.insert(name.to_string());
                result.push(pkg.clone());
            }

            Ok(())
        }

        for name in packages.keys() {
            visit(name, packages, &mut visited, &mut temp_visited, &mut result)?;
        }

        Ok(result)
    }

    /// Find packages that can be autoremoved
    pub fn find_orphans(&self) -> Result<Vec<String>> {
        let installed = self.store.list_installed()?;
        let explicit: HashSet<String> = installed.iter()
            .filter(|p| p.explicit)
            .map(|p| p.name.clone())
            .collect();

        // Build reverse dependency map
        let mut rdeps: HashMap<String, HashSet<String>> = HashMap::new();
        for pkg in &installed {
            for dep in &pkg.dependencies {
                let dep_name = dep.split(['@', '>', '<', '=']).next().unwrap();
                rdeps.entry(dep_name.to_string())
                    .or_default()
                    .insert(pkg.name.clone());
            }
        }

        // Find packages not required by anything explicit
        let mut orphans = Vec::new();
        let mut queue: VecDeque<String> = installed.iter()
            .filter(|p| !p.explicit)
            .map(|p| p.name.clone())
            .collect();

        let mut checked = HashSet::new();

        while let Some(name) = queue.pop_front() {
            if checked.contains(&name) {
                continue;
            }
            checked.insert(name.clone());

            let dependents = rdeps.get(&name).cloned().unwrap_or_default();

            // Check if any dependent is explicit or has other dependents
            let needed = dependents.iter().any(|d| {
                explicit.contains(d) || !orphans.contains(d)
            });

            if !needed && !explicit.contains(&name) {
                orphans.push(name);
            }
        }

        Ok(orphans)
    }
}

/// Check if package A conflicts with package B
pub fn check_conflict(a: &RepoPackage, b: &RepoPackage) -> Option<String> {
    // Same package, different versions
    if a.name == b.name && a.version != b.version {
        return Some(format!(
            "{} {} conflicts with {} {}",
            a.name, a.version, b.name, b.version
        ));
    }

    // TODO: Check conflicts field

    None
}
