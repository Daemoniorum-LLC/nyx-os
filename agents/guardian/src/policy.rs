//! Policy engine - evaluates static policies

use crate::config::{CapabilityRule, DefaultPolicy, PolicyConfig, RuleAction, RuleCondition, TrustedApp};
use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;
use tracing::debug;

/// Capability request to evaluate
#[derive(Debug, Clone)]
pub struct CapabilityRequest {
    /// Requesting process ID
    pub pid: u32,
    /// Requesting process path
    pub process_path: String,
    /// Requesting user
    pub user: String,
    /// Capability being requested
    pub capability: String,
    /// Target resource (if applicable)
    pub resource: Option<String>,
    /// Additional context
    pub context: HashMap<String, String>,
}

/// Policy evaluation result
#[derive(Debug, Clone)]
pub struct PolicyResult {
    /// Decision
    pub decision: PolicyDecision,
    /// Matched rule (if any)
    pub matched_rule: Option<String>,
    /// Reason for decision
    pub reason: String,
    /// Suggested sandbox profile
    pub sandbox_profile: Option<String>,
}

/// Policy decision
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyDecision {
    /// Allow the request
    Allow,
    /// Deny the request
    Deny,
    /// Prompt the user
    Prompt,
    /// Need intent analysis
    NeedIntentAnalysis,
    /// Apply sandbox
    Sandbox,
}

/// Policy engine
pub struct PolicyEngine {
    /// Default policy
    default_policy: DefaultPolicy,
    /// Trusted applications (compiled patterns)
    trusted_apps: Vec<CompiledTrustedApp>,
    /// Capability rules (compiled)
    capability_rules: Vec<CompiledRule>,
}

struct CompiledTrustedApp {
    name: String,
    path_pattern: Regex,
    capabilities: Vec<Regex>,
}

struct CompiledRule {
    name: String,
    capability_pattern: Regex,
    conditions: Vec<CompiledCondition>,
    action: RuleAction,
}

enum CompiledCondition {
    AppPath(Regex),
    User(String),
    TimeWindow { start: String, end: String },
    ResourcePath(Regex),
    Intent(String),
}

impl PolicyEngine {
    /// Create a new policy engine
    pub fn new(config: &PolicyConfig) -> Result<Self> {
        // Compile trusted app patterns
        let trusted_apps = config
            .trusted_apps
            .iter()
            .filter_map(|app| compile_trusted_app(app).ok())
            .collect();

        // Compile capability rules
        let capability_rules = config
            .capability_rules
            .iter()
            .filter_map(|rule| compile_rule(rule).ok())
            .collect();

        Ok(Self {
            default_policy: config.default_policy,
            trusted_apps,
            capability_rules,
        })
    }

    /// Evaluate a capability request
    pub fn evaluate(&self, request: &CapabilityRequest) -> PolicyResult {
        debug!(
            "Evaluating policy for capability '{}' from '{}'",
            request.capability, request.process_path
        );

        // Check trusted apps first
        if let Some(result) = self.check_trusted_apps(request) {
            return result;
        }

        // Check explicit rules
        if let Some(result) = self.check_rules(request) {
            return result;
        }

        // Fall back to default policy
        self.apply_default_policy(request)
    }

    fn check_trusted_apps(&self, request: &CapabilityRequest) -> Option<PolicyResult> {
        for app in &self.trusted_apps {
            if app.path_pattern.is_match(&request.process_path) {
                // Check if capability is allowed for this trusted app
                for cap_pattern in &app.capabilities {
                    if cap_pattern.is_match(&request.capability) || cap_pattern.as_str() == ".*" {
                        return Some(PolicyResult {
                            decision: PolicyDecision::Allow,
                            matched_rule: Some(format!("trusted_app:{}", app.name)),
                            reason: format!("Trusted application: {}", app.name),
                            sandbox_profile: None,
                        });
                    }
                }
            }
        }
        None
    }

