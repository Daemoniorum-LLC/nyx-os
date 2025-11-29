//! # Guardian
//!
//! AI-powered security agent for DaemonOS.
//!
//! ## Philosophy
//!
//! Traditional security is based on static permissions:
//! "Does user X have permission Y on resource Z?"
//!
//! Guardian adds **intent-based security**:
//! "Given this context, does this action make sense?"
//!
//! ## Features
//!
//! - **Capability Approval**: Evaluate requests for kernel capabilities
//! - **Intent Analysis**: Use AI to understand what an app is trying to do
//! - **Pattern Learning**: Learn normal behavior, detect anomalies
//! - **Sandboxing**: Configure and enforce sandboxes
//! - **Audit Logging**: Comprehensive security audit trail
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                        GUARDIAN                              │
//! ├─────────────────────────────────────────────────────────────┤
//! │                                                              │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐         │
//! │  │   Policy    │  │   Intent    │  │   Pattern   │         │
//! │  │   Engine    │  │  Analyzer   │  │   Learner   │         │
//! │  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘         │
//! │         │                │                │                 │
//! │  ┌──────┴────────────────┴────────────────┴──────┐         │
//! │  │              Decision Engine                   │         │
//! │  └──────────────────────┬────────────────────────┘         │
//! │                         │                                   │
//! │  ┌──────────────────────┴────────────────────────┐         │
//! │  │              Audit Logger                      │         │
//! │  └───────────────────────────────────────────────┘         │
//! │                                                              │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//!               ┌──────────────────────────┐
//!               │     IPC Socket           │
//!               │  /run/guardian/guardian  │
//!               └──────────────────────────┘
//! ```

mod policy;
mod intent;
mod pattern;
mod decision;
mod audit;
mod sandbox;
mod ipc;
mod config;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, error};

/// Guardian - AI-powered security agent
#[derive(Parser, Debug)]
#[command(name = "guardian", version, about)]
struct Args {
    /// Configuration file
    #[arg(short, long, default_value = "/grimoire/system/guardian.yaml")]
    config: PathBuf,

    /// Socket path
    #[arg(short, long, default_value = "/run/guardian/guardian.sock")]
    socket: PathBuf,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,

    /// Permissive mode (log but don't deny)
    #[arg(long)]
    permissive: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    let log_level = if args.debug { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(log_level)
        .init();

    info!("Guardian v{} starting", env!("CARGO_PKG_VERSION"));

    if args.permissive {
        info!("Running in PERMISSIVE mode - will log but not deny");
    }

    // Load configuration
    let config = config::load_config(&args.config).await?;

    // Initialize components
    let policy_engine = Arc::new(policy::PolicyEngine::new(&config.policies)?);
    let intent_analyzer = Arc::new(intent::IntentAnalyzer::new(&config.intent)?);
    let pattern_learner = Arc::new(pattern::PatternLearner::new(&config.patterns)?);
    let audit_logger = Arc::new(audit::AuditLogger::new(&config.audit)?);

    // Create decision engine
    let decision_engine = Arc::new(decision::DecisionEngine::new(
        policy_engine.clone(),
        intent_analyzer.clone(),
        pattern_learner.clone(),
        audit_logger.clone(),
        args.permissive,
    ));

    // Start IPC server
    let server = ipc::GuardianServer::new(
        args.socket,
        decision_engine.clone(),
        audit_logger.clone(),
    );

    info!("Guardian ready");

    // Run server
    server.run().await
}
