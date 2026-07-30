use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    AgentDisposition, BlockingReason, CostSummary, DEFAULT_TARGET_SCORE, Finding,
    FindingClassification, FindingDisposition, FindingDispositionRecord, FindingEvidence,
    MAX_DISPOSITION_HISTORY, ReviewAngleError, ReviewArtifact, ReviewErrorKind, ReviewGateError,
    ReviewScope, ReviewStatus, ReviewTimings, Severity, SummaryState, finding_code_fingerprint,
    semantic_fingerprint,
};

pub const AGENT_RESULT_SCHEMA_VERSION: &str = "reviewgate-agent-result/v1";
pub const AGENT_DISPOSITIONS_SCHEMA_VERSION: &str = "reviewgate-agent-dispositions/v1";
pub const MAX_AGENT_RESULT_BYTES: usize = 1024 * 1024;
pub const MAX_AGENT_DISPOSITION_EVIDENCE_BYTES: usize = 4 * 1024;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AgentReviewResult {
    pub schema_version: String,
    pub scope: ReviewScope,
    pub status: ReviewStatus,
    pub score: Option<u8>,
    pub reviewed_sha: String,
    pub angle_errors: Vec<ReviewAngleError>,
    pub costs: AgentResultCosts,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timings: Option<ReviewTimings>,
    pub findings: Vec<AgentResultFinding>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AgentResultCosts {
    pub estimated_total_usd: Option<f64>,
    pub summary: Option<CostSummary>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentThreadStatus {
    #[default]
    Unknown,
    NotPublished,
    Open,
    Resolved,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentThreadTransition {
    #[default]
    Unknown,
    NotPublished,
    Retained,
    Reopened,
    ResolutionPending,
    ResolvedFixed,
    ResolvedRejectedWithEvidence,
    ResolvedIntentionalContract,
    ResolvedSuperseded,
    ResolvedExternally,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentResultThread {
    pub id: Option<String>,
    pub status: AgentThreadStatus,
    pub is_outdated: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
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
    #[serde(default)]
    pub thread_status: AgentThreadStatus,
    #[serde(default)]
    pub thread_transition: AgentThreadTransition,
    #[serde(default)]
    pub thread_outdated: bool,
    #[serde(default)]
    pub reopening_evidence: Option<String>,
    pub prior_dispositions: Vec<FindingDispositionRecord>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentDispositionSubmission {
    pub semantic_fingerprint: String,
    pub disposition: AgentDisposition,
    pub evidence: String,
    pub actor: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
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

    pub fn apply_to_summary(
        &self,
        state: &mut SummaryState,
        submission_id: u64,
    ) -> Result<(), ReviewGateError> {
        self.validate()?;
        if submission_id == 0 {
            return Err(ReviewGateError::InvalidReviewOutcome(
                "agent disposition submission id must be positive".to_string(),
            ));
        }
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
        let duplicate = tracked
            .disposition_history
            .iter()
            .any(|record| record.submission_id == Some(submission_id));
        if !duplicate {
            tracked.disposition = disposition;
            tracked.disposition_history.push(FindingDispositionRecord {
                disposition,
                submitted_disposition: Some(submission.disposition),
                submission_id: Some(submission_id),
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
    pub fn artifact_validation_error(
        scope: ReviewScope,
        reviewed_sha: impl Into<String>,
    ) -> Result<Self, ReviewGateError> {
        let result = Self {
            schema_version: AGENT_RESULT_SCHEMA_VERSION.to_string(),
            scope,
            status: ReviewStatus::ReviewError,
            score: None,
            reviewed_sha: reviewed_sha.into(),
            angle_errors: vec![ReviewAngleError {
                angle_id: "artifact_validation".to_string(),
                angle_name: "Artifact validation".to_string(),
                kind: ReviewErrorKind::MalformedResponse,
                retryable: false,
                message: "The review artifact failed deterministic validation.".to_string(),
                model: "reviewgate".to_string(),
            }],
            costs: AgentResultCosts {
                estimated_total_usd: None,
                summary: None,
            },
            timings: None,
            findings: vec![],
        };
        result.validate()?;
        Ok(result)
    }

    pub fn from_artifact(
        artifact: &ReviewArtifact,
        scope: ReviewScope,
        threads: BTreeMap<String, AgentResultThread>,
    ) -> Result<Self, ReviewGateError> {
        artifact.validate()?;
        let findings = if artifact.tracked_findings.is_empty() {
            artifact
                .findings
                .iter()
                .map(|finding| {
                    project_finding(finding, FindingDisposition::StillOpen, &[], &threads)
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
                        &threads,
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
            timings: artifact
                .metrics
                .as_ref()
                .and_then(|metrics| metrics.timings.clone()),
            findings,
        };
        result.validate()?;
        Ok(result)
    }

    pub fn refresh_threads(
        &mut self,
        threads: BTreeMap<String, AgentResultThread>,
    ) -> Result<(), ReviewGateError> {
        for finding in &mut self.findings {
            apply_thread_state(finding, threads.get(&finding.semantic_fingerprint));
        }
        self.validate()
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
            ReviewStatus::Passed if self.score != Some(DEFAULT_TARGET_SCORE) => {
                return Err(ReviewGateError::InvalidReviewOutcome(
                    "passed agent result requires the fixed 5/5 score".to_string(),
                ));
            }
            ReviewStatus::NeedsChanges
                if self
                    .score
                    .is_some_and(|score| score >= DEFAULT_TARGET_SCORE) =>
            {
                return Err(ReviewGateError::InvalidReviewOutcome(
                    "needs_changes agent result requires a score below 5/5".to_string(),
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
        if self
            .costs
            .estimated_total_usd
            .is_some_and(|total| !total.is_finite() || total < 0.0)
        {
            return Err(ReviewGateError::InvalidReviewOutcome(
                "agent result estimated cost must be finite and non-negative".to_string(),
            ));
        }
        if let Some(summary) = &self.costs.summary {
            summary.validate()?;
        }
        for error in &self.angle_errors {
            error.validate()?;
        }
        let mut fingerprints = BTreeSet::new();
        for finding in &self.findings {
            if finding.id.trim().is_empty()
                || finding.semantic_fingerprint.trim().is_empty()
                || !fingerprints.insert(finding.semantic_fingerprint.as_str())
                || finding.suggested_fix.trim().is_empty()
                || !finding.confidence.is_finite()
                || !(0.0..=1.0).contains(&finding.confidence)
                || finding.line == Some(0)
                || finding
                    .thread_id
                    .as_ref()
                    .is_some_and(|thread_id| thread_id.trim().is_empty())
                || !valid_thread_state(finding)
                || finding
                    .reopening_evidence
                    .as_ref()
                    .is_some_and(|evidence| evidence.trim().is_empty())
                || finding.disposition != FindingDisposition::StillOpen
                    && finding.blocking_reason.is_some()
                || finding.evidence.iter().any(|evidence| {
                    evidence.path.trim().is_empty()
                        || evidence.line == 0
                        || evidence.excerpt.trim().is_empty()
                        || evidence.reason.trim().is_empty()
                })
                || finding.prior_dispositions.iter().any(|record| {
                    record.evidence_summary.trim().is_empty()
                        || record.actor.trim().is_empty()
                        || record.reviewed_sha.trim().is_empty()
                        || record.code_fingerprint.trim().is_empty()
                        || record.submission_id == Some(0)
                        || record.submitted_disposition.is_some() != record.submission_id.is_some()
                })
                || finding
                    .prior_dispositions
                    .last()
                    .is_some_and(|record| record.disposition != finding.disposition)
            {
                return Err(ReviewGateError::InvalidReviewOutcome(
                    "agent result findings require valid fields and unique semantic fingerprints"
                        .to_string(),
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

fn thread_transition(
    status: AgentThreadStatus,
    disposition: FindingDisposition,
    was_previously_resolved: bool,
    has_reopening_evidence: bool,
) -> AgentThreadTransition {
    match (status, disposition) {
        (AgentThreadStatus::Unknown, _) => AgentThreadTransition::Unknown,
        (AgentThreadStatus::NotPublished, _) => AgentThreadTransition::NotPublished,
        (AgentThreadStatus::Open, FindingDisposition::StillOpen)
            if was_previously_resolved && has_reopening_evidence =>
        {
            AgentThreadTransition::Reopened
        }
        (AgentThreadStatus::Open, FindingDisposition::StillOpen | FindingDisposition::Disputed) => {
            AgentThreadTransition::Retained
        }
        (
            AgentThreadStatus::Open,
            FindingDisposition::Fixed
            | FindingDisposition::RejectedWithEvidence
            | FindingDisposition::IntentionalContract
            | FindingDisposition::Superseded,
        ) => AgentThreadTransition::ResolutionPending,
        (AgentThreadStatus::Resolved, FindingDisposition::Fixed) => {
            AgentThreadTransition::ResolvedFixed
        }
        (AgentThreadStatus::Resolved, FindingDisposition::RejectedWithEvidence) => {
            AgentThreadTransition::ResolvedRejectedWithEvidence
        }
        (AgentThreadStatus::Resolved, FindingDisposition::IntentionalContract) => {
            AgentThreadTransition::ResolvedIntentionalContract
        }
        (AgentThreadStatus::Resolved, FindingDisposition::Superseded) => {
            AgentThreadTransition::ResolvedSuperseded
        }
        (
            AgentThreadStatus::Resolved,
            FindingDisposition::StillOpen | FindingDisposition::Disputed,
        ) => AgentThreadTransition::ResolvedExternally,
    }
}

fn valid_thread_state(finding: &AgentResultFinding) -> bool {
    let was_previously_resolved = finding
        .prior_dispositions
        .iter()
        .rev()
        .skip(1)
        .any(|record| record.disposition.is_settled());
    let expected_transition = thread_transition(
        finding.thread_status,
        finding.disposition,
        was_previously_resolved,
        finding.reopening_evidence.is_some(),
    );
    let identity_is_valid = match finding.thread_status {
        AgentThreadStatus::Unknown => !finding.thread_outdated,
        AgentThreadStatus::NotPublished => finding.thread_id.is_none() && !finding.thread_outdated,
        AgentThreadStatus::Open | AgentThreadStatus::Resolved => finding.thread_id.is_some(),
    };
    identity_is_valid && finding.thread_transition == expected_transition
}

fn project_finding(
    finding: &Finding,
    disposition: FindingDisposition,
    history: &[FindingDispositionRecord],
    threads: &BTreeMap<String, AgentResultThread>,
) -> AgentResultFinding {
    let grounding = finding.grounding.as_ref();
    let fingerprint = semantic_fingerprint(finding);
    let reopening_evidence = grounding
        .and_then(|grounding| grounding.reopening_evidence.clone())
        .filter(|evidence| !evidence.trim().is_empty());
    let mut result = AgentResultFinding {
        id: finding.id.clone(),
        semantic_fingerprint: fingerprint.clone(),
        disposition,
        severity: finding.severity,
        confidence: finding.confidence,
        classification: finding.classification,
        blocking_reason: finding
            .blocking_reason
            .filter(|_| disposition == FindingDisposition::StillOpen),
        path: finding.file.clone(),
        line: finding.line,
        claim: grounding.map(|grounding| grounding.claim.clone()),
        causal_evidence: grounding.map(|grounding| grounding.causal_path.clone()),
        evidence: grounding
            .map(|grounding| grounding.evidence.clone())
            .unwrap_or_default(),
        reproduction: grounding.and_then(|grounding| grounding.reproduction.clone()),
        suggested_fix: finding.agent_instruction.clone(),
        thread_id: None,
        thread_status: AgentThreadStatus::NotPublished,
        thread_transition: AgentThreadTransition::NotPublished,
        thread_outdated: false,
        reopening_evidence,
        prior_dispositions: history.to_vec(),
    };
    apply_thread_state(&mut result, threads.get(&fingerprint));
    result
}

fn apply_thread_state(finding: &mut AgentResultFinding, thread: Option<&AgentResultThread>) {
    finding.thread_id = thread.and_then(|thread| thread.id.clone());
    finding.thread_status = thread
        .map(|thread| thread.status)
        .unwrap_or(AgentThreadStatus::NotPublished);
    finding.thread_outdated = thread.is_some_and(|thread| thread.is_outdated);
    let was_previously_resolved = finding
        .prior_dispositions
        .iter()
        .rev()
        .skip(1)
        .any(|record| record.disposition.is_settled());
    finding.thread_transition = thread_transition(
        finding.thread_status,
        finding.disposition,
        was_previously_resolved,
        finding.reopening_evidence.is_some(),
    );
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{
        AGENT_DISPOSITIONS_SCHEMA_VERSION, AgentDisposition, AgentDispositionState,
        AgentDispositionSubmission, AgentResultThread, AgentReviewResult, AgentThreadStatus,
        AgentThreadTransition, DEFAULT_TARGET_SCORE, FindingDisposition, MAX_AGENT_RESULT_BYTES,
        ReviewArtifact, ReviewScope, SummaryState, reconcile_findings, semantic_fingerprint,
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
        artifact.metrics = Some(crate::ReviewMetrics {
            timings: Some(crate::ReviewTimings {
                queue_ms: Some(10),
                startup_ms: 20,
                model_ms: 30,
                publish_ms: 40,
            }),
            ..crate::compute_metrics(&artifact, crate::Severity::P4)
        });
        let fingerprint = semantic_fingerprint(&artifact.findings[0]);
        let result = AgentReviewResult::from_artifact(
            &artifact,
            ReviewScope::PullRequest {
                repository: "LVTD-LLC/reviewgate".to_string(),
                pull_request_number: 48,
            },
            BTreeMap::from([(
                fingerprint,
                AgentResultThread {
                    id: Some("PRRT_thread".to_string()),
                    status: AgentThreadStatus::Open,
                    is_outdated: true,
                },
            )]),
        )
        .expect("result projects");

        assert_eq!(result.schema_version, "reviewgate-agent-result/v1");
        assert_eq!(result.reviewed_sha, artifact.reviewed_sha);
        assert_eq!(result.findings.len(), 1);
        assert_eq!(
            result.timings,
            artifact.metrics.and_then(|metrics| metrics.timings)
        );
        assert_eq!(result.findings[0].thread_id.as_deref(), Some("PRRT_thread"));
        assert_eq!(result.findings[0].thread_status, AgentThreadStatus::Open);
        assert_eq!(
            result.findings[0].thread_transition,
            AgentThreadTransition::Retained
        );
        assert!(result.findings[0].thread_outdated);
        assert_eq!(result.findings[0].reopening_evidence, None);
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
    fn refreshes_agent_result_thread_state_after_external_reconciliation() {
        let mut artifact: ReviewArtifact =
            serde_json::from_str(include_str!("../../../fixtures/simple-review.json"))
                .expect("fixture parses");
        artifact.findings.truncate(1);
        artifact = artifact.with_computed_score().expect("fixture score");
        let mut state = SummaryState::for_artifact(&artifact, None, 20).expect("state");
        state.scope = ReviewScope::PullRequest {
            repository: "LVTD-LLC/reviewgate".to_string(),
            pull_request_number: 48,
        };
        let fingerprint = state.tracked_findings[0].semantic_fingerprint.clone();
        AgentDispositionState {
            schema_version: AGENT_DISPOSITIONS_SCHEMA_VERSION.to_string(),
            scope: state.scope.clone(),
            reviewed_sha: state.last_reviewed_sha.clone(),
            submission: AgentDispositionSubmission {
                semantic_fingerprint: fingerprint.clone(),
                disposition: AgentDisposition::Fixed,
                evidence: "The current head contains the fix.".to_string(),
                actor: "repair-agent".to_string(),
            },
        }
        .apply_to_summary(&mut state, 77)
        .expect("fixed disposition");
        artifact.findings.clear();
        artifact.tracked_findings = state.tracked_findings;
        artifact = artifact.with_computed_score().expect("passed artifact");
        let mut result = AgentReviewResult::from_artifact(
            &artifact,
            state.scope,
            BTreeMap::from([(
                fingerprint.clone(),
                AgentResultThread {
                    id: Some("PRRT_fixed".to_string()),
                    status: AgentThreadStatus::Open,
                    is_outdated: true,
                },
            )]),
        )
        .expect("result projects");
        assert_eq!(
            result.findings[0].thread_transition,
            AgentThreadTransition::ResolutionPending
        );

        result
            .refresh_threads(BTreeMap::from([(
                fingerprint,
                AgentResultThread {
                    id: Some("PRRT_fixed".to_string()),
                    status: AgentThreadStatus::Resolved,
                    is_outdated: true,
                },
            )]))
            .expect("thread state refreshes");

        assert_eq!(
            result.findings[0].thread_status,
            AgentThreadStatus::Resolved
        );
        assert_eq!(
            result.findings[0].thread_transition,
            AgentThreadTransition::ResolvedFixed
        );
        assert!(result.findings[0].thread_outdated);
    }

    #[test]
    fn rejects_artifacts_whose_active_and_tracked_findings_disagree() {
        let mut artifact: ReviewArtifact =
            serde_json::from_str(include_str!("../../../fixtures/simple-review.json"))
                .expect("fixture parses");
        artifact = artifact.with_computed_score().expect("fixture score");
        let convergence = reconcile_findings(
            artifact.findings.clone(),
            &[],
            &crate::ConvergenceDelta::first_review(&artifact.reviewed_sha),
        )
        .expect("fixture reconciles");
        artifact.tracked_findings = convergence.tracked_findings;
        artifact.findings.remove(0);
        let error = artifact
            .with_computed_score()
            .expect_err("mismatch rejected");

        assert!(error.to_string().contains("tracked finding state"));
    }

    #[test]
    fn resolved_findings_keep_history_without_advertising_a_blocker() {
        let mut artifact: ReviewArtifact =
            serde_json::from_str(include_str!("../../../fixtures/simple-review.json"))
                .expect("fixture parses");
        artifact.findings.truncate(1);
        artifact = artifact.with_computed_score().expect("fixture score");
        let mut state = SummaryState::for_artifact(&artifact, None, 20).expect("state");
        state.scope = ReviewScope::PullRequest {
            repository: "LVTD-LLC/reviewgate".to_string(),
            pull_request_number: 48,
        };
        let update = AgentDispositionState {
            schema_version: AGENT_DISPOSITIONS_SCHEMA_VERSION.to_string(),
            scope: state.scope.clone(),
            reviewed_sha: state.last_reviewed_sha.clone(),
            submission: AgentDispositionSubmission {
                semantic_fingerprint: state.tracked_findings[0].semantic_fingerprint.clone(),
                disposition: AgentDisposition::Fixed,
                evidence: "The replacement is present on the current head.".to_string(),
                actor: "repair-agent".to_string(),
            },
        };
        update
            .apply_to_summary(&mut state, 77)
            .expect("fixed disposition");
        let scope = state.scope.clone();
        artifact.findings.clear();
        artifact.tracked_findings = state.tracked_findings;
        artifact = artifact.with_computed_score().expect("passed artifact");
        let fingerprint = artifact.tracked_findings[0].semantic_fingerprint.clone();

        let result = AgentReviewResult::from_artifact(
            &artifact,
            scope,
            BTreeMap::from([(
                fingerprint,
                AgentResultThread {
                    id: Some("PRRT_fixed".to_string()),
                    status: AgentThreadStatus::Resolved,
                    is_outdated: false,
                },
            )]),
        )
        .expect("agent result");

        assert_eq!(result.status, crate::ReviewStatus::Passed);
        assert_eq!(result.findings[0].disposition, FindingDisposition::Fixed);
        assert_eq!(result.findings[0].blocking_reason, None);
        assert_eq!(
            result.findings[0].thread_transition,
            AgentThreadTransition::ResolvedFixed
        );
        assert_eq!(
            result.findings[0]
                .prior_dispositions
                .last()
                .and_then(|record| record.submitted_disposition),
            Some(AgentDisposition::Fixed)
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
    fn rejects_malformed_downloaded_agent_results() {
        let artifact: ReviewArtifact =
            serde_json::from_str(include_str!("../../../fixtures/simple-review.json"))
                .expect("fixture parses");
        let artifact = artifact.with_computed_score().expect("fixture score");
        let mut valid =
            AgentReviewResult::from_artifact(&artifact, ReviewScope::Local, BTreeMap::new())
                .expect("valid result");

        valid.schema_version = "unknown".to_string();
        assert!(valid.validate().is_err());

        valid = AgentReviewResult::from_artifact(&artifact, ReviewScope::Local, BTreeMap::new())
            .expect("valid result");
        valid.reviewed_sha.clear();
        assert!(valid.validate().is_err());

        valid = AgentReviewResult::from_artifact(&artifact, ReviewScope::Local, BTreeMap::new())
            .expect("valid result");
        valid.status = crate::ReviewStatus::Passed;
        valid.score = Some(DEFAULT_TARGET_SCORE - 1);
        assert!(valid.validate().is_err());

        let error_artifact = artifact.clone().prepared_for_publication("different-head");
        let error_result =
            AgentReviewResult::from_artifact(&error_artifact, ReviewScope::Local, BTreeMap::new())
                .expect("valid review error");
        let mut invalid_error_score = error_result.clone();
        invalid_error_score.score = Some(0);
        assert!(invalid_error_score.validate().is_err());
        let mut invalid_error_without_details = error_result.clone();
        invalid_error_without_details.angle_errors.clear();
        assert!(invalid_error_without_details.validate().is_err());

        valid = AgentReviewResult::from_artifact(&artifact, ReviewScope::Local, BTreeMap::new())
            .expect("valid result");
        valid.score = None;
        assert!(valid.validate().is_err());

        valid = AgentReviewResult::from_artifact(&artifact, ReviewScope::Local, BTreeMap::new())
            .expect("valid result");
        valid.angle_errors = error_result.angle_errors;
        assert!(valid.validate().is_err());

        valid = AgentReviewResult::from_artifact(&artifact, ReviewScope::Local, BTreeMap::new())
            .expect("valid result");
        valid.scope = ReviewScope::PullRequest {
            repository: String::new(),
            pull_request_number: 0,
        };
        assert!(valid.validate().is_err());

        valid = AgentReviewResult::from_artifact(&artifact, ReviewScope::Local, BTreeMap::new())
            .expect("valid result");
        valid.findings.push(valid.findings[0].clone());
        assert!(valid.validate().is_err());

        valid = AgentReviewResult::from_artifact(&artifact, ReviewScope::Local, BTreeMap::new())
            .expect("valid result");
        valid.findings[0].suggested_fix = "x".repeat(MAX_AGENT_RESULT_BYTES);
        assert!(valid.validate().is_err());

        valid = AgentReviewResult::from_artifact(&artifact, ReviewScope::Local, BTreeMap::new())
            .expect("valid result");
        let mut raw = serde_json::to_value(&valid).expect("serialize result");
        raw.as_object_mut()
            .expect("result object")
            .insert("unexpected".to_string(), serde_json::json!(true));
        assert!(serde_json::from_value::<AgentReviewResult>(raw).is_err());

        let mut raw = serde_json::to_value(&valid).expect("serialize result");
        raw.pointer_mut("/findings/0")
            .and_then(serde_json::Value::as_object_mut)
            .expect("finding object")
            .insert("unexpected".to_string(), serde_json::json!(true));
        assert!(serde_json::from_value::<AgentReviewResult>(raw).is_err());

        let mut raw = serde_json::to_value(&valid).expect("serialize result");
        raw["status"] = serde_json::json!("failed");
        assert!(serde_json::from_value::<AgentReviewResult>(raw).is_err());

        let mut tracked_artifact = artifact.clone();
        tracked_artifact.tracked_findings = SummaryState::for_artifact(&tracked_artifact, None, 20)
            .expect("tracked state")
            .tracked_findings;
        valid = AgentReviewResult::from_artifact(
            &tracked_artifact,
            ReviewScope::Local,
            BTreeMap::new(),
        )
        .expect("tracked result");
        valid.findings[0].prior_dispositions[0].actor.clear();
        assert!(valid.validate().is_err());
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
            update
                .apply_to_summary(&mut state, updates.len() as u64)
                .expect("applies");
            let history_len = state.tracked_findings[0].disposition_history.len();
            update
                .apply_to_summary(&mut state, updates.len() as u64)
                .expect("idempotent");
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
        for (index, update) in updates.iter().enumerate() {
            update
                .apply_to_summary(&mut state, (index + 1) as u64)
                .expect("replay");
        }
        assert_eq!(state.tracked_findings[0].disposition_history, history);
    }

    #[test]
    fn repeated_payload_after_an_intervening_transition_is_a_new_event() {
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
        let scope = state.scope.clone();
        let reviewed_sha = state.last_reviewed_sha.clone();
        let update = |disposition| AgentDispositionState {
            schema_version: AGENT_DISPOSITIONS_SCHEMA_VERSION.to_string(),
            scope: scope.clone(),
            reviewed_sha: reviewed_sha.clone(),
            submission: AgentDispositionSubmission {
                semantic_fingerprint: fingerprint.clone(),
                disposition,
                evidence: "Verified against the current head.".to_string(),
                actor: "repair-agent".to_string(),
            },
        };

        update(AgentDisposition::Accepted)
            .apply_to_summary(&mut state, 101)
            .expect("first accepted event");
        update(AgentDisposition::Fixed)
            .apply_to_summary(&mut state, 102)
            .expect("fixed event");
        update(AgentDisposition::Accepted)
            .apply_to_summary(&mut state, 103)
            .expect("second accepted event");

        let history = &state.tracked_findings[0].disposition_history;
        assert_eq!(history.len(), 4);
        assert_eq!(
            history.last().and_then(|record| record.submission_id),
            Some(103)
        );
        assert_eq!(
            state.tracked_findings[0].disposition,
            FindingDisposition::StillOpen
        );
    }

    #[test]
    fn legacy_v1_findings_deserialize_with_unknown_thread_state() {
        let artifact: ReviewArtifact =
            serde_json::from_str(include_str!("../../../fixtures/simple-review.json"))
                .expect("fixture parses");
        let artifact = artifact.with_computed_score().expect("fixture score");
        let result =
            AgentReviewResult::from_artifact(&artifact, ReviewScope::Local, BTreeMap::new())
                .expect("result projects");
        let mut legacy = serde_json::to_value(result).expect("result serializes");
        let findings = legacy
            .get_mut("findings")
            .and_then(serde_json::Value::as_array_mut)
            .expect("findings array");
        for finding in findings.iter_mut() {
            let finding = finding.as_object_mut().expect("finding object");
            finding.remove("thread_status");
            finding.remove("thread_transition");
            finding.remove("thread_outdated");
            finding.remove("reopening_evidence");
        }
        findings[0]["thread_id"] = serde_json::json!("PRRT_legacy");

        let legacy: AgentReviewResult =
            serde_json::from_value(legacy).expect("legacy v1 result deserializes");

        legacy.validate().expect("legacy v1 result validates");
        assert_eq!(legacy.findings[0].thread_status, AgentThreadStatus::Unknown);
        assert_eq!(
            legacy.findings[0].thread_transition,
            AgentThreadTransition::Unknown
        );
        assert_eq!(legacy.findings[0].thread_id.as_deref(), Some("PRRT_legacy"));
    }
}
