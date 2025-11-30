//! Decision engine - combines all analysis for final decision

use crate::audit::{AuditEvent, AuditLogger};
use crate::config::RiskLevel;
use crate::intent::{AnalyzedIntent, IntentAnalyzer};
use crate::pattern::{PatternAnalysis, PatternLearner};
use crate::policy::{CapabilityRequest, PolicyDecision, PolicyEngine, PolicyResult};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Final security decision
#[derive(Debug, Clone)]
pub struct SecurityDecision {
    /// Final decision
    pub decision: FinalDecision,
    /// Policy evaluation result
    pub policy_result: PolicyResult,
    /// Intent analysis result
    pub intent: Option<AnalyzedIntent>,
    /// Pattern analysis result
    pub pattern: Option<PatternAnalysis>,
    /// Human-readable reason
    pub reason: String,
    /// Recommended action
    pub recommended_action: Option<String>,
}

/// Final decision types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinalDecision {
    /// Allow the request
    Allow,
    /// Deny the request
    Deny,
    /// Allow but run in sandbox
    Sandbox(SandboxLevel),
    /// Need user confirmation
    Prompt,
}

/// Sandbox restriction levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum SandboxLevel {
    /// Light restrictions (network allowed, limited fs)
    Light,
    /// Medium restrictions
    Medium,
    /// Heavy restrictions (no network, minimal fs)
    Heavy,
    /// Maximum isolation
    Maximum,
}

/// Decision engine
pub struct DecisionEngine {
    policy_engine: Arc<PolicyEngine>,
    intent_analyzer: Arc<IntentAnalyzer>,
    pattern_learner: Arc<PatternLearner>,
    audit_logger: Arc<AuditLogger>,
    permissive_mode: bool,
}

impl DecisionEngine {
    /// Create a new decision engine
    pub fn new(
        policy_engine: Arc<PolicyEngine>,
        intent_analyzer: Arc<IntentAnalyzer>,
        pattern_learner: Arc<PatternLearner>,
        audit_logger: Arc<AuditLogger>,
        permissive_mode: bool,
    ) -> Self {
        Self {
            policy_engine,
            intent_analyzer,
            pattern_learner,
            audit_logger,
            permissive_mode,
        }
    }

    /// Evaluate a capability request and make a decision
    pub async fn evaluate(&self, request: &CapabilityRequest) -> SecurityDecision {
        debug!("Evaluating request: {:?}", request);

        // Step 1: Policy evaluation
        let policy_result = self.policy_engine.evaluate(request);
        debug!("Policy result: {:?}", policy_result.decision);

        // Fast path: explicit allow/deny from policy
        if policy_result.decision == PolicyDecision::Allow {
            return self.make_decision(
                FinalDecision::Allow,
                policy_result,
                None,
                None,
                "Allowed by policy",
            ).await;
        }

        if policy_result.decision == PolicyDecision::Deny {
            let decision = if self.permissive_mode {
                FinalDecision::Allow
            } else {
                FinalDecision::Deny
            };
            return self.make_decision(
                decision,
                policy_result,
                None,
                None,
                "Denied by policy",
            ).await;
        }

        // Step 2: Intent analysis
        let intent = self.intent_analyzer.analyze(request).await;
        debug!("Intent analysis: {:?}", intent);

        // Step 3: Pattern analysis
        let pattern = self.pattern_learner.analyze(request);
        debug!("Pattern analysis: {:?}", pattern);

        // Step 4: Make final decision based on all factors
        let (final_decision, reason) = self.synthesize_decision(
            &policy_result,
            &intent,
            &pattern,
        );

        self.make_decision(
            final_decision,
            policy_result,
            Some(intent),
            Some(pattern),
            &reason,
        ).await
    }

