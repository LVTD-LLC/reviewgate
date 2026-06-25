use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const SUMMARY_MARKER: &str = "<!-- review-gate-summary -->";

#[derive(Debug, Error)]
pub enum ReviewGateError {
    #[error("score must be between 0 and 5, got {0}")]
    InvalidScore(u8),
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    Passed,
    NeedsChanges,
    Failed,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub enum Severity {
    P1,
    P2,
    P3,
    P4,
}

impl Severity {
    pub fn score_ceiling(&self) -> u8 {
        match self {
            Severity::P1 => 2,
            Severity::P2 => 3,
            Severity::P3 => 4,
            Severity::P4 => 5,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::P1 => "P1",
            Severity::P2 => "P2",
            Severity::P3 => "P3",
            Severity::P4 => "P4",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Finding {
    pub id: String,
    pub severity: Severity,
    pub confidence: f32,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub title: String,
    pub detail: Option<String>,
    pub agent_instruction: String,
}

impl Finding {
    pub fn is_blocking(&self, fail_under: u8) -> bool {
        self.severity.score_ceiling() < fail_under
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ReviewArtifact {
    pub score: u8,
    pub target_score: u8,
    pub fail_under: u8,
    pub reviewed_sha: String,
    pub status: ReviewStatus,
    pub verdict: String,
    pub models: Vec<String>,
    pub estimated_cost_usd: Option<f32>,
    pub findings: Vec<Finding>,
    pub notes: Vec<String>,
}

impl ReviewArtifact {
    pub fn validate(&self) -> Result<(), ReviewGateError> {
        validate_score(self.score)?;
        validate_score(self.target_score)?;
        validate_score(self.fail_under)?;
        Ok(())
    }

    pub fn with_computed_score(mut self) -> Result<Self, ReviewGateError> {
        self.score = compute_score(&self.findings);
        self.status = if self.score >= self.target_score {
            ReviewStatus::Passed
        } else if self.score < self.fail_under {
            ReviewStatus::Failed
        } else {
            ReviewStatus::NeedsChanges
        };
        self.validate()?;
        Ok(self)
    }
}

pub fn validate_score(score: u8) -> Result<(), ReviewGateError> {
    if score <= 5 {
        Ok(())
    } else {
        Err(ReviewGateError::InvalidScore(score))
    }
}

pub fn compute_score(findings: &[Finding]) -> u8 {
    findings
        .iter()
        .map(|finding| finding.severity.score_ceiling())
        .min()
        .unwrap_or(5)
}

pub fn render_summary(artifact: &ReviewArtifact) -> Result<String, ReviewGateError> {
    artifact.validate()?;

    let mut output = String::new();
    output.push_str(SUMMARY_MARKER);
    output.push_str("\n\n");
    output.push_str(&format!("# Review Gate: {}/5\n\n", artifact.score));
    output.push_str(&format!("Reviewed commit: `{}`  \n", artifact.reviewed_sha));
    output.push_str(&format!("Target: {}/5  \n", artifact.target_score));
    output.push_str(&format!("Fail under: {}/5  \n", artifact.fail_under));
    output.push_str(&format!("Models: {}  \n", artifact.models.join(", ")));
    if let Some(cost) = artifact.estimated_cost_usd {
        output.push_str(&format!("Estimated model cost: ${cost:.2}\n"));
    }
    output.push('\n');
    output.push_str("## Verdict\n\n");
    output.push_str(&artifact.verdict);
    output.push_str("\n\n");

    let blocking: Vec<&Finding> = artifact
        .findings
        .iter()
        .filter(|finding| finding.is_blocking(artifact.fail_under))
        .collect();
    output.push_str("## Blocking Findings\n\n");
    if blocking.is_empty() {
        output.push_str("None.\n\n");
    } else {
        for (index, finding) in blocking.iter().enumerate() {
            output.push_str(&format!(
                "{}. {}: {}",
                index + 1,
                finding.severity.as_str(),
                finding.title
            ));
            if let (Some(file), Some(line)) = (&finding.file, finding.line) {
                output.push_str(&format!(" (`{}:{}`)", file, line));
            }
            output.push('\n');
        }
        output.push('\n');
    }

    let non_blocking: Vec<&Finding> = artifact
        .findings
        .iter()
        .filter(|finding| !finding.is_blocking(artifact.fail_under))
        .collect();
    output.push_str("## Non-Blocking Notes\n\n");
    if non_blocking.is_empty() && artifact.notes.is_empty() {
        output.push_str("None.\n\n");
    } else {
        for finding in non_blocking {
            output.push_str(&format!(
                "- {}: {}\n",
                finding.severity.as_str(),
                finding.title
            ));
        }
        for note in &artifact.notes {
            output.push_str(&format!("- {note}\n"));
        }
        output.push('\n');
    }

    output.push_str("## Agent Instructions\n\n");
    if artifact.findings.is_empty() {
        output.push_str("No findings remain. Re-run Review Gate if new commits land.\n");
    } else {
        for (index, finding) in artifact.findings.iter().enumerate() {
            output.push_str(&format!(
                "{}. {}: {}",
                index + 1,
                finding.severity.as_str(),
                finding.agent_instruction
            ));
            if let (Some(file), Some(line)) = (&finding.file, finding.line) {
                output.push_str(&format!(" (`{}:{}`)", file, line));
            }
            output.push('\n');
        }
        output.push('\n');
        if blocking.is_empty() {
            output.push_str("Re-run Review Gate after pushing if new commits land.\n");
        } else {
            output.push_str("Fix the blocking findings first. Re-run Review Gate after pushing.\n");
        }
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_score_from_highest_severity() {
        let findings = vec![Finding {
            id: "rg_001".to_string(),
            severity: Severity::P2,
            confidence: 0.9,
            file: Some("src/lib.rs".to_string()),
            line: Some(42),
            title: "Missing regression test".to_string(),
            detail: None,
            agent_instruction: "Add the regression test.".to_string(),
        }];

        assert_eq!(compute_score(&findings), 3);
    }

    #[test]
    fn computes_score_without_relying_on_enum_ordering() {
        let findings = vec![
            Finding {
                id: "rg_001".to_string(),
                severity: Severity::P4,
                confidence: 0.9,
                file: None,
                line: None,
                title: "Style note".to_string(),
                detail: None,
                agent_instruction: "Consider simplifying this wording.".to_string(),
            },
            Finding {
                id: "rg_002".to_string(),
                severity: Severity::P1,
                confidence: 0.9,
                file: None,
                line: None,
                title: "Security issue".to_string(),
                detail: None,
                agent_instruction: "Fix the unsafe behavior.".to_string(),
            },
        ];

        assert_eq!(compute_score(&findings), 2);
    }

    #[test]
    fn renders_canonical_summary_marker_and_score() {
        let artifact = ReviewArtifact {
            score: 4,
            target_score: 5,
            fail_under: 4,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::NeedsChanges,
            verdict: "Good shape, one minor issue remains.".to_string(),
            models: vec!["balanced".to_string()],
            estimated_cost_usd: Some(0.08),
            findings: vec![],
            notes: vec![],
        };

        let summary = render_summary(&artifact).expect("summary renders");
        assert!(summary.starts_with(SUMMARY_MARKER));
        assert!(summary.contains("# Review Gate: 4/5"));
    }

    #[test]
    fn renders_agent_instructions_for_findings() {
        let artifact = ReviewArtifact {
            score: 3,
            target_score: 5,
            fail_under: 3,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::NeedsChanges,
            verdict: "One blocking issue remains.".to_string(),
            models: vec!["balanced".to_string()],
            estimated_cost_usd: None,
            findings: vec![Finding {
                id: "rg_001".to_string(),
                severity: Severity::P2,
                confidence: 0.9,
                file: Some("src/lib.rs".to_string()),
                line: Some(42),
                title: "Missing regression test".to_string(),
                detail: None,
                agent_instruction: "Add a regression test for the missing branch.".to_string(),
            }],
            notes: vec![],
        };

        let summary = render_summary(&artifact).expect("summary renders");

        assert!(summary.contains("## Agent Instructions"));
        assert!(
            summary
                .contains("1. P2: Add a regression test for the missing branch. (`src/lib.rs:42`)")
        );
    }

    #[test]
    fn renders_non_blocking_instruction_footer_without_blocking_language() {
        let artifact = ReviewArtifact {
            score: 4,
            target_score: 5,
            fail_under: 4,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::NeedsChanges,
            verdict: "One advisory issue remains.".to_string(),
            models: vec!["balanced".to_string()],
            estimated_cost_usd: None,
            findings: vec![Finding {
                id: "rg_001".to_string(),
                severity: Severity::P3,
                confidence: 0.9,
                file: None,
                line: None,
                title: "Consider clearer docs".to_string(),
                detail: None,
                agent_instruction: "Clarify the README example.".to_string(),
            }],
            notes: vec![],
        };

        let summary = render_summary(&artifact).expect("summary renders");

        assert!(summary.contains("1. P3: Clarify the README example."));
        assert!(summary.contains("Re-run Review Gate after pushing if new commits land."));
        assert!(!summary.contains("Fix the blocking findings first."));
    }

    #[test]
    fn computed_status_uses_fail_under_threshold() {
        let artifact = ReviewArtifact {
            score: 5,
            target_score: 5,
            fail_under: 4,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::Passed,
            verdict: "One blocking issue remains.".to_string(),
            models: vec!["balanced".to_string()],
            estimated_cost_usd: None,
            findings: vec![Finding {
                id: "rg_001".to_string(),
                severity: Severity::P2,
                confidence: 0.9,
                file: Some("src/lib.rs".to_string()),
                line: Some(42),
                title: "Missing regression test".to_string(),
                detail: None,
                agent_instruction: "Add the regression test.".to_string(),
            }],
            notes: vec![],
        };

        let artifact = artifact
            .with_computed_score()
            .expect("computed artifact is valid");

        assert_eq!(artifact.score, 3);
        assert_eq!(artifact.status, ReviewStatus::Failed);
    }

    #[test]
    fn renders_blocking_findings_from_fail_under_threshold() {
        let artifact = ReviewArtifact {
            score: 3,
            target_score: 5,
            fail_under: 3,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::NeedsChanges,
            verdict: "One recoverable issue remains.".to_string(),
            models: vec!["balanced".to_string()],
            estimated_cost_usd: None,
            findings: vec![Finding {
                id: "rg_001".to_string(),
                severity: Severity::P2,
                confidence: 0.9,
                file: Some("src/lib.rs".to_string()),
                line: Some(42),
                title: "Missing regression test".to_string(),
                detail: None,
                agent_instruction: "Add the regression test.".to_string(),
            }],
            notes: vec![],
        };

        let summary = render_summary(&artifact).expect("summary renders");

        assert!(summary.contains("## Blocking Findings\n\nNone."));
        assert!(summary.contains("- P2: Missing regression test"));
        assert!(!summary.contains("Fix the blocking findings first."));
    }
}
