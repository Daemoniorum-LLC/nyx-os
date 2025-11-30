//! Nexus - Nyx Package Manager
//!
//! A modern package manager with content-addressable storage,
//! atomic transactions, and reproducible builds.

mod package;
mod repository;
mod resolver;
mod transaction;
mod store;
mod cache;
mod sandbox;
mod ipc;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::{info, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::ipc::NexusClient;
use crate::package::PackageSpec;
use crate::repository::RepositoryManager;
use crate::store::PackageStore;

#[derive(Parser)]
#[command(name = "nexus")]
#[command(about = "Nyx Package Manager", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Use system root
    #[arg(long, default_value = "/")]
    root: String,

    /// Configuration file
    #[arg(long, default_value = "/etc/nexus/nexus.toml")]
    config: String,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Install packages
    Install {
        /// Package names or specs
        packages: Vec<String>,

        /// Don't actually install, just show what would happen
        #[arg(long)]
        dry_run: bool,
    },

    /// Remove packages
    Remove {
        /// Package names
        packages: Vec<String>,

        /// Also remove unused dependencies
        #[arg(long)]
        autoremove: bool,
    },

    /// Upgrade packages
    Upgrade {
        /// Specific packages to upgrade (all if empty)
        packages: Vec<String>,
    },

    /// Search for packages
    Search {
        /// Search query
        query: String,
    },

    /// Show package information
    Info {
        /// Package name
        package: String,
    },

    /// List installed packages
    List {
        /// Show only explicitly installed
        #[arg(long)]
        explicit: bool,
    },

    /// Synchronize repository metadata
    Sync,

    /// Clean package cache
    Clean {
        /// Remove all cached packages
        #[arg(long)]
        all: bool,
    },