    fn synthesize_decision(
        &self,
        policy: &PolicyResult,
        intent: &AnalyzedIntent,
        pattern: &PatternAnalysis,
    ) -> (FinalDecision, String) {
        // High risk + suspicious = deny or heavy sandbox
        if matches!(intent.risk_level, RiskLevel::Critical) {
            if self.permissive_mode {
                return (
                    FinalDecision::Sandbox(SandboxLevel::Maximum),
                    "Critical risk - sandboxing (permissive mode)".into(),
                );
            }
            return (
                FinalDecision::Deny,
                format!("Critical risk: {}", intent.explanation),
            );
        }

        // High risk + anomalous pattern = sandbox or prompt
        if matches!(intent.risk_level, RiskLevel::High) {
            if pattern.anomaly_score > pattern.is_known as u8 as f32 * 0.5 + 0.5 {
                return (
                    FinalDecision::Sandbox(SandboxLevel::Heavy),
                    format!("High risk + anomalous pattern: {}", intent.explanation),
                );
            }
            return (
                FinalDecision::Prompt,
                format!("High risk operation: {}", intent.explanation),
            );
        }

        // Suspicious indicators
        if !intent.suspicious_indicators.is_empty() {
            return (
                FinalDecision::Prompt,
                format!(
                    "Suspicious indicators: {}",
                    intent.suspicious_indicators.join(", ")
                ),
            );
        }

        // Anomalous pattern
        if pattern.anomaly_score > self.pattern_learner.threshold() {
            return (
                FinalDecision::Prompt,
                format!("Unusual behavior: {}", pattern.explanation),
            );
        }

        // Medium risk with unknown pattern
        if matches!(intent.risk_level, RiskLevel::Medium) && !pattern.is_known {
            return (
                FinalDecision::Sandbox(SandboxLevel::Light),
                format!("Medium risk + new pattern: {}", intent.explanation),
            );
        }

        // Default: follow policy
        match policy.decision {
            PolicyDecision::Prompt => (FinalDecision::Prompt, "Policy requires confirmation".into()),
            PolicyDecision::Sandbox => (
                FinalDecision::Sandbox(SandboxLevel::Medium),
                "Policy requires sandbox".into(),
            ),
            _ => (FinalDecision::Allow, "Normal operation".into()),
        }
    }

    async fn make_decision(
        &self,
        decision: FinalDecision,
        policy_result: PolicyResult,
        intent: Option<AnalyzedIntent>,
        pattern: Option<PatternAnalysis>,
        reason: &str,
    ) -> SecurityDecision {
        let recommended_action = match decision {
            FinalDecision::Deny => Some("Review the application and its permissions".into()),
            FinalDecision::Sandbox(_) => Some("Application will run with restricted permissions".into()),
            FinalDecision::Prompt => Some("Please confirm this action".into()),
            FinalDecision::Allow => None,
        };

        SecurityDecision {
            decision,
            policy_result,
            intent,
            pattern,
            reason: reason.into(),
            recommended_action,
        }
    }

    /// Record a decision for learning
    pub fn record_decision(&self, request: &CapabilityRequest, decision: &SecurityDecision, user_approved: bool) {
        // Log to audit
        self.audit_logger.log(AuditEvent::Decision {
            request: request.clone(),
            decision: format!("{:?}", decision.decision),
            reason: decision.reason.clone(),
            user_approved,
        });

        // Learn from approved requests
        if user_approved || decision.decision == FinalDecision::Allow {
            self.pattern_learner.learn(request);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_allow_trusted_app() {
        let policy_config = PolicyConfig::default();
        let intent_config = IntentConfig::default();
        let pattern_config = PatternConfig::default();
        let audit_config = AuditConfig::default();

        let policy_engine = Arc::new(PolicyEngine::new(&policy_config).unwrap());
        let intent_analyzer = Arc::new(IntentAnalyzer::new(&intent_config).unwrap());
        let pattern_learner = Arc::new(PatternLearner::new(&pattern_config).unwrap());
        let audit_logger = Arc::new(AuditLogger::new(&audit_config).unwrap());

        let engine = DecisionEngine::new(
            policy_engine,
            intent_analyzer,
            pattern_learner,
            audit_logger,
            false,
        );

        let request = CapabilityRequest {
            pid: 1,
            process_path: "/usr/lib/nyx/init".into(),
            user: "root".into(),
            capability: "cap:full".into(),
            resource: None,
            context: HashMap::new(),
        };

        let decision = engine.evaluate(&request).await;
        assert_eq!(decision.decision, FinalDecision::Allow);
    }
}
