use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{DEFAULT_TARGET_SCORE, Finding, ReviewGateError};

pub const LATE_BLOCKER_CONFIDENCE_THRESHOLD: f64 = 0.95;
pub const MAX_DISPOSITION_HISTORY: usize = 8;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReviewScope {
    Local,
    PullRequest {
        repository: String,
        pull_request_number: u64,
    },
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FindingDisposition {
    Fixed,
    StillOpen,
    RejectedWithEvidence,
    IntentionalContract,
    Disputed,
    Superseded,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentDisposition {
    Accepted,
    Fixed,
    RejectedWithEvidence,
    AlreadyImplemented,
    IntentionalContract,
    NeedsHuman,
}

impl AgentDisposition {
    pub fn tracked_disposition(self) -> FindingDisposition {
        match self {
            Self::Accepted => FindingDisposition::StillOpen,
            Self::Fixed | Self::AlreadyImplemented => FindingDisposition::Fixed,
            Self::RejectedWithEvidence => FindingDisposition::RejectedWithEvidence,
            Self::IntentionalContract => FindingDisposition::IntentionalContract,
            Self::NeedsHuman => FindingDisposition::Disputed,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct FindingDispositionRecord {
    pub disposition: FindingDisposition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submitted_disposition: Option<AgentDisposition>,
    pub evidence_summary: String,
    pub actor: String,
    pub reviewed_sha: String,
    pub code_fingerprint: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct FindingDispositionUpdate {
    pub semantic_fingerprint: String,
    pub disposition: FindingDisposition,
    pub evidence_summary: String,
    pub actor: String,
    pub reviewed_sha: String,
    pub code_fingerprint: String,
    pub resolution: Finding,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct TrackedFinding {
    pub semantic_fingerprint: String,
    pub finding: Finding,
    pub disposition: FindingDisposition,
    pub disposition_history: Vec<FindingDispositionRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvergenceDelta {
    pub previous_reviewed_sha: Option<String>,
    pub current_reviewed_sha: String,
    pub changed_files: BTreeSet<String>,
    pub external_contract_changed: bool,
}

impl ConvergenceDelta {
    pub fn first_review(current_reviewed_sha: impl Into<String>) -> Self {
        Self {
            previous_reviewed_sha: None,
            current_reviewed_sha: current_reviewed_sha.into(),
            changed_files: BTreeSet::new(),
            external_contract_changed: false,
        }
    }

    pub fn unchanged(reviewed_sha: impl Into<String>) -> Self {
        let reviewed_sha = reviewed_sha.into();
        Self {
            previous_reviewed_sha: Some(reviewed_sha.clone()),
            current_reviewed_sha: reviewed_sha,
            changed_files: BTreeSet::new(),
            external_contract_changed: false,
        }
    }

    pub fn head_changed(
        previous_reviewed_sha: impl Into<String>,
        current_reviewed_sha: impl Into<String>,
        changed_files: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            previous_reviewed_sha: Some(previous_reviewed_sha.into()),
            current_reviewed_sha: current_reviewed_sha.into(),
            changed_files: changed_files.into_iter().collect(),
            external_contract_changed: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConvergenceResult {
    pub findings: Vec<Finding>,
    pub tracked_findings: Vec<TrackedFinding>,
    pub notes: Vec<String>,
}

pub fn semantic_fingerprint(finding: &Finding) -> String {
    let semantic_key = finding
        .grounding
        .as_ref()
        .map(|grounding| grounding.semantic_key.as_str())
        .filter(|semantic_key| !semantic_key.trim().is_empty())
        .unwrap_or(finding.title.as_str());
    format!(
        "{}:{}:{}",
        finding.classification.as_str(),
        normalize_identity_component(finding.file.as_deref().unwrap_or("pr")),
        normalize_identity_component(semantic_key)
    )
}

fn normalize_identity_component(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut last_was_separator = false;
    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            normalized.push(character);
            last_was_separator = false;
        } else if !last_was_separator && !normalized.is_empty() {
            normalized.push('.');
            last_was_separator = true;
        }
    }
    while normalized.ends_with('.') {
        normalized.pop();
    }
    normalized
}

pub fn finding_code_fingerprint(finding: &Finding) -> String {
    let mut canonical = String::new();
    canonical.push_str(finding.file.as_deref().unwrap_or("pr"));
    canonical.push('\n');
    if let Some(grounding) = &finding.grounding {
        canonical.push_str(&grounding.causal_path);
        canonical.push('\n');
        let mut evidence = grounding
            .evidence
            .iter()
            .chain(&grounding.related_tests)
            .map(|evidence| {
                format!(
                    "{}\n{}\n{}",
                    evidence.path,
                    match evidence.side {
                        crate::FindingEvidenceSide::New => "new",
                        crate::FindingEvidenceSide::Old => "old",
                    },
                    evidence.excerpt
                )
            })
            .collect::<Vec<_>>();
        evidence.sort();
        for item in evidence {
            canonical.push_str(&item);
            canonical.push('\n');
        }
    }
    let digest = Sha256::digest(canonical.as_bytes());
    format!("{digest:x}")
}

fn disposition_record(
    disposition: FindingDisposition,
    finding: &Finding,
    reviewed_sha: &str,
    evidence_summary: impl Into<String>,
) -> FindingDispositionRecord {
    FindingDispositionRecord {
        disposition,
        submitted_disposition: None,
        evidence_summary: evidence_summary.into(),
        actor: "reviewgate".to_string(),
        reviewed_sha: reviewed_sha.to_string(),
        code_fingerprint: finding_code_fingerprint(finding),
    }
}

fn relevant_code_changed(
    finding: &Finding,
    delta: &ConvergenceDelta,
    include_external_contract: bool,
) -> bool {
    if include_external_contract && delta.external_contract_changed {
        return true;
    }
    finding
        .file
        .as_deref()
        .map_or(!delta.changed_files.is_empty(), |file| {
            delta.changed_files.contains(file)
        })
}

fn has_reopening_evidence(finding: &Finding) -> bool {
    finding
        .grounding
        .as_ref()
        .and_then(|grounding| grounding.reopening_evidence.as_deref())
        .is_some_and(|evidence| !evidence.trim().is_empty())
}

fn has_novelty_evidence(finding: &Finding) -> bool {
    finding
        .grounding
        .as_ref()
        .and_then(|grounding| grounding.novelty_evidence.as_deref())
        .is_some_and(|evidence| !evidence.trim().is_empty())
}

fn validate_semantic_identity(finding: &Finding) -> Result<String, ReviewGateError> {
    let fingerprint = semantic_fingerprint(finding);
    if fingerprint.ends_with(':') {
        return Err(ReviewGateError::InvalidReviewOutcome(format!(
            "finding {} has an empty semantic identity",
            finding.id
        )));
    }
    Ok(fingerprint)
}

fn push_still_open_record(
    tracked: &mut TrackedFinding,
    current_sha: &str,
    evidence_summary: impl Into<String>,
) {
    let next = disposition_record(
        FindingDisposition::StillOpen,
        &tracked.finding,
        current_sha,
        evidence_summary,
    );
    if tracked.disposition_history.last().is_none_or(|last| {
        last.disposition != next.disposition
            || last.reviewed_sha != next.reviewed_sha
            || last.code_fingerprint != next.code_fingerprint
    }) {
        tracked.disposition_history.push(next);
        if tracked.disposition_history.len() > MAX_DISPOSITION_HISTORY {
            tracked
                .disposition_history
                .drain(0..tracked.disposition_history.len() - MAX_DISPOSITION_HISTORY);
        }
    }
    tracked.disposition = FindingDisposition::StillOpen;
}

pub fn reconcile_findings(
    current_findings: Vec<Finding>,
    previous_findings: &[TrackedFinding],
    delta: &ConvergenceDelta,
) -> Result<ConvergenceResult, ReviewGateError> {
    reconcile_findings_with_updates(current_findings, previous_findings, delta, &[])
}

pub fn reconcile_findings_with_updates(
    current_findings: Vec<Finding>,
    previous_findings: &[TrackedFinding],
    delta: &ConvergenceDelta,
    disposition_updates: &[FindingDispositionUpdate],
) -> Result<ConvergenceResult, ReviewGateError> {
    if delta.current_reviewed_sha.trim().is_empty() {
        return Err(ReviewGateError::InvalidReviewOutcome(
            "convergence current_reviewed_sha must not be empty".to_string(),
        ));
    }
    if !previous_findings.is_empty() && delta.previous_reviewed_sha.is_none() {
        return Err(ReviewGateError::InvalidReviewOutcome(
            "convergence prior finding state requires a previous SHA".to_string(),
        ));
    }

    let mut previous_by_fingerprint = BTreeMap::new();
    for previous in previous_findings {
        if previous.semantic_fingerprint != validate_semantic_identity(&previous.finding)? {
            return Err(ReviewGateError::InvalidReviewOutcome(format!(
                "tracked finding {} semantic fingerprint does not match its finding",
                previous.finding.id
            )));
        }
        if previous.disposition_history.is_empty() {
            return Err(ReviewGateError::InvalidReviewOutcome(format!(
                "tracked finding {} has no disposition history",
                previous.finding.id
            )));
        }
        if previous
            .disposition_history
            .last()
            .map(|record| record.disposition)
            != Some(previous.disposition)
        {
            return Err(ReviewGateError::InvalidReviewOutcome(format!(
                "tracked finding {} disposition does not match its latest record",
                previous.finding.id
            )));
        }
        if previous_by_fingerprint
            .insert(previous.semantic_fingerprint.clone(), previous.clone())
            .is_some()
        {
            return Err(ReviewGateError::InvalidReviewOutcome(format!(
                "duplicate prior semantic fingerprint {}",
                previous.semantic_fingerprint
            )));
        }
    }

    let mut current_by_fingerprint = BTreeMap::new();
    for finding in current_findings {
        let fingerprint = validate_semantic_identity(&finding)?;
        if current_by_fingerprint
            .insert(fingerprint.clone(), finding)
            .is_some()
        {
            return Err(ReviewGateError::InvalidReviewOutcome(format!(
                "duplicate current semantic fingerprint {fingerprint}"
            )));
        }
    }

    let unchanged_head =
        delta.previous_reviewed_sha.as_deref() == Some(delta.current_reviewed_sha.as_str());
    if unchanged_head {
        if !disposition_updates.is_empty() {
            return Err(ReviewGateError::InvalidReviewOutcome(
                "unchanged-head review cannot record disposition updates".to_string(),
            ));
        }
        let findings = previous_by_fingerprint
            .values()
            .filter(|tracked| tracked.disposition == FindingDisposition::StillOpen)
            .map(|tracked| tracked.finding.clone())
            .collect::<Vec<_>>();
        return Ok(ConvergenceResult {
            findings,
            tracked_findings: previous_by_fingerprint.into_values().collect(),
            notes: vec![
                "Unchanged head: reused the prior validated finding set and ignored reviewer drift."
                    .to_string(),
            ],
        });
    }

    let first_review = delta.previous_reviewed_sha.is_none();
    let mut findings = Vec::new();
    let mut tracked_findings = Vec::new();
    let mut notes = Vec::new();
    let mut updates_by_fingerprint = BTreeMap::new();
    for update in disposition_updates {
        let resolution_grounding = update.resolution.grounding.as_ref();
        if update.semantic_fingerprint.trim().is_empty()
            || update.evidence_summary.trim().is_empty()
            || update.actor.trim().is_empty()
            || update.reviewed_sha != delta.current_reviewed_sha
            || update.code_fingerprint.trim().is_empty()
            || update.semantic_fingerprint != validate_semantic_identity(&update.resolution)?
            || update.code_fingerprint != finding_code_fingerprint(&update.resolution)
            || resolution_grounding.and_then(|grounding| grounding.resolution_disposition)
                != Some(FindingDisposition::Fixed)
            || resolution_grounding
                .and_then(|grounding| grounding.resolution_evidence_summary.as_deref())
                .map(str::trim)
                != Some(update.evidence_summary.trim())
        {
            return Err(ReviewGateError::InvalidReviewOutcome(
                "disposition update is incomplete or targets the wrong SHA".to_string(),
            ));
        }
        if updates_by_fingerprint
            .insert(update.semantic_fingerprint.clone(), update.clone())
            .is_some()
        {
            return Err(ReviewGateError::InvalidReviewOutcome(format!(
                "duplicate disposition update {}",
                update.semantic_fingerprint
            )));
        }
    }

    for (fingerprint, mut previous) in previous_by_fingerprint {
        let current = current_by_fingerprint.remove(&fingerprint);
        let update = updates_by_fingerprint.remove(&fingerprint);
        if let Some(update) = update {
            let prior_code_fingerprint = previous
                .disposition_history
                .last()
                .map(|record| record.code_fingerprint.as_str())
                .unwrap_or_default();
            if previous.disposition != FindingDisposition::StillOpen
                || update.disposition != FindingDisposition::Fixed
                || current.is_some()
                || !relevant_code_changed(&previous.finding, delta, false)
                || update.code_fingerprint == prior_code_fingerprint
            {
                return Err(ReviewGateError::InvalidReviewOutcome(format!(
                    "fixed disposition update for {fingerprint} is not justified by the current delta"
                )));
            }
            previous.disposition = FindingDisposition::Fixed;
            previous.disposition_history.push(FindingDispositionRecord {
                disposition: update.disposition,
                submitted_disposition: None,
                evidence_summary: update.evidence_summary,
                actor: update.actor,
                reviewed_sha: update.reviewed_sha,
                code_fingerprint: update.code_fingerprint,
            });
            if previous.disposition_history.len() > MAX_DISPOSITION_HISTORY {
                previous
                    .disposition_history
                    .drain(0..previous.disposition_history.len() - MAX_DISPOSITION_HISTORY);
            }
            tracked_findings.push(previous);
            continue;
        }
        match previous.disposition {
            FindingDisposition::StillOpen => {
                let relevant_changed = relevant_code_changed(&previous.finding, delta, false);
                match current {
                    Some(current) if relevant_changed => {
                        previous.finding =
                            preserve_open_blocking_policy(&previous.finding, current);
                        push_still_open_record(
                            &mut previous,
                            &delta.current_reviewed_sha,
                            "The finding remains on relevant code changed since the prior review.",
                        );
                        findings.push(previous.finding.clone());
                    }
                    Some(_) => {
                        notes.push(format!(
                            "Retained still-open finding {} because its relevant code did not change.",
                            previous.finding.id
                        ));
                        findings.push(previous.finding.clone());
                    }
                    None => {
                        notes.push(format!(
                            "Retained still-open finding {} because reviewer omission is not evidence of a fix.",
                            previous.finding.id
                        ));
                        findings.push(previous.finding.clone());
                    }
                }
            }
            FindingDisposition::RejectedWithEvidence
            | FindingDisposition::IntentionalContract
            | FindingDisposition::Disputed
            | FindingDisposition::Fixed
            | FindingDisposition::Superseded => {
                if let Some(current) = current {
                    let current_code_fingerprint = finding_code_fingerprint(&current);
                    let prior_code_fingerprint = previous
                        .disposition_history
                        .last()
                        .map(|record| record.code_fingerprint.as_str())
                        .unwrap_or_default();
                    let can_reopen = relevant_code_changed(&previous.finding, delta, true)
                        && current_code_fingerprint != prior_code_fingerprint
                        && has_reopening_evidence(&current);
                    if can_reopen {
                        previous.finding = current;
                        push_still_open_record(
                            &mut previous,
                            &delta.current_reviewed_sha,
                            "Relevant code or contract evidence changed and justified reopening.",
                        );
                        findings.push(previous.finding.clone());
                    } else {
                        notes.push(format!(
                            "Suppressed recurrence of {} because its prior disposition remains binding.",
                            previous.finding.id
                        ));
                    }
                }
            }
        }
        tracked_findings.push(previous);
    }
    if let Some(unknown) = updates_by_fingerprint.keys().next() {
        return Err(ReviewGateError::InvalidReviewOutcome(format!(
            "disposition update references unknown finding {unknown}"
        )));
    }

    for (fingerprint, finding) in current_by_fingerprint {
        let late_blocker = !first_review && finding.is_blocking(DEFAULT_TARGET_SCORE);
        if late_blocker
            && (finding.confidence < LATE_BLOCKER_CONFIDENCE_THRESHOLD
                || !has_novelty_evidence(&finding))
        {
            notes.push(format!(
                "Suppressed late blocker {}: new blockers after the first review require confidence >= {:.2} and specific novelty evidence.",
                finding.id, LATE_BLOCKER_CONFIDENCE_THRESHOLD
            ));
            continue;
        }
        let evidence_summary = if late_blocker {
            "Accepted as a late blocker with high confidence and specific novelty evidence."
        } else {
            "Observed in the current review."
        };
        let record = disposition_record(
            FindingDisposition::StillOpen,
            &finding,
            &delta.current_reviewed_sha,
            evidence_summary,
        );
        findings.push(finding.clone());
        tracked_findings.push(TrackedFinding {
            semantic_fingerprint: fingerprint,
            finding,
            disposition: FindingDisposition::StillOpen,
            disposition_history: vec![record],
        });
    }

    tracked_findings
        .sort_by(|left, right| left.semantic_fingerprint.cmp(&right.semantic_fingerprint));
    Ok(ConvergenceResult {
        findings,
        tracked_findings,
        notes,
    })
}

fn preserve_open_blocking_policy(prior: &Finding, mut current: Finding) -> Finding {
    if prior.is_blocking(DEFAULT_TARGET_SCORE) {
        current.severity = current.severity.min(prior.severity);
        current.confidence = current.confidence.max(prior.confidence);
        current.evidence_gate_result = prior.evidence_gate_result;
        current.calibrate_policy();
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BlockingReason, EvidenceGateResult, FindingClassification, FindingGrounding, FindingScope,
        ReviewStatus, Severity,
    };
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct ConvergenceFixture {
        pr364_five_passes: Vec<FixtureRound>,
        pr365_rejected_permission_recurrence: PermissionRecurrenceFixture,
    }

    #[derive(Debug, Deserialize)]
    struct FixtureRound {
        previous_sha: Option<String>,
        sha: String,
        changed_files: Vec<String>,
        fixed_semantic_keys: Vec<String>,
        findings: Vec<FixtureFinding>,
        expected_open: usize,
    }

    #[derive(Debug, Deserialize)]
    struct FixtureFinding {
        id: String,
        semantic_key: String,
        file: String,
        confidence: f64,
        instruction: String,
        novelty_evidence: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    struct PermissionRecurrenceFixture {
        seed_sha: String,
        semantic_key: String,
        file: String,
        rounds: Vec<PermissionRecurrenceRound>,
    }

    #[derive(Debug, Deserialize)]
    struct PermissionRecurrenceRound {
        sha: String,
        changed_files: Vec<String>,
        finding_id: String,
        expected_open: usize,
    }

    fn blocker(
        id: &str,
        semantic_key: &str,
        file: &str,
        confidence: f64,
        instruction: &str,
    ) -> Finding {
        Finding {
            id: id.to_string(),
            angle_id: Some("general".to_string()),
            scope: FindingScope::Line,
            severity: Severity::P1,
            confidence,
            classification: FindingClassification::Defect,
            evidence_gate_result: EvidenceGateResult::Passed,
            blocking_reason: Some(BlockingReason::ValidatedDefect),
            grounding: Some(FindingGrounding {
                semantic_key: semantic_key.to_string(),
                resolution_disposition: None,
                resolution_evidence_summary: None,
                claim: "The changed configuration omits a required permission.".to_string(),
                causal_path: "workflow job -> package publication".to_string(),
                test_assessment: "No test covers the permission boundary.".to_string(),
                evidence: vec![],
                related_tests: vec![],
                reproduction: Some("Run the publication job.".to_string()),
                proof: None,
                novelty_evidence: None,
                reopening_evidence: None,
            }),
            file: Some(file.to_string()),
            line: Some(12),
            title: "Required permission is missing".to_string(),
            detail: None,
            agent_instruction: instruction.to_string(),
        }
    }

    fn tracked(
        finding: Finding,
        disposition: FindingDisposition,
        reviewed_sha: &str,
    ) -> TrackedFinding {
        let semantic_fingerprint = semantic_fingerprint(&finding);
        TrackedFinding {
            semantic_fingerprint,
            finding,
            disposition,
            disposition_history: vec![FindingDispositionRecord {
                disposition,
                submitted_disposition: None,
                evidence_summary: "Maintainer checked the repository contract.".to_string(),
                actor: "maintainer".to_string(),
                reviewed_sha: reviewed_sha.to_string(),
                code_fingerprint: "code-v1".to_string(),
            }],
        }
    }

    #[test]
    fn unchanged_head_is_idempotent_even_when_the_model_changes_its_mind() {
        let previous_finding = blocker(
            "general:permission",
            "workflow.package_permission",
            ".github/workflows/release.yml",
            0.99,
            "Add packages: write.",
        );
        let previous = tracked(
            previous_finding.clone(),
            FindingDisposition::StillOpen,
            "sha-1",
        );
        let contradictory = blocker(
            "general:permission-reworded",
            "workflow.package_permission",
            ".github/workflows/release.yml",
            0.99,
            "Remove packages: write.",
        );
        let surprise = blocker(
            "general:surprise",
            "workflow.unrelated_surprise",
            ".github/workflows/ci.yml",
            1.0,
            "Change an unrelated workflow.",
        );

        let result = reconcile_findings(
            vec![contradictory, surprise],
            &[previous],
            &ConvergenceDelta::unchanged("sha-1"),
        )
        .expect("unchanged reconciliation succeeds");

        assert_eq!(result.findings, vec![previous_finding]);
        assert_eq!(result.tracked_findings.len(), 1);
        assert_eq!(
            result.tracked_findings[0].disposition,
            FindingDisposition::StillOpen
        );
    }

    #[test]
    fn rejected_semantic_finding_stays_suppressed_until_relevant_evidence_changes() {
        let rejected = tracked(
            blocker(
                "general:permission",
                "workflow.package_permission",
                ".github/workflows/release.yml",
                0.99,
                "Add packages: write.",
            ),
            FindingDisposition::RejectedWithEvidence,
            "sha-1",
        );
        let recurrence = blocker(
            "adversarial:permission",
            "workflow.package_permission",
            ".github/workflows/release.yml",
            1.0,
            "Add a package permission already granted at job scope.",
        );

        let unrelated_delta =
            ConvergenceDelta::head_changed("sha-1", "sha-2", [String::from("README.md")]);
        let suppressed = reconcile_findings(
            vec![recurrence.clone()],
            std::slice::from_ref(&rejected),
            &unrelated_delta,
        )
        .expect("unrelated delta reconciles");
        assert!(suppressed.findings.is_empty());
        assert_eq!(
            suppressed.tracked_findings[0].disposition,
            FindingDisposition::RejectedWithEvidence
        );

        let mut reopened = recurrence;
        reopened
            .grounding
            .as_mut()
            .expect("grounding")
            .reopening_evidence =
            Some("The job-level packages grant was removed in this delta.".to_string());
        let relevant_delta = ConvergenceDelta::head_changed(
            "sha-1",
            "sha-3",
            [String::from(".github/workflows/release.yml")],
        );
        let reopened = reconcile_findings(vec![reopened], &[rejected], &relevant_delta)
            .expect("relevant delta reconciles");
        assert_eq!(reopened.findings.len(), 1);
        assert_eq!(
            reopened.tracked_findings[0].disposition,
            FindingDisposition::StillOpen
        );
    }

    #[test]
    fn late_blocker_requires_higher_confidence_and_novelty_evidence() {
        let prior = tracked(
            blocker(
                "general:existing",
                "parser.existing_defect",
                "src/parser.rs",
                0.99,
                "Fix the parser.",
            ),
            FindingDisposition::Fixed,
            "sha-1",
        );
        let mut late = blocker(
            "general:late",
            "installer.new_defect",
            "src/installer.rs",
            0.94,
            "Fix the installer.",
        );
        let delta =
            ConvergenceDelta::head_changed("sha-1", "sha-2", [String::from("src/parser.rs")]);

        let below_threshold =
            reconcile_findings(vec![late.clone()], std::slice::from_ref(&prior), &delta)
                .expect("late finding reconciles");
        assert!(below_threshold.findings.is_empty());

        late.confidence = LATE_BLOCKER_CONFIDENCE_THRESHOLD;
        let missing_novelty =
            reconcile_findings(vec![late.clone()], std::slice::from_ref(&prior), &delta)
                .expect("late finding reconciles");
        assert!(missing_novelty.findings.is_empty());

        late.grounding
            .as_mut()
            .expect("grounding")
            .novelty_evidence = Some(
            "The installer defect is introduced by the current parser fix and did not exist at sha-1."
                .to_string(),
        );
        let accepted =
            reconcile_findings(vec![late], &[prior], &delta).expect("late finding reconciles");
        assert_eq!(accepted.findings.len(), 1);
        assert_eq!(
            accepted.tracked_findings[0].disposition,
            FindingDisposition::StillOpen
        );
    }

    #[test]
    fn reviewer_omission_does_not_mark_a_changed_finding_fixed() {
        let prior_finding = blocker(
            "general:parser",
            "parser.positional_operand",
            "src/parser.rs",
            0.99,
            "Remove the positional operand.",
        );
        let prior = tracked(prior_finding, FindingDisposition::StillOpen, "sha-1");
        let delta =
            ConvergenceDelta::head_changed("sha-1", "sha-2", [String::from("src/parser.rs")]);

        let result =
            reconcile_findings(vec![], &[prior], &delta).expect("fixed finding reconciles");

        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.tracked_findings.len(), 1);
        assert_eq!(
            result.tracked_findings[0].disposition,
            FindingDisposition::StillOpen
        );
    }

    #[test]
    fn same_identity_recurrence_cannot_downgrade_an_open_blocker() {
        let prior_finding = blocker(
            "general:parser",
            "parser.positional_operand",
            "src/parser.rs",
            0.99,
            "Remove the positional operand.",
        );
        let prior = tracked(
            prior_finding.clone(),
            FindingDisposition::StillOpen,
            "sha-1",
        );
        let mut weaker = prior_finding;
        weaker.severity = Severity::P4;
        weaker.confidence = 0.25;
        weaker.evidence_gate_result = EvidenceGateResult::NotRequired;
        weaker.blocking_reason = None;
        let delta =
            ConvergenceDelta::head_changed("sha-1", "sha-2", [String::from("src/parser.rs")]);

        let result =
            reconcile_findings(vec![weaker], &[prior], &delta).expect("recurrence reconciles");

        assert_eq!(result.findings[0].severity, Severity::P1);
        assert_eq!(result.findings[0].confidence, 0.99);
        assert_eq!(
            result.findings[0].evidence_gate_result,
            EvidenceGateResult::Passed
        );
        assert_eq!(
            result.findings[0].blocking_reason,
            Some(BlockingReason::ValidatedDefect)
        );
    }

    #[test]
    fn explicit_evidence_backed_update_marks_an_open_finding_fixed() {
        let prior_finding = blocker(
            "general:parser",
            "parser.positional_operand",
            "src/parser.rs",
            0.99,
            "Remove the positional operand.",
        );
        let prior = tracked(prior_finding, FindingDisposition::StillOpen, "sha-1");
        let mut fixed_evidence = blocker(
            "general:parser-fixed",
            "parser.positional_operand",
            "src/parser.rs",
            0.99,
            "The parser now rejects positional operands.",
        );
        let grounding = fixed_evidence.grounding.as_mut().expect("grounding");
        grounding.causal_path = "parser guard -> rejected positional operand".to_string();
        grounding.resolution_disposition = Some(FindingDisposition::Fixed);
        grounding.resolution_evidence_summary =
            Some("The new parser guard rejects the prior reproduction.".to_string());
        let update = FindingDispositionUpdate {
            semantic_fingerprint: prior.semantic_fingerprint.clone(),
            disposition: FindingDisposition::Fixed,
            evidence_summary: "The new parser guard rejects the prior reproduction.".to_string(),
            actor: "reviewgate".to_string(),
            reviewed_sha: "sha-2".to_string(),
            code_fingerprint: finding_code_fingerprint(&fixed_evidence),
            resolution: fixed_evidence,
        };
        let delta =
            ConvergenceDelta::head_changed("sha-1", "sha-2", [String::from("src/parser.rs")]);

        let result = reconcile_findings_with_updates(vec![], &[prior], &delta, &[update])
            .expect("validated update reconciles");

        assert!(result.findings.is_empty());
        assert_eq!(
            result.tracked_findings[0].disposition,
            FindingDisposition::Fixed
        );
        assert_eq!(
            result.tracked_findings[0]
                .disposition_history
                .last()
                .expect("latest record")
                .actor,
            "reviewgate"
        );
    }

    #[test]
    fn fixed_update_requires_the_prior_findings_relevant_file_to_change() {
        let prior_finding = blocker(
            "general:parser",
            "parser.positional_operand",
            "src/parser.rs",
            0.99,
            "Remove the positional operand.",
        );
        let prior = tracked(prior_finding, FindingDisposition::StillOpen, "sha-1");
        let mut fixed_evidence = prior.finding.clone();
        let grounding = fixed_evidence.grounding.as_mut().expect("grounding");
        grounding.causal_path = "parser guard -> rejected positional operand".to_string();
        grounding.resolution_disposition = Some(FindingDisposition::Fixed);
        grounding.resolution_evidence_summary =
            Some("The parser guard rejects the prior reproduction.".to_string());
        let update = FindingDispositionUpdate {
            semantic_fingerprint: prior.semantic_fingerprint.clone(),
            disposition: FindingDisposition::Fixed,
            evidence_summary: "The parser guard rejects the prior reproduction.".to_string(),
            actor: "reviewgate".to_string(),
            reviewed_sha: "sha-2".to_string(),
            code_fingerprint: finding_code_fingerprint(&fixed_evidence),
            resolution: fixed_evidence,
        };
        let delta =
            ConvergenceDelta::head_changed("sha-1", "sha-2", [String::from("docs/parser.md")]);

        let error = reconcile_findings_with_updates(vec![], &[prior], &delta, &[update])
            .expect_err("unrelated changes cannot fix the finding");

        assert!(
            error
                .to_string()
                .contains("is not justified by the current delta")
        );
    }

    #[test]
    fn unrelated_external_contract_change_does_not_clear_an_open_finding() {
        let prior = tracked(
            blocker(
                "general:parser",
                "parser.positional_operand",
                "src/parser.rs",
                0.99,
                "Remove the positional operand.",
            ),
            FindingDisposition::StillOpen,
            "sha-1",
        );
        let mut delta =
            ConvergenceDelta::head_changed("sha-1", "sha-2", [String::from("README.md")]);
        delta.external_contract_changed = true;

        let result = reconcile_findings(vec![], &[prior], &delta)
            .expect("external contract delta reconciles");

        assert_eq!(result.findings.len(), 1);
        assert_eq!(
            result.tracked_findings[0].disposition,
            FindingDisposition::StillOpen
        );
    }

    #[test]
    fn completed_findings_keep_review_status_derivable_from_the_reconciled_set() {
        let result = reconcile_findings(
            vec![blocker(
                "general:blocker",
                "parser.real_defect",
                "src/parser.rs",
                1.0,
                "Fix it.",
            )],
            &[],
            &ConvergenceDelta::first_review("sha-1"),
        )
        .expect("first review reconciles");

        let status = if result.findings.is_empty() {
            ReviewStatus::Passed
        } else {
            ReviewStatus::NeedsChanges
        };
        assert_eq!(status, ReviewStatus::NeedsChanges);
    }

    #[test]
    fn recorded_pr364_and_pr365_sequences_converge_without_reopening_rejected_advice() {
        let fixture: ConvergenceFixture = serde_json::from_str(include_str!(
            "../../../fixtures/convergence/regressions.json"
        ))
        .expect("convergence fixture parses");

        let mut tracked_findings: Vec<TrackedFinding> = Vec::new();
        for round in fixture.pr364_five_passes {
            for semantic_key in &round.fixed_semantic_keys {
                let tracked = tracked_findings
                    .iter_mut()
                    .find(|tracked| {
                        tracked
                            .finding
                            .grounding
                            .as_ref()
                            .is_some_and(|grounding| grounding.semantic_key == *semantic_key)
                    })
                    .expect("fixture disposition references a tracked finding");
                tracked.disposition = FindingDisposition::Fixed;
                tracked.disposition_history.push(FindingDispositionRecord {
                    disposition: FindingDisposition::Fixed,
                    submitted_disposition: None,
                    evidence_summary: "The repair agent verified the finding against the new head."
                        .to_string(),
                    actor: "repair-agent".to_string(),
                    reviewed_sha: round.sha.clone(),
                    code_fingerprint: finding_code_fingerprint(&tracked.finding),
                });
            }
            let current = round
                .findings
                .into_iter()
                .map(|fixture| {
                    let mut finding = blocker(
                        &fixture.id,
                        &fixture.semantic_key,
                        &fixture.file,
                        fixture.confidence,
                        &fixture.instruction,
                    );
                    finding
                        .grounding
                        .as_mut()
                        .expect("grounding")
                        .novelty_evidence = fixture.novelty_evidence;
                    finding
                })
                .collect();
            let delta = match round.previous_sha {
                Some(previous_sha) => {
                    ConvergenceDelta::head_changed(previous_sha, &round.sha, round.changed_files)
                }
                None => ConvergenceDelta::first_review(&round.sha),
            };
            let result = reconcile_findings(current, &tracked_findings, &delta)
                .expect("PR 364 round reconciles");
            assert_eq!(
                result.findings.len(),
                round.expected_open,
                "unexpected open set at {}",
                round.sha
            );
            tracked_findings = result.tracked_findings;
        }
        assert!(
            tracked_findings
                .iter()
                .all(|tracked| { tracked.disposition != FindingDisposition::StillOpen })
        );

        let recurrence = fixture.pr365_rejected_permission_recurrence;
        let seed = tracked(
            blocker(
                "general:package-permission",
                &recurrence.semantic_key,
                &recurrence.file,
                1.0,
                "Add packages: write.",
            ),
            FindingDisposition::RejectedWithEvidence,
            &recurrence.seed_sha,
        );
        let mut prior_sha = recurrence.seed_sha;
        let mut tracked_findings = vec![seed];
        for round in recurrence.rounds {
            let current = blocker(
                &round.finding_id,
                &recurrence.semantic_key,
                &recurrence.file,
                1.0,
                "Add a permission that the effective job already grants.",
            );
            let delta = ConvergenceDelta::head_changed(&prior_sha, &round.sha, round.changed_files);
            let result = reconcile_findings(vec![current], &tracked_findings, &delta)
                .expect("PR 365 recurrence reconciles");
            assert_eq!(result.findings.len(), round.expected_open);
            assert_eq!(
                result.tracked_findings[0].disposition,
                FindingDisposition::RejectedWithEvidence
            );
            prior_sha = round.sha;
            tracked_findings = result.tracked_findings;
        }
    }
}