    /// Build a package from source
    Build {
        /// Path to package definition
        path: String,

        /// Output directory
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Verify package integrity
    Verify {
        /// Package name (all if empty)
        package: Option<String>,
    },

    /// Rollback to previous state
    Rollback {
        /// Generation number
        generation: Option<u32>,
    },

    /// List system generations
    Generations,

    /// Query package database
    Query {
        /// File to find owner of
        #[arg(long)]
        owns: Option<String>,

        /// List files in package
        #[arg(long)]
        files: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let filter = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::new(filter))
        .init();

    // Try to connect to daemon first
    let client = NexusClient::connect().await.ok();

    match cli.command {
        Commands::Install { packages, dry_run } => {
            install_packages(&packages, dry_run, client.as_ref()).await?;
        }

        Commands::Remove { packages, autoremove } => {
            remove_packages(&packages, autoremove, client.as_ref()).await?;
        }

        Commands::Upgrade { packages } => {
            upgrade_packages(&packages, client.as_ref()).await?;
        }

        Commands::Search { query } => {
            search_packages(&query).await?;
        }

        Commands::Info { package } => {
            show_package_info(&package).await?;
        }

        Commands::List { explicit } => {
            list_packages(explicit).await?;
        }

        Commands::Sync => {
            sync_repositories(client.as_ref()).await?;
        }

        Commands::Clean { all } => {
            clean_cache(all).await?;
        }

        Commands::Build { path, output } => {
            build_package(&path, output.as_deref()).await?;
        }

        Commands::Verify { package } => {
            verify_packages(package.as_deref()).await?;
        }

        Commands::Rollback { generation } => {
            rollback(generation, client.as_ref()).await?;
        }

        Commands::Generations => {
            list_generations().await?;
        }

        Commands::Query { owns, files } => {
            query_packages(owns.as_deref(), files.as_deref()).await?;
        }
    }

    Ok(())
}

async fn install_packages(
    packages: &[String],
    dry_run: bool,
    client: Option<&NexusClient>,
) -> Result<()> {
    info!("Installing packages: {:?}", packages);

    let specs: Vec<PackageSpec> = packages.iter()
        .map(|s| s.parse())
        .collect::<Result<Vec<_>, _>>()?;

    if let Some(client) = client {
        // Use daemon for installation
        client.install(&specs, dry_run).await?;
    } else {
        // Direct installation (requires root)
        let store = PackageStore::open("/nyx/store")?;
        let repos = RepositoryManager::load("/etc/nexus/repos.d")?;

        // Resolve dependencies
        let resolver = resolver::DependencyResolver::new(&store, &repos);
        let plan = resolver.resolve(&specs).await?;

        if dry_run {
            println!("Would install {} packages:", plan.to_install.len());
            for pkg in &plan.to_install {
                println!("  {} {}", pkg.name, pkg.version);
            }
            return Ok(());
        }

        // Execute transaction
        let mut tx = transaction::Transaction::new(&store);
        for pkg in plan.to_install {
            tx.add_install(pkg);
        }
        tx.commit().await?;
    }

    info!("Installation complete");
    Ok(())
}

async fn remove_packages(
    packages: &[String],
    autoremove: bool,
    client: Option<&NexusClient>,
) -> Result<()> {
    info!("Removing packages: {:?}", packages);

    if let Some(client) = client {
        client.remove(packages, autoremove).await?;
    } else {
        let store = PackageStore::open("/nyx/store")?;

        let mut tx = transaction::Transaction::new(&store);
        for name in packages {
            tx.add_remove(name);
        }

        if autoremove {
            tx.add_autoremove();
        }

        tx.commit().await?;
    }

    info!("Removal complete");
    Ok(())
}

async fn upgrade_packages(
    packages: &[String],
    client: Option<&NexusClient>,
) -> Result<()> {
    info!("Upgrading packages");

    if let Some(client) = client {
        client.upgrade(packages).await?;
    } else {
        let store = PackageStore::open("/nyx/store")?;
        let repos = RepositoryManager::load("/etc/nexus/repos.d")?;

        let upgrades = if packages.is_empty() {
            store.find_upgrades(&repos).await?
        } else {
            store.find_upgrades_for(&repos, packages).await?
        };

        if upgrades.is_empty() {
            println!("All packages are up to date");
            return Ok(());
        }

        println!("Packages to upgrade:");
        for (old, new) in &upgrades {
            println!("  {} {} -> {}", old.name, old.version, new.version);
        }

        let mut tx = transaction::Transaction::new(&store);
        for (_, new) in upgrades {
            tx.add_install(new);
        }
        tx.commit().await?;
    }

    Ok(())
}

async fn search_packages(query: &str) -> Result<()> {
    let repos = RepositoryManager::load("/etc/nexus/repos.d")?;

    let results = repos.search(query).await?;

    if results.is_empty() {
        println!("No packages found matching '{}'", query);
        return Ok(());
    }

    for pkg in results {
        let installed = if pkg.installed { " [installed]" } else { "" };
        println!("{} {} - {}{}", pkg.name, pkg.version, pkg.description, installed);
    }

    Ok(())
}

async fn show_package_info(name: &str) -> Result<()> {
    let repos = RepositoryManager::load("/etc/nexus/repos.d")?;
    let store = PackageStore::open("/nyx/store")?;

    // Check installed first
    if let Some(pkg) = store.get_installed(name)? {
        println!("Name:         {}", pkg.name);
        println!("Version:      {}", pkg.version);
        println!("Description:  {}", pkg.description);
        println!("License:      {}", pkg.license);
        println!("Size:         {} bytes", pkg.installed_size);
        println!("Dependencies: {}", pkg.dependencies.join(", "));
        println!("Status:       Installed");
        println!("Store Path:   {}", pkg.store_path);
        return Ok(());
    }

    // Check repositories
    if let Some(pkg) = repos.get_package(name).await? {
        println!("Name:         {}", pkg.name);
        println!("Version:      {}", pkg.version);
        println!("Description:  {}", pkg.description);
        println!("License:      {}", pkg.license);
        println!("Size:         {} bytes (download)", pkg.download_size);
        println!("Dependencies: {}", pkg.dependencies.join(", "));
        println!("Status:       Not installed");
        return Ok(());
    }

    println!("Package '{}' not found", name);
    Ok(())
}

async fn list_packages(explicit: bool) -> Result<()> {
    let store = PackageStore::open("/nyx/store")?;

    let packages = if explicit {
        store.list_explicit()?
    } else {
        store.list_installed()?
    };

    for pkg in packages {
        println!("{} {}", pkg.name, pkg.version);
    }

    Ok(())
}

async fn sync_repositories(client: Option<&NexusClient>) -> Result<()> {
    info!("Synchronizing repositories");

    if let Some(client) = client {
        client.sync().await?;
    } else {
        let mut repos = RepositoryManager::load("/etc/nexus/repos.d")?;
        repos.sync_all().await?;
    }

    println!("Repository sync complete");
    Ok(())
}

async fn clean_cache(all: bool) -> Result<()> {
    let cache = cache::PackageCache::open("/var/cache/nexus")?;

    if all {
        let freed = cache.clean_all()?;
        println!("Removed all cached packages, freed {} bytes", freed);
    } else {
        let freed = cache.clean_old()?;
        println!("Removed old cached packages, freed {} bytes", freed);
    }

    Ok(())
}

async fn build_package(path: &str, output: Option<&str>) -> Result<()> {
    info!("Building package from {}", path);

    let sandbox = sandbox::BuildSandbox::new()?;
    let pkg = sandbox.build(path).await?;

    let output_path = output.unwrap_or(".");
    let archive_path = format!("{}/{}-{}.nyx", output_path, pkg.name, pkg.version);

    pkg.write_archive(&archive_path)?;
    println!("Built package: {}", archive_path);

    Ok(())
}

async fn verify_packages(package: Option<&str>) -> Result<()> {
    let store = PackageStore::open("/nyx/store")?;

    let packages = if let Some(name) = package {
        vec![store.get_installed(name)?
            .ok_or_else(|| anyhow::anyhow!("Package not found: {}", name))?]
    } else {
        store.list_installed()?
    };

    let mut errors = Vec::new();

    for pkg in packages {
        match store.verify(&pkg) {
            Ok(true) => println!("{}: OK", pkg.name),
            Ok(false) => {
                println!("{}: MODIFIED", pkg.name);
                errors.push(pkg.name.clone());
            }
            Err(e) => {
                println!("{}: ERROR ({})", pkg.name, e);
                errors.push(pkg.name.clone());
            }
        }
    }

    if !errors.is_empty() {
        println!("\n{} packages have issues", errors.len());
    }

    Ok(())
}

async fn rollback(generation: Option<u32>, client: Option<&NexusClient>) -> Result<()> {
    if let Some(client) = client {
        client.rollback(generation).await?;
    } else {
        let store = PackageStore::open("/nyx/store")?;

        let gen = generation.unwrap_or_else(|| store.current_generation().saturating_sub(1));
        store.activate_generation(gen)?;
        println!("Rolled back to generation {}", gen);
    }

    Ok(())
}

async fn list_generations() -> Result<()> {
    let store = PackageStore::open("/nyx/store")?;
    let current = store.current_generation();

    for gen in store.list_generations()? {
        let marker = if gen.number == current { " *" } else { "" };
        println!(
            "{}: {} ({} packages){}",
            gen.number,
            gen.timestamp.format("%Y-%m-%d %H:%M:%S"),
            gen.package_count,
            marker
        );
    }

    Ok(())
}

async fn query_packages(owns: Option<&str>, files: Option<&str>) -> Result<()> {
    let store = PackageStore::open("/nyx/store")?;

    if let Some(path) = owns {
        if let Some(pkg) = store.find_owner(path)? {
            println!("{} is owned by {}", path, pkg);
        } else {
            println!("{} is not owned by any package", path);
        }
    }

    if let Some(name) = files {
        let pkg = store.get_installed(name)?
            .ok_or_else(|| anyhow::anyhow!("Package not found: {}", name))?;

        for file in &pkg.files {
            println!("{}", file);
        }
    }

    Ok(())
}
