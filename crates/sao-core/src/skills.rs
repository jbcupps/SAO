//! Skills & tools registry types and policy evaluation engine.

use serde::{Deserialize, Serialize};

/// Risk level classification for skills.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
    Unknown,
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            RiskLevel::Low => "low",
            RiskLevel::Medium => "medium",
            RiskLevel::High => "high",
            RiskLevel::Critical => "critical",
            RiskLevel::Unknown => "unknown",
        }
    }

    pub fn from_score(score: u32) -> Self {
        match score {
            0..=10 => RiskLevel::Low,
            11..=30 => RiskLevel::Medium,
            31..=60 => RiskLevel::High,
            _ => RiskLevel::Critical,
        }
    }
}

/// Review status for skills and bindings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    PendingReview,
    Approved,
    Rejected,
    Deprecated,
    Revoked,
}

impl ReviewStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReviewStatus::PendingReview => "pending_review",
            ReviewStatus::Approved => "approved",
            ReviewStatus::Rejected => "rejected",
            ReviewStatus::Deprecated => "deprecated",
            ReviewStatus::Revoked => "revoked",
        }
    }
}

/// Skill declaration submitted by an agent or admin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDeclaration {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub api_endpoints: Vec<String>,
    #[serde(default)]
    pub input_schema: Option<serde_json::Value>,
    #[serde(default)]
    pub output_schema: Option<serde_json::Value>,
}

/// Result of a single policy check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyCheck {
    pub name: String,
    pub passed: bool,
    pub weight: u32,
    pub message: String,
}

/// Aggregated result of the policy evaluation engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyCheckResult {
    pub score: u32,
    pub risk_level: RiskLevel,
    pub auto_approve: bool,
    pub checks: Vec<PolicyCheck>,
}

/// Dangerous permission patterns that indicate high risk.
const DANGEROUS_PERMISSIONS: &[&str] = &[
    "filesystem:write",
    "system:exec",
    "shell:execute",
    "admin:*",
    "root",
    "sudo",
];

/// SSRF indicator patterns in API endpoints.
const SSRF_PATTERNS: &[&str] = &["169.254.169.254", "metadata.google", "file://", ".onion"];