    fn check_rules(&self, request: &CapabilityRequest) -> Option<PolicyResult> {
        for rule in &self.capability_rules {
            if !rule.capability_pattern.is_match(&request.capability) {
                continue;
            }

            // Check all conditions
            let conditions_met = rule.conditions.iter().all(|cond| {
                match cond {
                    CompiledCondition::AppPath(pattern) => {
                        pattern.is_match(&request.process_path)
                    }
                    CompiledCondition::User(user) => {
                        &request.user == user || user == "*"
                    }
                    CompiledCondition::TimeWindow { start, end } => {
                        // TODO: Implement time window check
                        let _ = (start, end);
                        true
                    }
                    CompiledCondition::ResourcePath(pattern) => {
                        request.resource.as_ref()
                            .map(|r| pattern.is_match(r))
                            .unwrap_or(false)
                    }
                    CompiledCondition::Intent(intent) => {
                        request.context.get("intent")
                            .map(|i| i == intent)
                            .unwrap_or(false)
                    }
                }
            });

            if conditions_met {
                let decision = match rule.action {
                    RuleAction::Allow => PolicyDecision::Allow,
                    RuleAction::Deny => PolicyDecision::Deny,
                    RuleAction::Prompt => PolicyDecision::Prompt,
                    RuleAction::AllowOnce => PolicyDecision::Prompt, // Prompt but only allow once
                    RuleAction::DenyWithMessage => PolicyDecision::Deny,
                    RuleAction::Sandbox => PolicyDecision::Sandbox,
                };

                return Some(PolicyResult {
                    decision,
                    matched_rule: Some(rule.name.clone()),
                    reason: format!("Matched rule: {}", rule.name),
                    sandbox_profile: if decision == PolicyDecision::Sandbox {
                        Some("strict".into())
                    } else {
                        None
                    },
                });
            }
        }
        None
    }

    fn apply_default_policy(&self, request: &CapabilityRequest) -> PolicyResult {
        let decision = match self.default_policy {
            DefaultPolicy::Allow => PolicyDecision::Allow,
            DefaultPolicy::Deny => PolicyDecision::Deny,
            DefaultPolicy::Prompt => PolicyDecision::Prompt,
            DefaultPolicy::AllowWithAudit => PolicyDecision::Allow,
        };

        PolicyResult {
            decision,
            matched_rule: None,
            reason: format!("Default policy: {:?}", self.default_policy),
            sandbox_profile: None,
        }
    }
}

fn compile_trusted_app(app: &TrustedApp) -> Result<CompiledTrustedApp> {
    let path_pattern = glob_to_regex(&app.path_pattern)?;
    let capabilities = app
        .capabilities
        .iter()
        .map(|c| glob_to_regex(c))
        .collect::<Result<Vec<_>>>()?;

    Ok(CompiledTrustedApp {
        name: app.name.clone(),
        path_pattern,
        capabilities,
    })
}

fn compile_rule(rule: &CapabilityRule) -> Result<CompiledRule> {
    let capability_pattern = glob_to_regex(&rule.capability)?;

    let conditions = rule
        .conditions
        .iter()
        .map(|cond| match cond {
            RuleCondition::AppPath(pattern) => {
                Ok(CompiledCondition::AppPath(glob_to_regex(pattern)?))
            }
            RuleCondition::User(user) => Ok(CompiledCondition::User(user.clone())),
            RuleCondition::TimeWindow { start, end } => Ok(CompiledCondition::TimeWindow {
                start: start.clone(),
                end: end.clone(),
            }),
            RuleCondition::ResourcePath(pattern) => {
                Ok(CompiledCondition::ResourcePath(glob_to_regex(pattern)?))
            }
            RuleCondition::Intent(intent) => Ok(CompiledCondition::Intent(intent.clone())),
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(CompiledRule {
        name: rule.name.clone(),
        capability_pattern,
        conditions,
        action: rule.action,
    })
}

fn glob_to_regex(pattern: &str) -> Result<Regex> {
    let escaped = regex::escape(pattern);
    let regex_pattern = escaped
        .replace(r"\*", ".*")
        .replace(r"\?", ".");
    Ok(Regex::new(&format!("^{}$", regex_pattern))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trusted_app() {
        let config = PolicyConfig {
            default_policy: DefaultPolicy::Deny,
            trusted_apps: vec![TrustedApp {
                name: "test".into(),
                path_pattern: "/usr/bin/test*".into(),
                capabilities: vec!["*".into()],
            }],
            capability_rules: vec![],
            sandboxes: vec![],
        };

        let engine = PolicyEngine::new(&config).unwrap();

        let request = CapabilityRequest {
            pid: 1234,
            process_path: "/usr/bin/test-app".into(),
            user: "user".into(),
            capability: "cap:filesystem".into(),
            resource: None,
            context: HashMap::new(),
        };

        let result = engine.evaluate(&request);
        assert_eq!(result.decision, PolicyDecision::Allow);
    }
}
