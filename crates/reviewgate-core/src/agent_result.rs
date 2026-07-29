use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    AgentDisposition, BlockingReason, CostSummary, Finding, FindingClassification,
    FindingDisposition, FindingDispositionRecord, FindingEvidence, MAX_DISPOSITION_HISTORY,
    ReviewAngleError, ReviewArtifact, ReviewGateError, ReviewScope, ReviewStatus, Severity,
    SummaryState, finding_code_fingerprint, semantic_fingerprint,
};

pub const AGENT_RESULT_SCHEMA_VERSION: &str = "reviewgate-agent-result/v1";
pub const AGENT_DISPOSITIONS_SCHEMA_VERSION: &str = "reviewgate-agent-dispositions/v1";
pub const MAX_AGENT_RESULT_BYTES: usize = 1024 * 1024;
pub const MAX_AGENT_DISPOSITION_EVIDENCE_BYTES: usize = 4 * 1024;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct AgentReviewResult {
    pub schema_version: String,
    pub scope: ReviewScope,
    pub status: ReviewStatus,
    pub score: Option<u8>,
    pub reviewed_sha: String,
    pub angle_errors: Vec<ReviewAngleError>,
    pub costs: AgentResultCosts,
    pub findings: Vec<AgentResultFinding>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct AgentResultCosts {
    pub estimated_total_usd: Option<f64>,
    pub summary: Option<CostSummary>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct AgentResultFinding {
    pub id: String,
    pub semantic_fingerprint: String,
    pub disposition: FindingDisposition,
    pub severity: Severity,
    pub confidence: f64,
    pub classification: FindingClassification,
    pub blocking_reason: Option<BlockingReason>,
    pub path: Option<String>,
    pub line: Option<u32>,
    pub claim: Option<String>,
    pub causal_evidence: Option<String>,
    pub evidence: Vec<FindingEvidence>,
    pub reproduction: Option<String>,
    pub suggested_fix: String,
    pub thread_id: Option<String>,
    pub prior_dispositions: Vec<FindingDispositionRecord>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AgentDispositionSubmission {
    pub semantic_fingerprint: String,
    pub disposition: AgentDisposition,
    pub evidence: String,
    pub actor: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AgentDispositionState {
    pub schema_version: String,
    pub scope: ReviewScope,
    pub reviewed_sha: String,
    pub submission: AgentDispositionSubmission,
}

impl AgentDispositionState {
    pub fn validate(&self) -> Result<(), ReviewGateError> {
        if self.schema_version != AGENT_DISPOSITIONS_SCHEMA_VERSION {
            return Err(ReviewGateError::InvalidReviewOutcome(
                "agent dispositions have an unsupported schema_version".to_string(),
            ));
        }
        if self.reviewed_sha.trim().is_empty() {
            return Err(ReviewGateError::InvalidReviewOutcome(
                "agent dispositions reviewed_sha must not be empty".to_string(),
            ));
        }
        if !matches!(
            &self.scope,
            ReviewScope::PullRequest {
                repository,
                pull_request_number,
            } if !repository.trim().is_empty() && *pull_request_number > 0
        ) {
            return Err(ReviewGateError::InvalidReviewOutcome(
                "agent dispositions require pull request scope".to_string(),
            ));
        }
        let submission = &self.submission;
        if submission.semantic_fingerprint.trim().is_empty()
            || submission.evidence.trim().is_empty()
            || submission.evidence.len() > MAX_AGENT_DISPOSITION_EVIDENCE_BYTES
            || submission.actor.trim().is_empty()
        {
            return Err(ReviewGateError::InvalidReviewOutcome(
                "agent dispositions require a finding fingerprint, bounded evidence, and an actor"
                    .to_string(),
            ));
        }
        Ok(())
    }

    pub fn apply_to_summary(&self, state: &mut SummaryState) -> Result<(), ReviewGateError> {
        self.validate()?;
        if self.scope != state.scope || self.reviewed_sha != state.last_reviewed_sha {
            return Err(ReviewGateError::InvalidReviewOutcome(
                "agent dispositions do not match the canonical review state".to_string(),
            ));
        }
        let submission = &self.submission;
        let tracked = state
            .tracked_findings
            .iter_mut()
            .find(|tracked| tracked.semantic_fingerprint == submission.semantic_fingerprint)
            .ok_or_else(|| {
                ReviewGateError::InvalidReviewOutcome(format!(
                    "agent disposition references unknown finding {}",
                    submission.semantic_fingerprint
                ))
            })?;
        let disposition = submission.disposition.tracked_disposition();
        let duplicate = tracked.disposition_history.iter().any(|record| {
            record.submitted_disposition == Some(submission.disposition)
                && record.evidence_summary == submission.evidence
                && record.actor == submission.actor
                && record.reviewed_sha == self.reviewed_sha
        });
        if !duplicate {
            tracked.disposition = disposition;
            tracked.disposition_history.push(FindingDispositionRecord {
                disposition,
                submitted_disposition: Some(submission.disposition),
                evidence_summary: submission.evidence.clone(),
                actor: submission.actor.clone(),
                reviewed_sha: self.reviewed_sha.clone(),
                code_fingerprint: finding_code_fingerprint(&tracked.finding),
            });
            if tracked.disposition_history.len() > MAX_DISPOSITION_HISTORY {
                tracked
                    .disposition_history
                    .drain(0..tracked.disposition_history.len() - MAX_DISPOSITION_HISTORY);
            }
        }
        state.validate()
    }
}

impl AgentReviewResult {
    pub fn from_artifact(
        artifact: &ReviewArtifact,
        scope: ReviewScope,
        thread_ids: BTreeMap<String, String>,
    ) -> Result<Self, ReviewGateError> {
        artifact.validate()?;
        let findings = if artifact.tracked_findings.is_empty() {
            artifact
                .findings
                .iter()
                .map(|finding| {
                    project_finding(finding, FindingDisposition::StillOpen, &[], &thread_ids)
                })
                .collect()
        } else {
            artifact
                .tracked_findings
                .iter()
                .map(|tracked| {
                    project_finding(
                        &tracked.finding,
                        tracked.disposition,
                        &tracked.disposition_history,
                        &thread_ids,
                    )
                })
                .collect()
        };
        let result = Self {
            schema_version: AGENT_RESULT_SCHEMA_VERSION.to_string(),
            scope,
            status: artifact.status.clone(),
            score: artifact.score,
            reviewed_sha: artifact.reviewed_sha.clone(),
            angle_errors: artifact.angle_errors.clone(),
            costs: AgentResultCosts {
                estimated_total_usd: artifact.estimated_cost_usd,
                summary: artifact.cost_summary.clone(),
            },
            findings,
        };
        result.validate()?;
        Ok(result)
    }

    pub fn validate(&self) -> Result<(), ReviewGateError> {
        if self.schema_version != AGENT_RESULT_SCHEMA_VERSION {
            return Err(ReviewGateError::InvalidReviewOutcome(
                "agent result has an unsupported schema_version".to_string(),
            ));
        }
        if self.reviewed_sha.trim().is_empty() {
            return Err(ReviewGateError::InvalidReviewOutcome(
                "agent result reviewed_sha must not be empty".to_string(),
            ));
        }
        match self.status {
            ReviewStatus::ReviewError if self.score.is_some() || self.angle_errors.is_empty() => {
                return Err(ReviewGateError::InvalidReviewOutcome(
                    "review_error agent result requires null score and angle_errors".to_string(),
                ));
            }
            ReviewStatus::Passed | ReviewStatus::NeedsChanges
                if self.score.is_none() || !self.angle_errors.is_empty() =>
            {
                return Err(ReviewGateError::InvalidReviewOutcome(
                    "completed agent result requires a score and no angle_errors".to_string(),
                ));
            }
            _ => {}
        }
        if let ReviewScope::PullRequest {
            repository,
            pull_request_number,
        } = &self.scope
            && (repository.trim().is_empty() || *pull_request_number == 0)
        {
            return Err(ReviewGateError::InvalidReviewOutcome(
                "pull request agent result requires repository and pull request number".to_string(),
            ));
        }
        let mut fingerprints = BTreeSet::new();
        for finding in &self.findings {
            if finding.semantic_fingerprint.trim().is_empty()
                || !fingerprints.insert(finding.semantic_fingerprint.as_str())
            {
                return Err(ReviewGateError::InvalidReviewOutcome(
                    "agent result findings require unique semantic fingerprints".to_string(),
                ));
            }
        }
        let encoded = serde_json::to_vec(self).map_err(|error| {
            ReviewGateError::InvalidReviewOutcome(format!(
                "agent result could not be serialized: {error}"
            ))
        })?;
        if encoded.len() > MAX_AGENT_RESULT_BYTES {
            return Err(ReviewGateError::InvalidReviewOutcome(format!(
                "agent result exceeds {MAX_AGENT_RESULT_BYTES} bytes"
            )));
        }
        Ok(())
    }
}

fn project_finding(
    finding: &Finding,
    disposition: FindingDisposition,
    history: &[FindingDispositionRecord],
    thread_ids: &BTreeMap<String, String>,
) -> AgentResultFinding {
    let grounding = finding.grounding.as_ref();
    AgentResultFinding {
        id: finding.id.clone(),
        semantic_fingerprint: semantic_fingerprint(finding),
        disposition,
        severity: finding.severity,
        confidence: finding.confidence,
        classification: finding.classification,
        blocking_reason: finding.blocking_reason,
        path: finding.file.clone(),
        line: finding.line,
        claim: grounding.map(|grounding| grounding.claim.clone()),
        causal_evidence: grounding.map(|grounding| grounding.causal_path.clone()),
        evidence: grounding
            .map(|grounding| grounding.evidence.clone())
            .unwrap_or_default(),
        reproduction: grounding.and_then(|grounding| grounding.reproduction.clone()),
        suggested_fix: finding.agent_instruction.clone(),
        thread_id: thread_ids.get(&finding.id).cloned(),
        prior_dispositions: history.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{
        AGENT_DISPOSITIONS_SCHEMA_VERSION, AgentDisposition, AgentDispositionState,
        AgentDispositionSubmission, AgentReviewResult, FindingDisposition, ReviewArtifact,
        ReviewScope, SummaryState, reconcile_findings, semantic_fingerprint,
    };

    #[test]
    fn projects_a_versioned_agent_result_with_threads_and_disposition_history() {
        let mut artifact: ReviewArtifact =
            serde_json::from_str(include_str!("../../../fixtures/simple-review.json"))
                .expect("fixture parses");
        artifact.findings.truncate(1);
        artifact = artifact
            .with_computed_score()
            .expect("fixture score computes");
        let convergence = reconcile_findings(
            artifact.findings.clone(),
            &[],
            &crate::ConvergenceDelta::first_review(&artifact.reviewed_sha),
        )
        .expect("fixture reconciles");
        artifact.tracked_findings = convergence.tracked_findings;
        let finding_id = artifact.findings[0].id.clone();
        let result = AgentReviewResult::from_artifact(
            &artifact,
            ReviewScope::PullRequest {
                repository: "LVTD-LLC/reviewgate".to_string(),
                pull_request_number: 48,
            },
            BTreeMap::from([(finding_id, "PRRT_thread".to_string())]),
        )
        .expect("result projects");

        assert_eq!(result.schema_version, "reviewgate-agent-result/v1");
        assert_eq!(result.reviewed_sha, artifact.reviewed_sha);
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].thread_id.as_deref(), Some("PRRT_thread"));
        assert_eq!(
            result.findings[0].semantic_fingerprint,
            semantic_fingerprint(&artifact.findings[0])
        );
        assert_eq!(
            result.findings[0].prior_dispositions[0].disposition,
            FindingDisposition::StillOpen
        );
        assert_eq!(
            result.findings[0].claim.as_deref(),
            artifact.findings[0]
                .grounding
                .as_ref()
                .map(|grounding| grounding.claim.as_str())
        );
    }

    #[test]
    fn result_status_and_score_follow_every_artifact_outcome() {
        let completed: ReviewArtifact =
            serde_json::from_str(include_str!("../../../fixtures/simple-review.json"))
                .expect("fixture parses");
        let completed = completed
            .with_computed_score()
            .expect("fixture score computes");
        let passed = ReviewArtifact {
            findings: vec![],
            tracked_findings: vec![],
            ..completed.clone()
        }
        .with_computed_score()
        .expect("passed artifact");
        let passed_result =
            AgentReviewResult::from_artifact(&passed, ReviewScope::Local, BTreeMap::new())
                .expect("passed result");
        assert_eq!(passed_result.score, Some(5));
        assert!(passed_result.angle_errors.is_empty());

        let needs_changes =
            AgentReviewResult::from_artifact(&completed, ReviewScope::Local, BTreeMap::new())
                .expect("needs changes result");
        assert_eq!(needs_changes.score, completed.score);

        let mut review_error = completed.prepared_for_publication("different-current-head");
        review_error.tracked_findings.clear();
        let error_result =
            AgentReviewResult::from_artifact(&review_error, ReviewScope::Local, BTreeMap::new())
                .expect("review error result");
        assert_eq!(error_result.score, None);
        assert!(!error_result.angle_errors.is_empty());
    }

    #[test]
    fn applies_every_agent_disposition_to_canonical_history_idempotently() {
        let artifact: ReviewArtifact =
            serde_json::from_str(include_str!("../../../fixtures/simple-review.json"))
                .expect("fixture");
        let artifact = artifact.with_computed_score().expect("score");
        let mut state = SummaryState::for_artifact(&artifact, None, 20).expect("state");
        state.scope = ReviewScope::PullRequest {
            repository: "LVTD-LLC/reviewgate".to_string(),
            pull_request_number: 48,
        };
        let fingerprint = state.tracked_findings[0].semantic_fingerprint.clone();
        let mut updates = Vec::new();

        for disposition in [
            AgentDisposition::Accepted,
            AgentDisposition::Fixed,
            AgentDisposition::RejectedWithEvidence,
            AgentDisposition::AlreadyImplemented,
            AgentDisposition::IntentionalContract,
            AgentDisposition::NeedsHuman,
        ] {
            let update = AgentDispositionState {
                schema_version: AGENT_DISPOSITIONS_SCHEMA_VERSION.to_string(),
                scope: state.scope.clone(),
                reviewed_sha: state.last_reviewed_sha.clone(),
                submission: AgentDispositionSubmission {
                    semantic_fingerprint: fingerprint.clone(),
                    disposition,
                    evidence: format!("{disposition:?} verified"),
                    actor: "repair-agent".to_string(),
                },
            };
            updates.push(update.clone());
            update.apply_to_summary(&mut state).expect("applies");
            let history_len = state.tracked_findings[0].disposition_history.len();
            update.apply_to_summary(&mut state).expect("idempotent");
            assert_eq!(
                state.tracked_findings[0].disposition_history.len(),
                history_len
            );
            assert_eq!(
                state.tracked_findings[0]
                    .disposition_history
                    .last()
                    .and_then(|record| record.submitted_disposition),
                Some(disposition)
            );
        }
        let history = state.tracked_findings[0].disposition_history.clone();
        for update in &updates {
            update.apply_to_summary(&mut state).expect("replay");
        }
        assert_eq!(state.tracked_findings[0].disposition_history, history);
    }
}