/// Evaluate a skill declaration against the policy engine.
///
/// Runs 5 checks and produces an aggregate risk score (0-100):
/// 1. Dangerous permissions (0 or 30)
/// 2. External API endpoints (0-25, 5 per non-local URL)
/// 3. Permission scope breadth (0, 10, or 20)
/// 4. Metadata completeness (0-15, deducted for missing fields)
/// 5. SSRF pattern check (0 or 30)
///
/// Score <= 10 triggers auto-approval; higher scores flag for manual review.
pub fn evaluate_skill_policy(decl: &SkillDeclaration) -> PolicyCheckResult {
    let mut checks = Vec::with_capacity(5);
    let mut total_score: u32 = 0;

    // Check 1: Dangerous permissions
    let has_dangerous = decl.permissions.iter().any(|p| {
        let lower = p.to_lowercase();
        DANGEROUS_PERMISSIONS.iter().any(|d| lower.contains(d))
    });
    let weight1 = if has_dangerous { 30 } else { 0 };
    total_score += weight1;
    checks.push(PolicyCheck {
        name: "dangerous_permissions".to_string(),
        passed: !has_dangerous,
        weight: weight1,
        message: if has_dangerous {
            "Skill requests dangerous permissions (filesystem:write, system:exec, shell:execute, admin:*, root, or sudo)".to_string()
        } else {
            "No dangerous permissions detected".to_string()
        },
    });

    // Check 2: External API endpoints
    let external_count = decl
        .api_endpoints
        .iter()
        .filter(|ep| {
            let lower = ep.to_lowercase();
            !lower.contains("localhost") && !lower.contains("127.0.0.1") && !lower.contains("::1")
        })
        .count();
    let weight2 = (external_count as u32 * 5).min(25);
    total_score += weight2;
    checks.push(PolicyCheck {
        name: "external_api_endpoints".to_string(),
        passed: external_count == 0,
        weight: weight2,
        message: if external_count == 0 {
            "No external API endpoints".to_string()
        } else {
            format!("{} external API endpoint(s) detected", external_count)
        },
    });

    // Check 3: Permission scope breadth
    let perm_count = decl.permissions.len();
    let weight3 = if perm_count > 10 {
        20
    } else if perm_count > 5 {
        10
    } else {
        0
    };
    total_score += weight3;
    checks.push(PolicyCheck {
        name: "permission_scope_breadth".to_string(),
        passed: perm_count <= 5,
        weight: weight3,
        message: if perm_count <= 5 {
            format!(
                "Permission count ({}) is within acceptable range",
                perm_count
            )
        } else {
            format!(
                "Broad permission scope: {} permissions requested",
                perm_count
            )
        },
    });

    // Check 4: Metadata completeness
    let mut missing_fields = Vec::new();
    if decl.description.as_ref().is_none_or(|s| s.is_empty()) {
        missing_fields.push("description");
    }
    if decl.author.as_ref().is_none_or(|s| s.is_empty()) {
        missing_fields.push("author");
    }
    if decl.input_schema.is_none() {
        missing_fields.push("input_schema");
    }
    if decl.output_schema.is_none() {
        missing_fields.push("output_schema");
    }
    let weight4 = (missing_fields.len() as u32 * 4).min(15);
    total_score += weight4;
    checks.push(PolicyCheck {
        name: "metadata_completeness".to_string(),
        passed: missing_fields.is_empty(),
        weight: weight4,
        message: if missing_fields.is_empty() {
            "All metadata fields present".to_string()
        } else {
            format!("Missing metadata: {}", missing_fields.join(", "))
        },
    });

    // Check 5: SSRF pattern check
    let has_ssrf = decl.api_endpoints.iter().any(|ep| {
        let lower = ep.to_lowercase();
        SSRF_PATTERNS.iter().any(|p| lower.contains(p))
    });
    let weight5 = if has_ssrf { 30 } else { 0 };
    total_score += weight5;
    checks.push(PolicyCheck {
        name: "ssrf_pattern_check".to_string(),
        passed: !has_ssrf,
        weight: weight5,
        message: if has_ssrf {
            "Potential SSRF indicators detected in API endpoints".to_string()
        } else {
            "No SSRF patterns detected".to_string()
        },
    });

    let risk_level = RiskLevel::from_score(total_score);
    let auto_approve = total_score <= 10;

    PolicyCheckResult {
        score: total_score,
        risk_level,
        auto_approve,
        checks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn safe_skill() -> SkillDeclaration {
        SkillDeclaration {
            name: "text-formatter".to_string(),
            version: "1.0.0".to_string(),
            description: Some("Formats text output".to_string()),
            author: Some("SAO Team".to_string()),
            category: Some("utility".to_string()),
            tags: vec!["text".to_string()],
            permissions: vec!["text:read".to_string()],
            api_endpoints: vec![],
            input_schema: Some(serde_json::json!({"type": "string"})),
            output_schema: Some(serde_json::json!({"type": "string"})),
        }
    }

    fn dangerous_skill() -> SkillDeclaration {
        SkillDeclaration {
            name: "shell-executor".to_string(),
            version: "1.0.0".to_string(),
            description: Some("Executes shell commands".to_string()),
            author: Some("Unknown".to_string()),
            category: Some("system".to_string()),
            tags: vec!["shell".to_string()],
            permissions: vec![
                "shell:execute".to_string(),
                "filesystem:write".to_string(),
                "system:exec".to_string(),
            ],
            api_endpoints: vec!["https://evil.example.com/callback".to_string()],
            input_schema: Some(serde_json::json!({"type": "string"})),
            output_schema: Some(serde_json::json!({"type": "string"})),
        }
    }

    fn ssrf_skill() -> SkillDeclaration {
        SkillDeclaration {
            name: "metadata-fetcher".to_string(),
            version: "1.0.0".to_string(),
            description: Some("Fetches cloud metadata".to_string()),
            author: Some("Attacker".to_string()),
            category: Some("network".to_string()),
            tags: vec![],
            permissions: vec!["network:read".to_string()],
            api_endpoints: vec!["http://169.254.169.254/latest/meta-data/".to_string()],
            input_schema: None,
            output_schema: None,
        }
    }

    #[test]
    fn test_safe_skill_auto_approves() {
        let result = evaluate_skill_policy(&safe_skill());
        assert!(
            result.auto_approve,
            "Safe skill should auto-approve, score={}",
            result.score
        );
        assert_eq!(result.risk_level, RiskLevel::Low);
        assert_eq!(result.score, 0);
        assert!(result.checks.iter().all(|c| c.passed));
    }

    #[test]
    fn test_dangerous_skill_flagged() {
        let result = evaluate_skill_policy(&dangerous_skill());
        assert!(
            !result.auto_approve,
            "Dangerous skill should not auto-approve"
        );
        assert!(
            result.score > 30,
            "Score should be high, got {}",
            result.score
        );
        // Should fail dangerous_permissions and external_api_endpoints checks
        let dangerous_check = result
            .checks
            .iter()
            .find(|c| c.name == "dangerous_permissions")
            .unwrap();
        assert!(!dangerous_check.passed);
        assert_eq!(dangerous_check.weight, 30);
    }

    #[test]
    fn test_ssrf_skill_flagged() {
        let result = evaluate_skill_policy(&ssrf_skill());
        assert!(!result.auto_approve, "SSRF skill should not auto-approve");
        let ssrf_check = result
            .checks
            .iter()
            .find(|c| c.name == "ssrf_pattern_check")
            .unwrap();
        assert!(!ssrf_check.passed);
        assert_eq!(ssrf_check.weight, 30);
    }

    #[test]
    fn test_risk_level_from_score() {
        assert_eq!(RiskLevel::from_score(0), RiskLevel::Low);
        assert_eq!(RiskLevel::from_score(10), RiskLevel::Low);
        assert_eq!(RiskLevel::from_score(11), RiskLevel::Medium);
        assert_eq!(RiskLevel::from_score(30), RiskLevel::Medium);
        assert_eq!(RiskLevel::from_score(31), RiskLevel::High);
        assert_eq!(RiskLevel::from_score(60), RiskLevel::High);
        assert_eq!(RiskLevel::from_score(61), RiskLevel::Critical);
        assert_eq!(RiskLevel::from_score(100), RiskLevel::Critical);
    }

    #[test]
    fn test_missing_metadata_adds_score() {
        let decl = SkillDeclaration {
            name: "bare-skill".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            author: None,
            category: None,
            tags: vec![],
            permissions: vec![],
            api_endpoints: vec![],
            input_schema: None,
            output_schema: None,
        };
        let result = evaluate_skill_policy(&decl);
        let meta_check = result
            .checks
            .iter()
            .find(|c| c.name == "metadata_completeness")
            .unwrap();
        assert!(!meta_check.passed);
        // 4 missing fields * 4 = 16, capped at 15
        assert_eq!(meta_check.weight, 15);
    }

    #[test]
    fn test_broad_permissions_flagged() {
        let decl = SkillDeclaration {
            name: "multi-perm".to_string(),
            version: "1.0.0".to_string(),
            description: Some("Many permissions".to_string()),
            author: Some("Test".to_string()),
            category: None,
            tags: vec![],
            permissions: (0..12).map(|i| format!("perm:{}", i)).collect(),
            api_endpoints: vec![],
            input_schema: Some(serde_json::json!({})),
            output_schema: Some(serde_json::json!({})),
        };
        let result = evaluate_skill_policy(&decl);
        let scope_check = result
            .checks
            .iter()
            .find(|c| c.name == "permission_scope_breadth")
            .unwrap();
        assert!(!scope_check.passed);
        assert_eq!(scope_check.weight, 20);
    }

    #[test]
    fn test_local_endpoints_not_flagged() {
        let decl = SkillDeclaration {
            name: "local-tool".to_string(),
            version: "1.0.0".to_string(),
            description: Some("Local only".to_string()),
            author: Some("Test".to_string()),
            category: None,
            tags: vec![],
            permissions: vec![],
            api_endpoints: vec![
                "http://localhost:8080/api".to_string(),
                "http://127.0.0.1:3000/health".to_string(),
            ],
            input_schema: Some(serde_json::json!({})),
            output_schema: Some(serde_json::json!({})),
        };
        let result = evaluate_skill_policy(&decl);
        assert!(result.auto_approve);
        assert_eq!(result.score, 0);
    }
}
