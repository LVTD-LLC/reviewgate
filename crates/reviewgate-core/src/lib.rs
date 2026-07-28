use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const SUMMARY_MARKER: &str = "<!-- reviewgate-summary -->";
pub const SUMMARY_STATE_PREFIX: &str = "<!-- reviewgate-state ";
pub const SUMMARY_STATE_SUFFIX: &str = " -->";
pub const DEFAULT_COST_HISTORY_LIMIT: usize = 20;
pub const DEFAULT_TARGET_SCORE: u8 = 5;
pub const OPENROUTER_API_KEY_ENV: &str = "OPENROUTER_API_KEY";
pub const OPENROUTER_CHAT_COMPLETIONS_PATH: &str = "/chat/completions";
pub const OPENROUTER_DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";
pub const OPENROUTER_MODELS_PATH: &str = "/models";
pub const OPENROUTER_APP_REFERER: &str = "https://github.com/LVTD-LLC/reviewgate";
pub const OPENROUTER_APP_TITLE: &str = "ReviewGate";
pub const OPENROUTER_APP_CATEGORIES: &str = "cli-agent,cloud-agent";

fn default_target_score() -> u8 {
    DEFAULT_TARGET_SCORE
}

#[derive(Debug, Error)]
pub enum ReviewGateError {
    #[error("score must be between 0 and 5, got {0}")]
    InvalidScore(u8),
    #[error("confidence must be between 0 and 1, got {0}")]
    InvalidConfidence(f64),
    #[error("estimated cost must be finite and non-negative, got {0}")]
    InvalidEstimatedCost(f64),
    #[error("cost component {field} must not be empty")]
    InvalidCostComponent { field: &'static str },
    #[error("review angle {field} must not be empty")]
    InvalidReviewAngle { field: &'static str },
    #[error("review outcome is invalid: {0}")]
    InvalidReviewOutcome(String),
    #[error("invalid severity {0:?}; expected P0, P1, P2, P3, or P4")]
    InvalidSeverity(String),
    #[error("summary state is invalid: {0}")]
    InvalidSummaryState(String),
    #[error("model pricing is invalid: {0}")]
    InvalidModelPricing(String),
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    Passed,
    #[serde(alias = "failed")]
    NeedsChanges,
    ReviewError,
}

impl ReviewStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReviewStatus::Passed => "passed",
            ReviewStatus::NeedsChanges => "needs_changes",
            ReviewStatus::ReviewError => "review_error",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewErrorKind {
    Timeout,
    MalformedResponse,
    EmptyResponse,
    ProviderError,
    TransportError,
}

impl ReviewErrorKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReviewErrorKind::Timeout => "timeout",
            ReviewErrorKind::MalformedResponse => "malformed_response",
            ReviewErrorKind::EmptyResponse => "empty_response",
            ReviewErrorKind::ProviderError => "provider_error",
            ReviewErrorKind::TransportError => "transport_error",
        }
    }

    pub fn public_message(&self) -> &'static str {
        match self {
            ReviewErrorKind::Timeout => "The reviewer request timed out.",
            ReviewErrorKind::MalformedResponse => {
                "The reviewer returned an invalid structured response."
            }
            ReviewErrorKind::EmptyResponse => "The reviewer returned an empty response.",
            ReviewErrorKind::ProviderError => {
                "The reviewer provider rejected or could not complete the request."
            }
            ReviewErrorKind::TransportError => "The reviewer transport failed.",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReviewAngleError {
    pub angle_id: String,
    pub angle_name: String,
    pub kind: ReviewErrorKind,
    pub retryable: bool,
    pub message: String,
    pub model: String,
}

impl ReviewAngleError {
    pub fn validate(&self) -> Result<(), ReviewGateError> {
        for (field, value) in [
            ("error.angle_id", self.angle_id.as_str()),
            ("error.angle_name", self.angle_name.as_str()),
            ("error.message", self.message.as_str()),
            ("error.model", self.model.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(ReviewGateError::InvalidReviewAngle { field });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    P0,
    P1,
    P2,
    P3,
    P4,
}

impl Severity {
    pub fn parse(value: &str) -> Result<Self, ReviewGateError> {
        match value.trim().to_ascii_uppercase().as_str() {
            "P0" => Ok(Severity::P0),
            "P1" => Ok(Severity::P1),
            "P2" => Ok(Severity::P2),
            "P3" => Ok(Severity::P3),
            "P4" => Ok(Severity::P4),
            _ => Err(ReviewGateError::InvalidSeverity(value.to_string())),
        }
    }

    pub fn score_ceiling(&self) -> u8 {
        match self {
            Severity::P0 => 1,
            Severity::P1 => 2,
            Severity::P2 => 3,
            Severity::P3 => 4,
            Severity::P4 => 5,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::P0 => "P0",
            Severity::P1 => "P1",
            Severity::P2 => "P2",
            Severity::P3 => "P3",
            Severity::P4 => "P4",
        }
    }

    pub fn is_at_or_above(&self, floor: Severity) -> bool {
        *self <= floor
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FindingScope {
    #[default]
    Line,
    File,
    Pr,
}

impl FindingScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            FindingScope::Line => "line",
            FindingScope::File => "file",
            FindingScope::Pr => "pr",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Finding {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle_id: Option<String>,
    #[serde(default)]
    pub scope: FindingScope,
    pub severity: Severity,
    pub confidence: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grounding: Option<FindingGrounding>,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub title: String,
    pub detail: Option<String>,
    pub agent_instruction: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct FindingGrounding {
    pub claim: String,
    pub causal_path: String,
    pub test_assessment: String,
    #[serde(default)]
    pub evidence: Vec<FindingEvidence>,
    #[serde(default)]
    pub related_tests: Vec<FindingEvidence>,
    #[serde(default)]
    pub reproduction: Option<String>,
    #[serde(default)]
    pub proof: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct FindingEvidence {
    pub path: String,
    #[serde(default)]
    pub side: FindingEvidenceSide,
    pub line: u32,
    pub excerpt: String,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum FindingEvidenceSide {
    #[default]
    New,
    Old,
}

impl Finding {
    pub fn is_blocking(&self, target_score: u8) -> bool {
        self.severity.score_ceiling() < target_score
    }

    pub fn validate(&self) -> Result<(), ReviewGateError> {
        validate_confidence(self.confidence)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct CostComponent {
    pub label: String,
    pub model: String,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub estimated_cost_usd: f64,
}

impl CostComponent {
    pub fn validate(&self) -> Result<(), ReviewGateError> {
        if self.label.trim().is_empty() {
            return Err(ReviewGateError::InvalidCostComponent { field: "label" });
        }
        if self.model.trim().is_empty() {
            return Err(ReviewGateError::InvalidCostComponent { field: "model" });
        }
        validate_estimated_cost(self.estimated_cost_usd)
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CostSource {
    OpenRouterUsage,
    FallbackPricing,
    Unknown,
}

impl CostSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            CostSource::OpenRouterUsage => "open_router_usage",
            CostSource::FallbackPricing => "fallback_pricing",
            CostSource::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ReviewMetrics {
    pub finding_count: u32,
    pub blocking_finding_count: u32,
    pub inline_eligible_count: u32,
    pub p0_count: u32,
    pub p1_count: u32,
    pub p2_count: u32,
    pub p3_count: u32,
    pub p4_count: u32,
    pub analyzed_line_count: Option<u32>,
    pub current_run_cost_usd: Option<f64>,
    pub cost_source: CostSource,
}

impl ReviewMetrics {
    pub fn validate(&self) -> Result<(), ReviewGateError> {
        if let Some(cost) = self.current_run_cost_usd {
            validate_estimated_cost(cost)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct CostSummary {
    pub current_run_usd: f64,
    pub components: Vec<CostComponent>,
    #[serde(default)]
    pub source: Option<CostSource>,
}

impl CostSummary {
    pub fn validate(&self) -> Result<(), ReviewGateError> {
        validate_estimated_cost(self.current_run_usd)?;
        for component in &self.components {
            component.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ReviewStage {
    pub name: String,
    pub model: String,
    pub status: String,
    pub reason: String,
    pub estimated_cost_usd: Option<f64>,
}

impl ReviewStage {
    pub fn validate(&self) -> Result<(), ReviewGateError> {
        if self.name.trim().is_empty() {
            return Err(ReviewGateError::InvalidCostComponent {
                field: "stage.name",
            });
        }
        if self.model.trim().is_empty() {
            return Err(ReviewGateError::InvalidCostComponent {
                field: "stage.model",
            });
        }
        if self.status.trim().is_empty() {
            return Err(ReviewGateError::InvalidCostComponent {
                field: "stage.status",
            });
        }
        if self.reason.trim().is_empty() {
            return Err(ReviewGateError::InvalidCostComponent {
                field: "stage.reason",
            });
        }
        if let Some(cost) = self.estimated_cost_usd {
            validate_estimated_cost(cost)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ReviewAngleResult {
    pub id: String,
    pub name: String,
    pub score: u8,
    pub status: ReviewStatus,
    pub verdict: String,
    pub model: String,
    pub finding_ids: Vec<String>,
}

impl ReviewAngleResult {
    pub fn validate(&self) -> Result<(), ReviewGateError> {
        if self.id.trim().is_empty() {
            return Err(ReviewGateError::InvalidReviewAngle { field: "id" });
        }
        if self.name.trim().is_empty() {
            return Err(ReviewGateError::InvalidReviewAngle { field: "name" });
        }
        validate_score(self.score)?;
        if self.status == ReviewStatus::ReviewError {
            return Err(ReviewGateError::InvalidReviewOutcome(format!(
                "successful angle {} cannot have review_error status",
                self.id
            )));
        }
        if self.verdict.trim().is_empty() {
            return Err(ReviewGateError::InvalidReviewAngle { field: "verdict" });
        }
        if self.model.trim().is_empty() {
            return Err(ReviewGateError::InvalidReviewAngle { field: "model" });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ReviewArtifact {
    pub score: Option<u8>,
    #[serde(default = "default_target_score", skip_serializing)]
    pub target_score: u8,
    pub reviewed_sha: String,
    pub status: ReviewStatus,
    pub verdict: String,
    pub models: Vec<String>,
    pub estimated_cost_usd: Option<f64>,
    #[serde(default)]
    pub cost_summary: Option<CostSummary>,
    #[serde(default)]
    pub metrics: Option<ReviewMetrics>,
    #[serde(default)]
    pub review_stages: Vec<ReviewStage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub angle_results: Vec<ReviewAngleResult>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub angle_errors: Vec<ReviewAngleError>,
    pub findings: Vec<Finding>,
    pub notes: Vec<String>,
}

impl ReviewArtifact {
    pub fn validate(&self) -> Result<(), ReviewGateError> {
        if self.reviewed_sha.trim().is_empty() {
            return Err(ReviewGateError::InvalidReviewOutcome(
                "reviewed_sha must not be empty".to_string(),
            ));
        }
        if let Some(score) = self.score {
            validate_score(score)?;
        }
        validate_score(self.target_score)?;
        match self.status {
            ReviewStatus::ReviewError => {
                if self.score.is_some() {
                    return Err(ReviewGateError::InvalidReviewOutcome(
                        "review_error must not expose a numeric score".to_string(),
                    ));
                }
                if self.angle_errors.is_empty() {
                    return Err(ReviewGateError::InvalidReviewOutcome(
                        "review_error must include at least one angle error".to_string(),
                    ));
                }
            }
            ReviewStatus::Passed | ReviewStatus::NeedsChanges => {
                let Some(score) = self.score else {
                    return Err(ReviewGateError::InvalidReviewOutcome(
                        "completed review must expose a numeric score".to_string(),
                    ));
                };
                if !self.angle_errors.is_empty() {
                    return Err(ReviewGateError::InvalidReviewOutcome(
                        "angle errors require review_error status".to_string(),
                    ));
                }
                let expected_score = compute_score(&self.findings);
                if score != expected_score {
                    return Err(ReviewGateError::InvalidReviewOutcome(format!(
                        "score {score} does not match derived score {expected_score}"
                    )));
                }
                let expected_status = status_for_score(expected_score);
                if self.status != expected_status {
                    return Err(ReviewGateError::InvalidReviewOutcome(format!(
                        "status {} does not match derived status {}",
                        self.status.as_str(),
                        expected_status.as_str()
                    )));
                }
            }
        }
        if let Some(cost) = self.estimated_cost_usd {
            validate_estimated_cost(cost)?;
        }
        if let Some(cost_summary) = &self.cost_summary {
            cost_summary.validate()?;
        }
        if let Some(metrics) = &self.metrics {
            metrics.validate()?;
        }
        for stage in &self.review_stages {
            stage.validate()?;
        }
        self.validate_angle_outcomes()?;
        for error in &self.angle_errors {
            error.validate()?;
        }
        self.validate_angle_errors()?;
        for finding in &self.findings {
            finding.validate()?;
        }
        Ok(())
    }

    fn validate_angle_outcomes(&self) -> Result<(), ReviewGateError> {
        let mut finding_ids = BTreeSet::new();
        for finding in &self.findings {
            if !finding_ids.insert(finding.id.as_str()) {
                return Err(ReviewGateError::InvalidReviewOutcome(format!(
                    "duplicate finding id {}",
                    finding.id
                )));
            }
        }

        let mut angle_ids = BTreeSet::new();
        for angle in &self.angle_results {
            angle.validate()?;
            if !angle_ids.insert(angle.id.as_str()) {
                return Err(ReviewGateError::InvalidReviewOutcome(format!(
                    "duplicate review angle id {}",
                    angle.id
                )));
            }

            let mut referenced_ids = BTreeSet::new();
            let mut angle_findings = Vec::new();
            for finding_id in &angle.finding_ids {
                if !referenced_ids.insert(finding_id.as_str()) {
                    return Err(ReviewGateError::InvalidReviewOutcome(format!(
                        "review angle {} repeats finding id {finding_id}",
                        angle.id
                    )));
                }
                let finding = self
                    .findings
                    .iter()
                    .find(|finding| finding.id == *finding_id)
                    .ok_or_else(|| {
                        ReviewGateError::InvalidReviewOutcome(format!(
                            "review angle {} references unknown finding id {finding_id}",
                            angle.id
                        ))
                    })?;
                if finding.angle_id.as_deref() != Some(angle.id.as_str()) {
                    return Err(ReviewGateError::InvalidReviewOutcome(format!(
                        "finding {finding_id} is not owned by review angle {}",
                        angle.id
                    )));
                }
                angle_findings.push(finding.clone());
            }
            if let Some(omitted) = self.findings.iter().find(|finding| {
                finding.angle_id.as_deref() == Some(angle.id.as_str())
                    && !referenced_ids.contains(finding.id.as_str())
            }) {
                return Err(ReviewGateError::InvalidReviewOutcome(format!(
                    "review angle {} omits finding id {}",
                    angle.id, omitted.id
                )));
            }

            let expected_score = compute_score(&angle_findings);
            let expected_status = status_for_score(expected_score);
            if angle.score != expected_score || angle.status != expected_status {
                return Err(ReviewGateError::InvalidReviewOutcome(format!(
                    "review angle {} outcome does not match its findings",
                    angle.id
                )));
            }
        }
        if let Some(orphaned) = self.findings.iter().find(|finding| {
            finding
                .angle_id
                .as_deref()
                .is_some_and(|angle_id| !angle_ids.contains(angle_id))
        }) {
            return Err(ReviewGateError::InvalidReviewOutcome(format!(
                "finding {} is owned by an unknown review angle",
                orphaned.id
            )));
        }
        Ok(())
    }

    fn validate_angle_errors(&self) -> Result<(), ReviewGateError> {
        let successful_angle_ids = self
            .angle_results
            .iter()
            .map(|angle| angle.id.as_str())
            .collect::<BTreeSet<_>>();
        let mut failed_angle_ids = BTreeSet::new();
        for error in &self.angle_errors {
            if !failed_angle_ids.insert(error.angle_id.as_str()) {
                return Err(ReviewGateError::InvalidReviewOutcome(format!(
                    "duplicate review angle error id {}",
                    error.angle_id
                )));
            }
            if successful_angle_ids.contains(error.angle_id.as_str()) {
                return Err(ReviewGateError::InvalidReviewOutcome(format!(
                    "review angle {} cannot both succeed and fail",
                    error.angle_id
                )));
            }
        }
        Ok(())
    }

    pub fn with_computed_score(mut self) -> Result<Self, ReviewGateError> {
        self.target_score = DEFAULT_TARGET_SCORE;
        if self.angle_errors.is_empty() {
            let score = compute_effective_score(&self.findings, &self.angle_results);
            self.score = Some(score);
            self.status = status_for_score(score);
        } else {
            self.score = None;
            self.status = ReviewStatus::ReviewError;
        }
        self.validate()?;
        Ok(self)
    }

    pub fn prepared_for_publication(mut self, current_head_sha: &str) -> Self {
        if self.reviewed_sha != current_head_sha || self.validate().is_err() {
            return Self::artifact_validation_error(current_head_sha);
        }
        if self.status == ReviewStatus::ReviewError {
            self.verdict = "ReviewGate could not complete every enabled review angle.".to_string();
            self.notes.clear();
            for error in &mut self.angle_errors {
                error.message = if error.angle_id == "artifact_validation" {
                    "The review artifact failed deterministic validation.".to_string()
                } else {
                    error.kind.public_message().to_string()
                };
            }
        }
        self
    }

    fn artifact_validation_error(current_head_sha: &str) -> Self {
        Self {
            score: None,
            target_score: DEFAULT_TARGET_SCORE,
            reviewed_sha: current_head_sha.to_string(),
            status: ReviewStatus::ReviewError,
            verdict: "ReviewGate rejected an internally inconsistent review artifact.".to_string(),
            models: vec!["reviewgate".to_string()],
            estimated_cost_usd: None,
            cost_summary: None,
            metrics: None,
            review_stages: vec![],
            angle_results: vec![],
            angle_errors: vec![ReviewAngleError {
                angle_id: "artifact_validation".to_string(),
                angle_name: "Artifact validation".to_string(),
                kind: ReviewErrorKind::MalformedResponse,
                retryable: false,
                message: "The review artifact failed deterministic validation.".to_string(),
                model: "reviewgate".to_string(),
            }],
            findings: vec![],
            notes: vec![],
        }
    }
}

pub fn validate_score(score: u8) -> Result<(), ReviewGateError> {
    if score <= 5 {
        Ok(())
    } else {
        Err(ReviewGateError::InvalidScore(score))
    }
}

pub fn validate_estimated_cost(cost: f64) -> Result<(), ReviewGateError> {
    if cost.is_finite() && cost >= 0.0 {
        Ok(())
    } else {
        Err(ReviewGateError::InvalidEstimatedCost(cost))
    }
}

fn validate_confidence(confidence: f64) -> Result<(), ReviewGateError> {
    if (0.0..=1.0).contains(&confidence) {
        Ok(())
    } else {
        Err(ReviewGateError::InvalidConfidence(confidence))
    }
}

pub fn compute_score(findings: &[Finding]) -> u8 {
    findings
        .iter()
        .map(|finding| finding.severity.score_ceiling())
        .min()
        .unwrap_or(5)
}

pub fn compute_effective_score(findings: &[Finding], angle_results: &[ReviewAngleResult]) -> u8 {
    let finding_score = compute_score(findings);
    // Angle results contain only successfully completed reviews. Their scores are validated
    // against their referenced findings before an artifact can be published.
    angle_results
        .iter()
        .map(|angle| angle.score)
        .min()
        .map_or(finding_score, |angle_score| angle_score.min(finding_score))
}

pub fn status_for_score(score: u8) -> ReviewStatus {
    if score >= DEFAULT_TARGET_SCORE {
        ReviewStatus::Passed
    } else {
        ReviewStatus::NeedsChanges
    }
}

pub fn compute_metrics(artifact: &ReviewArtifact, min_severity: Severity) -> ReviewMetrics {
    let mut metrics = ReviewMetrics {
        finding_count: artifact.findings.len() as u32,
        blocking_finding_count: 0,
        inline_eligible_count: 0,
        p0_count: 0,
        p1_count: 0,
        p2_count: 0,
        p3_count: 0,
        p4_count: 0,
        analyzed_line_count: artifact
            .metrics
            .as_ref()
            .and_then(|metrics| metrics.analyzed_line_count),
        current_run_cost_usd: artifact
            .cost_summary
            .as_ref()
            .map(|summary| summary.current_run_usd)
            .or(artifact.estimated_cost_usd),
        cost_source: artifact
            .cost_summary
            .as_ref()
            .and_then(|summary| summary.source)
            .unwrap_or(CostSource::Unknown),
    };

    for finding in &artifact.findings {
        if finding.is_blocking(DEFAULT_TARGET_SCORE) {
            metrics.blocking_finding_count += 1;
        }
        if is_inline_comment_eligible(finding, min_severity) {
            metrics.inline_eligible_count += 1;
        }
        match finding.severity {
            Severity::P0 => metrics.p0_count += 1,
            Severity::P1 => metrics.p1_count += 1,
            Severity::P2 => metrics.p2_count += 1,
            Severity::P3 => metrics.p3_count += 1,
            Severity::P4 => metrics.p4_count += 1,
        }
    }

    metrics
}

pub fn is_inline_comment_eligible(finding: &Finding, min_severity: Severity) -> bool {
    finding.severity.is_at_or_above(min_severity)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelPricing {
    pub prompt_usd_per_million: f64,
    pub completion_usd_per_million: f64,
}

impl ModelPricing {
    pub fn estimate_cost_usd(
        &self,
        prompt_tokens: u64,
        completion_tokens: u64,
    ) -> Result<f64, ReviewGateError> {
        validate_estimated_cost(self.prompt_usd_per_million)?;
        validate_estimated_cost(self.completion_usd_per_million)?;
        Ok(
            (prompt_tokens as f64 / 1_000_000.0) * self.prompt_usd_per_million
                + (completion_tokens as f64 / 1_000_000.0) * self.completion_usd_per_million,
        )
    }
}

pub fn fallback_model_pricing(model: &str) -> Option<ModelPricing> {
    match model {
        "deepseek/deepseek-v4-flash" => Some(ModelPricing {
            prompt_usd_per_million: 0.09,
            completion_usd_per_million: 0.18,
        }),
        "qwen/qwen3-coder" => Some(ModelPricing {
            prompt_usd_per_million: 0.20,
            completion_usd_per_million: 0.80,
        }),
        "anthropic/claude-sonnet-4" => Some(ModelPricing {
            prompt_usd_per_million: 3.00,
            completion_usd_per_million: 15.00,
        }),
        _ => None,
    }
}

pub fn estimate_model_cost_usd(
    model: &str,
    prompt_tokens: u64,
    completion_tokens: u64,
) -> Result<Option<f64>, ReviewGateError> {
    fallback_model_pricing(model)
        .map(|pricing| pricing.estimate_cost_usd(prompt_tokens, completion_tokens))
        .transpose()
}

pub fn parse_openrouter_model_pricing(
    models_response: &serde_json::Value,
    model: &str,
) -> Result<Option<ModelPricing>, ReviewGateError> {
    let Some(models) = models_response
        .get("data")
        .and_then(serde_json::Value::as_array)
    else {
        return Ok(None);
    };

    for entry in models {
        let Some(id) = entry.get("id").and_then(serde_json::Value::as_str) else {
            continue;
        };
        if id != model {
            continue;
        }
        let Some(pricing) = entry.get("pricing") else {
            return Ok(None);
        };
        let prompt = parse_openrouter_price(pricing.get("prompt"))?;
        let completion = parse_openrouter_price(pricing.get("completion"))?;
        return Ok(Some(ModelPricing {
            prompt_usd_per_million: prompt,
            completion_usd_per_million: completion,
        }));
    }

    Ok(None)
}

fn parse_openrouter_price(value: Option<&serde_json::Value>) -> Result<f64, ReviewGateError> {
    let Some(value) = value else {
        return Err(ReviewGateError::InvalidModelPricing(
            "missing pricing field".to_string(),
        ));
    };
    let price = if let Some(raw) = value.as_str() {
        raw.parse::<f64>()
            .map_err(|error| ReviewGateError::InvalidModelPricing(error.to_string()))?
    } else if let Some(raw) = value.as_f64() {
        raw
    } else {
        return Err(ReviewGateError::InvalidModelPricing(
            "pricing field must be a string or number".to_string(),
        ));
    };
    validate_estimated_cost(price)?;
    // OpenRouter's models API returns per-token USD prices as tiny values
    // such as 0.00000009. Checked-in fallback pricing is stored per 1M tokens,
    // so values at normal per-million scale are left unchanged.
    if price < 0.001 {
        Ok(price * 1_000_000.0)
    } else {
        Ok(price)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct SummaryCostRun {
    pub reviewed_sha: String,
    pub cost_usd: f64,
}

impl SummaryCostRun {
    pub fn validate(&self) -> Result<(), ReviewGateError> {
        if self.reviewed_sha.trim().is_empty() {
            return Err(ReviewGateError::InvalidSummaryState(
                "cost run reviewed_sha must not be empty".to_string(),
            ));
        }
        validate_estimated_cost(self.cost_usd)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct SummaryState {
    pub version: u8,
    pub last_reviewed_sha: String,
    #[serde(default)]
    pub last_valid_reviewed_sha: Option<String>,
    #[serde(default)]
    pub last_valid_score: Option<u8>,
    #[serde(default)]
    pub last_valid_status: Option<ReviewStatus>,
    pub reviewed_shas: Vec<String>,
    pub run_count: u32,
    pub cumulative_cost_usd: f64,
    pub cost_history: Vec<SummaryCostRun>,
}

impl SummaryState {
    pub fn for_artifact(
        artifact: &ReviewArtifact,
        previous: Option<&SummaryState>,
        history_limit: usize,
    ) -> Result<Self, ReviewGateError> {
        let current_cost = artifact
            .cost_summary
            .as_ref()
            .map(|cost| cost.current_run_usd)
            .or(artifact.estimated_cost_usd)
            .unwrap_or(0.0);
        validate_estimated_cost(current_cost)?;

        let mut reviewed_shas = previous
            .map(|state| state.reviewed_shas.clone())
            .unwrap_or_default();
        if !reviewed_shas.contains(&artifact.reviewed_sha) {
            reviewed_shas.push(artifact.reviewed_sha.clone());
        }

        let mut cost_history = previous
            .map(|state| state.cost_history.clone())
            .unwrap_or_default();
        cost_history.push(SummaryCostRun {
            reviewed_sha: artifact.reviewed_sha.clone(),
            cost_usd: current_cost,
        });
        let limit = history_limit.max(1);
        if cost_history.len() > limit {
            cost_history.drain(0..cost_history.len() - limit);
        }

        let (last_valid_reviewed_sha, last_valid_score, last_valid_status) =
            if artifact.status == ReviewStatus::ReviewError {
                previous
                    .map(|state| {
                        (
                            state.last_valid_reviewed_sha.clone(),
                            state.last_valid_score,
                            state.last_valid_status.clone(),
                        )
                    })
                    .unwrap_or((None, None, None))
            } else {
                (
                    Some(artifact.reviewed_sha.clone()),
                    artifact.score,
                    Some(artifact.status.clone()),
                )
            };
        let mut state = SummaryState {
            version: 1,
            last_reviewed_sha: artifact.reviewed_sha.clone(),
            last_valid_reviewed_sha,
            last_valid_score,
            last_valid_status,
            reviewed_shas,
            run_count: previous
                .map(|state| state.run_count.saturating_add(1))
                .unwrap_or(1),
            cumulative_cost_usd: previous
                .map(|state| state.cumulative_cost_usd)
                .unwrap_or(0.0)
                + current_cost,
            cost_history,
        };
        if state.reviewed_shas.len() > limit {
            state
                .reviewed_shas
                .drain(0..state.reviewed_shas.len() - limit);
        }
        state.validate()?;
        Ok(state)
    }

    pub fn validate(&self) -> Result<(), ReviewGateError> {
        if self.version != 1 {
            return Err(ReviewGateError::InvalidSummaryState(format!(
                "unsupported version {}",
                self.version
            )));
        }
        if self.last_reviewed_sha.trim().is_empty() {
            return Err(ReviewGateError::InvalidSummaryState(
                "last_reviewed_sha must not be empty".to_string(),
            ));
        }
        if let Some(score) = self.last_valid_score {
            validate_score(score)?;
        }
        if self.last_valid_score.is_some() != self.last_valid_reviewed_sha.is_some()
            || self.last_valid_score.is_some() != self.last_valid_status.is_some()
        {
            return Err(ReviewGateError::InvalidSummaryState(
                "latest valid review fields must be present together".to_string(),
            ));
        }
        if self.last_valid_status == Some(ReviewStatus::ReviewError) {
            return Err(ReviewGateError::InvalidSummaryState(
                "latest valid review cannot have review_error status".to_string(),
            ));
        }
        validate_estimated_cost(self.cumulative_cost_usd)?;
        for run in &self.cost_history {
            run.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SummaryOptions {
    pub min_severity: Severity,
    pub cost_history_limit: usize,
}

impl Default for SummaryOptions {
    fn default() -> Self {
        Self {
            min_severity: Severity::P4,
            cost_history_limit: DEFAULT_COST_HISTORY_LIMIT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelPreset {
    Cheap,
    Balanced,
    Strong,
}

impl ModelPreset {
    pub fn as_str(&self) -> &'static str {
        match self {
            ModelPreset::Cheap => "cheap",
            ModelPreset::Balanced => "balanced",
            ModelPreset::Strong => "strong",
        }
    }

    pub fn default_model(&self) -> &'static str {
        match self {
            ModelPreset::Cheap => "qwen/qwen3-coder",
            ModelPreset::Balanced => "deepseek/deepseek-v4-flash",
            ModelPreset::Strong => "anthropic/claude-sonnet-4",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SecretString([redacted])")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenRouterConfig {
    pub base_url: String,
    pub api_key: SecretString,
    pub model: String,
}

impl OpenRouterConfig {
    pub fn byok(api_key: impl Into<String>, preset: ModelPreset) -> Self {
        Self {
            base_url: OPENROUTER_DEFAULT_BASE_URL.to_string(),
            api_key: SecretString::new(api_key),
            model: preset.default_model().to_string(),
        }
    }

    pub fn bearer_header(&self) -> String {
        format!("Bearer {}", self.api_key.expose())
    }

    pub fn chat_completions_url(&self) -> String {
        format!(
            "{}{}",
            self.base_url.trim_end_matches('/'),
            OPENROUTER_CHAT_COMPLETIONS_PATH
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpenRouterMessage {
    pub role: String,
    pub content: String,
}

impl OpenRouterMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OpenRouterChatRequest {
    pub model: String,
    pub messages: Vec<OpenRouterMessage>,
    pub temperature: f64,
}

impl OpenRouterChatRequest {
    pub fn review_prompt(config: &OpenRouterConfig, prompt: impl Into<String>) -> Self {
        Self {
            model: config.model.clone(),
            messages: vec![
                OpenRouterMessage::system(
                    "You are ReviewGate. Return concise, actionable PR review findings.",
                ),
                OpenRouterMessage::user(prompt),
            ],
            temperature: 0.0,
        }
    }
}

pub trait OpenRouterTransport {
    type Error;

    fn send_chat_completion(
        &mut self,
        config: &OpenRouterConfig,
        request: &OpenRouterChatRequest,
    ) -> Result<String, Self::Error>;
}

#[derive(Debug)]
pub struct OpenRouterClient<T> {
    config: OpenRouterConfig,
    transport: T,
}

impl<T> OpenRouterClient<T> {
    pub fn new(config: OpenRouterConfig, transport: T) -> Self {
        Self { config, transport }
    }

    pub fn config(&self) -> &OpenRouterConfig {
        &self.config
    }
}

impl<T: OpenRouterTransport> OpenRouterClient<T> {
    pub fn review_prompt(&mut self, prompt: impl Into<String>) -> Result<String, T::Error> {
        let request = OpenRouterChatRequest::review_prompt(&self.config, prompt);
        self.transport.send_chat_completion(&self.config, &request)
    }
}

pub fn extract_summary_state(summary: &str) -> Result<Option<SummaryState>, ReviewGateError> {
    let Some(start) = summary.find(SUMMARY_STATE_PREFIX) else {
        return Ok(None);
    };
    let state_start = start + SUMMARY_STATE_PREFIX.len();
    let Some(relative_end) = summary[state_start..].find(SUMMARY_STATE_SUFFIX) else {
        return Err(ReviewGateError::InvalidSummaryState(
            "missing state comment suffix".to_string(),
        ));
    };
    let state_end = state_start + relative_end;
    let raw = &summary[state_start..state_end];
    let mut state: SummaryState = serde_json::from_str(raw)
        .map_err(|error| ReviewGateError::InvalidSummaryState(error.to_string()))?;
    if state.last_valid_score.is_none()
        && let Some(score) = extract_legacy_summary_score(summary)
    {
        state.last_valid_reviewed_sha = Some(state.last_reviewed_sha.clone());
        state.last_valid_score = Some(score);
        state.last_valid_status = Some(status_for_score(score));
    }
    state.validate()?;
    Ok(Some(state))
}

fn extract_legacy_summary_score(summary: &str) -> Option<u8> {
    let score = summary
        .lines()
        .find_map(|line| {
            line.strip_prefix("<h2 align=\"left\">Confidence Score: ")?
                .strip_suffix("/5</h2>")
        })?
        .trim()
        .parse::<u8>()
        .ok()?;
    (score <= DEFAULT_TARGET_SCORE).then_some(score)
}

pub fn render_summary(artifact: &ReviewArtifact) -> Result<String, ReviewGateError> {
    render_summary_with_options(artifact, SummaryOptions::default(), None)
}

pub fn render_summary_with_options(
    artifact: &ReviewArtifact,
    options: SummaryOptions,
    previous_state: Option<&SummaryState>,
) -> Result<String, ReviewGateError> {
    artifact.validate()?;
    let state = SummaryState::for_artifact(artifact, previous_state, options.cost_history_limit)?;
    let state_json = serde_json::to_string(&state)
        .map_err(|error| ReviewGateError::InvalidSummaryState(error.to_string()))?;

    let mut output = String::new();
    render_summary_header(&mut output, &state_json);
    render_concise_summary_body(&mut output, artifact, &options, &state);

    Ok(output)
}

fn render_summary_header(output: &mut String, state_json: &str) {
    output.push_str(SUMMARY_MARKER);
    output.push_str("\n\n");
    output.push_str(SUMMARY_STATE_PREFIX);
    output.push_str(state_json);
    output.push_str(SUMMARY_STATE_SUFFIX);
    output.push_str("\n\n");
    output.push_str("# Review Gate Summary\n\n");
}

fn render_concise_summary_body(
    output: &mut String,
    artifact: &ReviewArtifact,
    options: &SummaryOptions,
    state: &SummaryState,
) {
    let metrics = compute_metrics(artifact, options.min_severity);

    output.push_str(artifact.verdict.trim());
    output.push_str("\n\n");
    render_score_block(output, artifact, state);
    render_angle_score_table(output, artifact);
    render_angle_errors(output, artifact);
    output.push_str(&format!(
        "Findings: {} total, {} blocking, {} inline candidates\n",
        metrics.finding_count, metrics.blocking_finding_count, metrics.inline_eligible_count
    ));
    if !artifact.notes.is_empty() {
        output.push_str(&format!(
            "Notes: {} note(s) in the JSON artifact.\n",
            artifact.notes.len()
        ));
    }

    output.push('\n');
    if artifact.status == ReviewStatus::ReviewError {
        output.push_str(
            "This review is inconclusive. Retry the reviewer errors before using it as a code-quality decision.\n",
        );
    } else if metrics.finding_count == 0 {
        output.push_str("No findings. Re-run ReviewGate if new commits land.\n");
    } else {
        output.push_str(&format!(
            "Findings at or above {} are published as inline PR comments when ReviewGate runs in GitHub Actions. File-level, PR-level, and stale-line findings are anchored to fallback right-side diff lines when needed. See the JSON artifact for the full machine-readable review.\n",
            options.min_severity.as_str()
        ));
    }

    output.push('\n');
    render_summary_details(output, artifact, options);
    render_summary_footer(output, state, artifact);
}

fn render_score_block(output: &mut String, artifact: &ReviewArtifact, state: &SummaryState) {
    if let Some(score) = artifact.score {
        output.push_str(&format!(
            "<h2 align=\"left\">Confidence Score: {score}/5</h2>\n\n"
        ));
        return;
    }

    output.push_str("<h2 align=\"left\">Review incomplete</h2>\n\n");
    if let (Some(score), Some(reviewed_sha)) = (
        state.last_valid_score,
        state.last_valid_reviewed_sha.as_deref(),
    ) {
        output.push_str(&format!(
            "Latest valid score: {score}/5 on <code>{}</code>. The current review did not replace it.\n\n",
            escape_html_text(reviewed_sha)
        ));
    }
}

fn render_angle_score_table(output: &mut String, artifact: &ReviewArtifact) {
    if artifact.angle_results.is_empty() {
        return;
    }

    output.push_str("| Review angle | Score | Findings |\n");
    output.push_str("| --- | ---: | ---: |\n");
    for angle in &artifact.angle_results {
        output.push_str(&format!(
            "| {} | {}/5 | {} |\n",
            markdown_table_cell(&angle.name),
            angle.score,
            angle.finding_ids.len()
        ));
    }
    output.push('\n');
}

fn render_angle_errors(output: &mut String, artifact: &ReviewArtifact) {
    if artifact.angle_errors.is_empty() {
        return;
    }

    output.push_str("| Review angle error | Kind | Retryable |\n");
    output.push_str("| --- | --- | --- |\n");
    for error in &artifact.angle_errors {
        output.push_str(&format!(
            "| {} | {} | {} |\n",
            markdown_table_cell(&error.angle_name),
            error.kind.as_str(),
            if error.retryable { "yes" } else { "no" }
        ));
    }
    output.push('\n');
}

fn render_summary_details(
    output: &mut String,
    artifact: &ReviewArtifact,
    options: &SummaryOptions,
) {
    render_important_files_changed(output, artifact, options.min_severity);
    render_flowchart(output, artifact);
}

fn render_important_files_changed(
    output: &mut String,
    artifact: &ReviewArtifact,
    min_severity: Severity,
) {
    output.push_str("<details>\n<summary>Important Files Changed</summary>\n\n");
    let files = important_file_summaries(artifact, min_severity);
    if files.is_empty() {
        output.push_str("No files require special attention from this review.\n\n");
    } else {
        output.push_str("| Filename | Overview |\n");
        output.push_str("| --- | --- |\n");
        for file in files {
            output.push_str(&format!(
                "| {} | {} |\n",
                markdown_table_cell(&file.path),
                markdown_table_cell(&file.overviews.join("; "))
            ));
        }
        output.push('\n');
    }
    output.push_str("</details>\n\n");
}

fn render_flowchart(output: &mut String, artifact: &ReviewArtifact) {
    output.push_str("<details>\n<summary>Flowchart</summary>\n\n");
    output.push_str("```mermaid\n");
    output.push_str("flowchart TD\n");
    output.push_str(
        "    A[\"Pull request update\"] --> B[\"ReviewGate analyzes the latest commit\"]\n",
    );
    output.push_str("    B --> C[\"Model review stages\"]\n");
    output.push_str("    C --> D[\"Structured JSON artifact\"]\n");
    if let Some(score) = artifact.score {
        output.push_str(&format!("    D --> E[\"Confidence Score: {score}/5\"]\n"));
    } else {
        output.push_str("    D --> E[\"Review incomplete\"]\n");
    }
    output.push_str("    D --> F[\"Canonical PR summary comment\"]\n");
    output.push_str("    F --> G[\"Human or agent fixes findings\"]\n");
    output.push_str(&format!(
        "    E --> H[\"Status: {}\"]\n",
        artifact.status.as_str()
    ));
    output.push_str("```\n\n");
    output.push_str("</details>\n\n");
}

fn render_summary_footer(output: &mut String, state: &SummaryState, artifact: &ReviewArtifact) {
    output.push_str(&format!(
        "<sub>Reviews on this PR: {}. ",
        format_run_count(state.run_count)
    ));
    if let Some(line_count) = artifact
        .metrics
        .as_ref()
        .and_then(|metrics| metrics.analyzed_line_count)
    {
        output.push_str(&format!(
            "Changed lines analyzed: {}. ",
            format_line_count(line_count)
        ));
    }
    output.push_str(&format!(
        "Total cost: {}. Latest commit analyzed: <code>{}</code>.</sub>\n",
        format_cost(state.cumulative_cost_usd),
        escape_html_text(&state.last_reviewed_sha)
    ));
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ImportantFileSummary {
    path: String,
    overviews: Vec<String>,
}

fn important_file_summaries(
    artifact: &ReviewArtifact,
    min_severity: Severity,
) -> Vec<ImportantFileSummary> {
    let mut files: Vec<ImportantFileSummary> = Vec::new();
    for finding in &artifact.findings {
        if !finding.severity.is_at_or_above(min_severity) {
            continue;
        }
        if !is_important_file_finding(finding) {
            continue;
        }
        let Some(path) = finding.file.as_deref().and_then(normalize_finding_path) else {
            continue;
        };
        let overview = finding_overview(finding);
        if let Some(existing) = files.iter_mut().find(|file| file.path == path) {
            existing.overviews.push(overview);
        } else {
            files.push(ImportantFileSummary {
                path,
                overviews: vec![overview],
            });
        }
    }
    files
}

fn is_important_file_finding(finding: &Finding) -> bool {
    finding.is_blocking(DEFAULT_TARGET_SCORE) || finding.severity.is_at_or_above(Severity::P3)
}

fn normalize_finding_path(path: &str) -> Option<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }
    let without_current_dir = trimmed.strip_prefix("./").unwrap_or(trimmed);
    let normalized = without_current_dir
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .collect::<Vec<_>>()
        .join("/");
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn finding_overview(finding: &Finding) -> String {
    format!("{} finding", finding.severity.as_str())
}

fn markdown_table_cell(value: &str) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.is_empty() {
        "-".to_string()
    } else {
        escape_html_text(&compact).replace('|', "\\|")
    }
}

fn escape_html_text(value: &str) -> String {
    let mut escaped = String::new();
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn format_run_count(run_count: u32) -> String {
    if run_count == 1 {
        "1 run".to_string()
    } else {
        format!("{run_count} runs")
    }
}

fn format_cost(cost: f64) -> String {
    if cost > 0.0 && cost < 0.01 {
        format!("${cost:.4}")
    } else {
        format!("${cost:.2}")
    }
}

fn format_line_count(line_count: u32) -> String {
    let raw = line_count.to_string();
    let mut formatted = String::new();
    for (index, character) in raw.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            formatted.push(',');
        }
        formatted.push(character);
    }
    formatted.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scoring_artifact(findings: Vec<Finding>) -> ReviewArtifact {
        ReviewArtifact {
            score: Some(compute_score(&findings)),
            target_score: DEFAULT_TARGET_SCORE,
            reviewed_sha: "abc123".to_string(),
            status: if compute_score(&findings) == DEFAULT_TARGET_SCORE {
                ReviewStatus::Passed
            } else {
                ReviewStatus::NeedsChanges
            },
            verdict: "Deterministic test artifact.".to_string(),
            models: vec!["balanced".to_string()],
            estimated_cost_usd: None,
            cost_summary: None,
            metrics: None,
            review_stages: vec![],
            angle_results: vec![],
            angle_errors: vec![],
            findings,
            notes: vec![],
        }
    }

    fn scoring_finding(id: &str, severity: Severity) -> Finding {
        Finding {
            id: id.to_string(),
            angle_id: None,
            scope: FindingScope::Pr,
            severity,
            confidence: 1.0,
            grounding: None,
            file: None,
            line: None,
            title: "Invariant fixture".to_string(),
            detail: None,
            agent_instruction: "Fix the invariant fixture.".to_string(),
        }
    }

    #[test]
    fn completed_outcome_validation_exhaustively_matches_derived_score_and_status() {
        let finding_sets = [
            vec![],
            vec![scoring_finding("p0", Severity::P0)],
            vec![scoring_finding("p1", Severity::P1)],
            vec![scoring_finding("p2", Severity::P2)],
            vec![scoring_finding("p3", Severity::P3)],
            vec![scoring_finding("p4", Severity::P4)],
        ];

        for findings in finding_sets {
            let expected_score = compute_score(&findings);
            let expected_status = if expected_score == DEFAULT_TARGET_SCORE {
                ReviewStatus::Passed
            } else {
                ReviewStatus::NeedsChanges
            };
            for score in 0..=5 {
                for status in [ReviewStatus::Passed, ReviewStatus::NeedsChanges] {
                    let mut artifact = scoring_artifact(findings.clone());
                    artifact.score = Some(score);
                    artifact.status = status.clone();

                    assert_eq!(
                        artifact.validate().is_ok(),
                        score == expected_score && status == expected_status,
                        "score={score}, status={}, expected_score={expected_score}, expected_status={}",
                        status.as_str(),
                        expected_status.as_str(),
                    );
                }
            }
        }
    }

    #[test]
    fn angle_outcome_validation_matches_its_referenced_findings() {
        let mut artifact = scoring_artifact(vec![Finding {
            angle_id: Some("general".to_string()),
            ..scoring_finding("general:p2", Severity::P2)
        }]);
        artifact.angle_results = vec![ReviewAngleResult {
            id: "general".to_string(),
            name: "General".to_string(),
            score: 5,
            status: ReviewStatus::Passed,
            verdict: "No issue.".to_string(),
            model: "balanced".to_string(),
            finding_ids: vec!["general:p2".to_string()],
        }];

        assert!(
            artifact.validate().is_err(),
            "an angle score/status cannot contradict its referenced findings"
        );

        artifact.angle_results[0].score = 3;
        artifact.angle_results[0].status = ReviewStatus::NeedsChanges;
        assert!(artifact.validate().is_ok());
    }

    #[test]
    fn angle_outcome_rejects_unknown_or_omitted_finding_references() {
        let mut artifact = scoring_artifact(vec![Finding {
            angle_id: Some("general".to_string()),
            ..scoring_finding("general:p2", Severity::P2)
        }]);
        artifact.angle_results = vec![ReviewAngleResult {
            id: "general".to_string(),
            name: "General".to_string(),
            score: 5,
            status: ReviewStatus::Passed,
            verdict: "No issue.".to_string(),
            model: "balanced".to_string(),
            finding_ids: vec![],
        }];
        assert!(
            artifact.validate().is_err(),
            "angle-owned findings must be included in the angle result"
        );

        artifact.angle_results[0].finding_ids = vec!["missing".to_string()];
        assert!(
            artifact.validate().is_err(),
            "angle results cannot cite findings that do not exist"
        );

        artifact.angle_results.clear();
        assert!(
            artifact.validate().is_err(),
            "angle-owned findings require a matching successful angle result"
        );
    }

    #[test]
    fn angle_outcome_rejects_duplicate_and_cross_owned_ids() {
        let finding = Finding {
            angle_id: Some("general".to_string()),
            ..scoring_finding("general:p2", Severity::P2)
        };
        let angle = ReviewAngleResult {
            id: "general".to_string(),
            name: "General".to_string(),
            score: 3,
            status: ReviewStatus::NeedsChanges,
            verdict: "Material issue.".to_string(),
            model: "balanced".to_string(),
            finding_ids: vec![finding.id.clone()],
        };
        let mut valid = scoring_artifact(vec![finding]);
        valid.angle_results = vec![angle];
        assert!(valid.validate().is_ok());

        let mut duplicate_finding = valid.clone();
        duplicate_finding
            .findings
            .push(duplicate_finding.findings[0].clone());
        assert!(duplicate_finding.validate().is_err());

        let mut duplicate_angle = valid.clone();
        duplicate_angle
            .angle_results
            .push(duplicate_angle.angle_results[0].clone());
        assert!(duplicate_angle.validate().is_err());

        let mut repeated_reference = valid.clone();
        repeated_reference.angle_results[0]
            .finding_ids
            .push("general:p2".to_string());
        assert!(repeated_reference.validate().is_err());

        let mut cross_owned = valid;
        cross_owned.findings[0].angle_id = Some("security".to_string());
        assert!(cross_owned.validate().is_err());
    }

    #[test]
    fn angle_errors_reject_duplicate_ids_and_success_failure_overlap() {
        let angle_error = ReviewAngleError {
            angle_id: "security".to_string(),
            angle_name: "Security".to_string(),
            kind: ReviewErrorKind::Timeout,
            retryable: true,
            message: "The reviewer request timed out.".to_string(),
            model: "balanced".to_string(),
        };
        let mut duplicate_errors = ReviewArtifact {
            score: None,
            target_score: DEFAULT_TARGET_SCORE,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::ReviewError,
            verdict: "Review incomplete.".to_string(),
            models: vec!["balanced".to_string()],
            estimated_cost_usd: None,
            cost_summary: None,
            metrics: None,
            review_stages: vec![],
            angle_results: vec![],
            angle_errors: vec![angle_error.clone(), angle_error.clone()],
            findings: vec![],
            notes: vec![],
        };
        assert!(duplicate_errors.validate().is_err());

        duplicate_errors.angle_errors = vec![ReviewAngleError {
            angle_id: "general".to_string(),
            angle_name: "General".to_string(),
            ..angle_error
        }];
        duplicate_errors.angle_results = vec![ReviewAngleResult {
            id: "general".to_string(),
            name: "General".to_string(),
            score: 5,
            status: ReviewStatus::Passed,
            verdict: "No issue.".to_string(),
            model: "balanced".to_string(),
            finding_ids: vec![],
        }];
        assert!(duplicate_errors.validate().is_err());
    }

    #[test]
    fn publication_preparation_preserves_a_fresh_valid_artifact() {
        let artifact = scoring_artifact(vec![scoring_finding("p2", Severity::P2)]);

        assert_eq!(
            artifact.clone().prepared_for_publication("abc123"),
            artifact
        );
    }

    #[test]
    fn publication_preparation_sanitizes_review_error_messages() {
        let artifact = ReviewArtifact {
            score: None,
            target_score: DEFAULT_TARGET_SCORE,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::ReviewError,
            verdict: "provider body: sentinel-secret".to_string(),
            models: vec!["balanced".to_string()],
            estimated_cost_usd: None,
            cost_summary: None,
            metrics: None,
            review_stages: vec![],
            angle_results: vec![],
            angle_errors: vec![ReviewAngleError {
                angle_id: "general".to_string(),
                angle_name: "General".to_string(),
                kind: ReviewErrorKind::ProviderError,
                retryable: false,
                message: "provider body: sentinel-secret".to_string(),
                model: "balanced".to_string(),
            }],
            findings: vec![],
            notes: vec!["provider body: sentinel-secret".to_string()],
        };

        let prepared = artifact.prepared_for_publication("abc123");
        let public_json = serde_json::to_string(&prepared).expect("artifact serializes");

        assert_eq!(
            prepared.verdict,
            "ReviewGate could not complete every enabled review angle."
        );
        assert_eq!(
            prepared.angle_errors[0].message,
            "The reviewer provider rejected or could not complete the request."
        );
        assert!(prepared.notes.is_empty());
        assert!(!public_json.contains("sentinel-secret"));
    }

    #[test]
    fn invalid_publication_state_uses_the_existing_error_kind_contract() {
        let mut artifact = scoring_artifact(vec![]);
        artifact.score = Some(0);
        artifact.status = ReviewStatus::NeedsChanges;

        let prepared = artifact.prepared_for_publication("current-sha");

        assert_eq!(prepared.reviewed_sha, "current-sha");
        assert_eq!(prepared.status, ReviewStatus::ReviewError);
        assert_eq!(prepared.score, None);
        assert_eq!(prepared.angle_errors[0].angle_id, "artifact_validation");
        assert_eq!(
            prepared.angle_errors[0].kind,
            ReviewErrorKind::MalformedResponse
        );
        assert!(!prepared.angle_errors[0].retryable);
    }

    #[test]
    fn computes_score_from_highest_severity() {
        let findings = vec![Finding {
            id: "rg_001".to_string(),
            angle_id: None,
            scope: FindingScope::Line,
            severity: Severity::P2,
            confidence: 0.9,
            grounding: None,
            file: Some("src/lib.rs".to_string()),
            line: Some(42),
            title: "Missing regression test".to_string(),
            detail: None,
            agent_instruction: "Add the regression test.".to_string(),
        }];

        assert_eq!(compute_score(&findings), 3);
    }

    #[test]
    fn p0_findings_cap_score_at_one() {
        let findings = vec![Finding {
            id: "rg_001".to_string(),
            angle_id: None,
            scope: FindingScope::Line,
            severity: Severity::P0,
            confidence: 0.98,
            grounding: None,
            file: Some("src/auth.rs".to_string()),
            line: Some(7),
            title: "Authentication bypass".to_string(),
            detail: None,
            agent_instruction: "Fix the bypass before merge.".to_string(),
        }];

        assert_eq!(compute_score(&findings), 1);
    }

    #[test]
    fn non_line_scope_findings_are_inline_eligible() {
        let finding = Finding {
            id: "rg_001".to_string(),
            angle_id: None,
            scope: FindingScope::File,
            severity: Severity::P2,
            confidence: 0.95,
            grounding: None,
            file: Some("src/lib.rs".to_string()),
            line: Some(42),
            title: "Module-level behavior needs a test".to_string(),
            detail: None,
            agent_instruction: "Add coverage for the broader module behavior.".to_string(),
        };

        assert!(is_inline_comment_eligible(&finding, Severity::P2));

        let artifact = ReviewArtifact {
            score: Some(3),
            target_score: 5,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::NeedsChanges,
            verdict: "One file-scoped issue remains.".to_string(),
            models: vec!["balanced".to_string()],
            estimated_cost_usd: None,
            cost_summary: None,
            metrics: None,
            review_stages: vec![],
            angle_results: vec![],
            angle_errors: vec![],
            findings: vec![finding],
            notes: vec![],
        };
        let summary = render_summary(&artifact).expect("summary renders");

        assert!(summary.contains("inline PR comments"));
        assert!(!summary.contains("standalone PR comments"));
        assert!(!summary.contains("Module-level behavior needs a test"));
    }

    #[test]
    fn computes_score_without_relying_on_enum_ordering() {
        let findings = vec![
            Finding {
                id: "rg_001".to_string(),
                angle_id: None,
                scope: FindingScope::Line,
                severity: Severity::P4,
                confidence: 0.9,
                grounding: None,
                file: None,
                line: None,
                title: "Style note".to_string(),
                detail: None,
                agent_instruction: "Consider simplifying this wording.".to_string(),
            },
            Finding {
                id: "rg_002".to_string(),
                angle_id: None,
                scope: FindingScope::Line,
                severity: Severity::P1,
                confidence: 0.9,
                grounding: None,
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
            score: Some(4),
            target_score: 5,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::NeedsChanges,
            verdict: "Good shape, one minor issue remains.".to_string(),
            models: vec!["balanced".to_string()],
            estimated_cost_usd: Some(0.08),
            cost_summary: None,
            metrics: None,
            review_stages: vec![],
            angle_results: vec![],
            angle_errors: vec![],
            findings: vec![scoring_finding("rg_001", Severity::P3)],
            notes: vec![],
        };

        let summary = render_summary(&artifact).expect("summary renders");
        assert!(summary.starts_with(SUMMARY_MARKER));
        assert!(summary.contains("# Review Gate Summary"));
        assert!(summary.contains("<h2 align=\"left\">Confidence Score: 4/5</h2>"));
    }

    #[test]
    fn renders_angle_score_table_when_angle_results_are_present() {
        let raw = r#"{
          "score": 3,
          "reviewed_sha": "abc123",
          "status": "needs_changes",
          "verdict": "Adversarial review found one blocking issue.",
          "models": ["deepseek/deepseek-v4-flash"],
          "angle_results": [
            {
              "id": "general",
              "name": "General",
              "score": 5,
              "status": "passed",
              "verdict": "No general findings.",
              "model": "deepseek/deepseek-v4-flash",
              "finding_ids": []
            },
            {
              "id": "adversarial",
              "name": "Adversarial",
              "score": 3,
              "status": "needs_changes",
              "verdict": "One correctness issue survived the skeptical pass.",
              "model": "deepseek/deepseek-v4-flash",
              "finding_ids": ["adversarial:rg_001"]
            }
          ],
          "findings": [
            {
              "id": "adversarial:rg_001",
              "angle_id": "adversarial",
              "scope": "line",
              "severity": "P2",
              "confidence": 0.9,
              "file": "src/lib.rs",
              "line": 42,
              "title": "Missing error handling",
              "detail": "The error path is dropped.",
              "agent_instruction": "Handle and test the error path."
            }
          ],
          "notes": []
        }"#;
        let artifact: ReviewArtifact = serde_json::from_str(raw).expect("artifact parses");

        let summary = render_summary(&artifact).expect("summary renders");

        assert!(summary.contains("| Review angle | Score | Findings |"));
        assert!(summary.contains("| General | 5/5 | 0 |"));
        assert!(summary.contains("| Adversarial | 3/5 | 1 |"));
    }

    #[test]
    fn fixture_renders_default_concise_summary_shape() {
        let artifact: ReviewArtifact =
            serde_json::from_str(include_str!("../../../fixtures/simple-review.json"))
                .expect("fixture parses");
        let artifact = artifact.with_computed_score().expect("score computes");

        let summary = render_summary(&artifact).expect("summary renders");

        assert!(summary.contains("# Review Gate Summary"));
        assert!(
            summary
                .contains("Good structure, but not ready for merge because one test gap remains.")
        );
        assert!(summary.contains("<h2 align=\"left\">Confidence Score: 3/5</h2>"));
        assert!(summary.contains("Findings: 2 total, 1 blocking, 2 inline candidates"));
        assert!(!summary.contains("Cost: $0.08 (1 run)"));
        assert!(!summary.contains("Status: `"));
        assert!(!summary.contains("Target:"));
        assert!(!summary.contains("Reviewed:"));
        assert!(!summary.contains("## Cost"));
        assert!(!summary.contains("## Metrics"));
        assert!(!summary.contains("## Target-Blocking Findings"));
        assert!(!summary.contains("## Non-Blocking Notes"));
        assert!(!summary.contains("## Agent Instructions"));
        assert!(!summary.contains("- P2: Missing regression test for retry exhaustion"));
        assert!(!summary.contains("Helper name is slightly vague"));
        assert!(!summary.contains("Fallback findings:"));
        assert!(summary.contains("Findings at or above P4 are published"));
        assert!(summary.contains("<details>\n<summary>Important Files Changed</summary>"));
        assert!(summary.contains("| Filename | Overview |"));
        assert!(summary.contains("| app/webhooks/retry.py | P2 finding |"));
        assert!(!summary.contains(
            "| app/webhooks/retry.py | P2: Missing regression test for retry exhaustion; P4: Helper name is slightly vague |"
        ));
        assert!(summary.contains("<details>\n<summary>Flowchart</summary>"));
        assert!(summary.contains("```mermaid\nflowchart TD"));
        assert!(summary.contains("D --> E[\"Confidence Score: 3/5\"]"));
        assert!(summary.contains("<sub>Reviews on this PR: 1 run. Total cost: $0.08. Latest commit analyzed: <code>abc123</code>.</sub>"));
        assert!(!summary.contains("<details open>"));
    }

    #[test]
    fn concise_summary_escapes_important_file_table_cells() {
        let artifact = ReviewArtifact {
            score: Some(3),
            target_score: 5,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::NeedsChanges,
            verdict: "One inline finding remains.".to_string(),
            models: vec!["balanced".to_string()],
            estimated_cost_usd: None,
            cost_summary: None,
            metrics: None,
            review_stages: vec![],
            angle_results: vec![],
            angle_errors: vec![],
            findings: vec![Finding {
                id: "rg_001".to_string(),
                angle_id: None,
                scope: FindingScope::Line,
                severity: Severity::P2,
                confidence: 0.9,
                grounding: None,
                file: Some("./src//a|b.rs".to_string()),
                line: Some(42),
                title: "Pipe | \"<tag>\" & issue".to_string(),
                detail: None,
                agent_instruction: "Fix the escaped table issue.".to_string(),
            }],
            notes: vec![],
        };

        let summary = render_summary(&artifact).expect("summary renders");

        assert!(summary.contains("| src/a\\|b.rs | P2 finding |"));
        assert!(!summary.contains("| ./src//a|b.rs | P2: Pipe | \"<tag>\" & issue |"));
    }

    #[test]
    fn renders_cost_summary_in_footer_only() {
        let artifact = ReviewArtifact {
            score: Some(5),
            target_score: 5,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::Passed,
            verdict: "Clean review.".to_string(),
            models: vec!["deepseek/deepseek-v4-flash".to_string()],
            estimated_cost_usd: None,
            cost_summary: Some(CostSummary {
                current_run_usd: 0.0123,
                source: None,
                components: vec![CostComponent {
                    label: "general".to_string(),
                    model: "deepseek/deepseek-v4-flash".to_string(),
                    prompt_tokens: Some(1200),
                    completion_tokens: Some(300),
                    estimated_cost_usd: 0.0123,
                }],
            }),
            metrics: None,
            review_stages: vec![],
            angle_results: vec![],
            angle_errors: vec![],
            findings: vec![],
            notes: vec![],
        };

        let summary = render_summary(&artifact).expect("summary renders");

        assert!(summary.contains("Total cost: $0.01"));
        assert!(!summary.contains("## Cost"));
        assert!(!summary.contains("Current run cost: $0.0123"));
        assert!(!summary.contains("- general (`deepseek/deepseek-v4-flash`): $0.0123"));
    }

    #[test]
    fn extracts_and_carries_hidden_summary_state() {
        let artifact = ReviewArtifact {
            score: Some(5),
            target_score: 5,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::Passed,
            verdict: "Clean review.".to_string(),
            models: vec!["deepseek/deepseek-v4-flash".to_string()],
            estimated_cost_usd: None,
            cost_summary: Some(CostSummary {
                current_run_usd: 0.0100,
                source: None,
                components: vec![],
            }),
            metrics: None,
            review_stages: vec![],
            angle_results: vec![],
            angle_errors: vec![],
            findings: vec![],
            notes: vec![],
        };
        let first = render_summary(&artifact).expect("summary renders");
        let previous = extract_summary_state(&first)
            .expect("state parses")
            .expect("state exists");
        let mut rerun_artifact = artifact.clone();
        rerun_artifact.reviewed_sha = "def456".to_string();
        rerun_artifact.cost_summary = Some(CostSummary {
            current_run_usd: 0.0200,
            source: None,
            components: vec![],
        });

        let second = render_summary_with_options(
            &rerun_artifact,
            SummaryOptions::default(),
            Some(&previous),
        )
        .expect("summary renders");
        let state = extract_summary_state(&second)
            .expect("state parses")
            .expect("state exists");

        assert_eq!(state.run_count, 2);
        assert_eq!(state.last_reviewed_sha, "def456");
        assert_eq!(state.reviewed_shas, vec!["abc123", "def456"]);
        assert!((state.cumulative_cost_usd - 0.03).abs() < f64::EPSILON);
        assert!(second.contains("Reviews on this PR: 2 runs. Total cost: $0.03."));
        assert!(second.contains("Latest commit analyzed: <code>def456</code>."));
    }

    #[test]
    fn legacy_summary_state_recovers_its_visible_score_for_an_inconclusive_rerun() {
        let legacy = concat!(
            "<!-- reviewgate-summary -->\n\n",
            "<!-- reviewgate-state {\"version\":1,\"last_reviewed_sha\":\"legacy-sha\",",
            "\"reviewed_shas\":[\"legacy-sha\"],\"run_count\":1,",
            "\"cumulative_cost_usd\":0.01,\"cost_history\":[]} -->\n\n",
            "<p>Model verdict mentioned Confidence Score: 1/5 before the owned heading.</p>\n",
            "<h2 align=\"left\">Confidence Score: 3/5</h2>\n"
        );

        let state = extract_summary_state(legacy)
            .expect("legacy state parses")
            .expect("state exists");

        assert_eq!(state.last_valid_score, Some(3));
        assert_eq!(state.last_valid_reviewed_sha.as_deref(), Some("legacy-sha"));
        assert_eq!(state.last_valid_status, Some(ReviewStatus::NeedsChanges));
    }

    #[test]
    fn min_severity_filters_inline_candidate_count_without_listing_findings() {
        let artifact = ReviewArtifact {
            score: Some(3),
            target_score: 5,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::NeedsChanges,
            verdict: "One visible issue remains.".to_string(),
            models: vec!["balanced".to_string()],
            estimated_cost_usd: None,
            cost_summary: None,
            metrics: None,
            review_stages: vec![],
            angle_results: vec![],
            angle_errors: vec![],
            findings: vec![
                Finding {
                    id: "rg_001".to_string(),
                    angle_id: None,
                    scope: FindingScope::Line,
                    severity: Severity::P2,
                    confidence: 0.9,
                    grounding: None,
                    file: None,
                    line: None,
                    title: "Visible reliability issue".to_string(),
                    detail: None,
                    agent_instruction: "Fix the reliability issue.".to_string(),
                },
                Finding {
                    id: "rg_002".to_string(),
                    angle_id: None,
                    scope: FindingScope::Line,
                    severity: Severity::P4,
                    confidence: 0.9,
                    grounding: None,
                    file: None,
                    line: None,
                    title: "Hidden style note".to_string(),
                    detail: None,
                    agent_instruction: "Consider a style tweak.".to_string(),
                },
            ],
            notes: vec![],
        };

        let summary = render_summary_with_options(
            &artifact,
            SummaryOptions {
                min_severity: Severity::P2,
                ..SummaryOptions::default()
            },
            None,
        )
        .expect("summary renders");

        assert!(summary.contains("Findings: 2 total, 1 blocking, 1 inline candidates"));
        assert!(summary.contains("Findings at or above P2 are published"));
        assert!(!summary.contains("Visible reliability issue"));
        assert!(!summary.contains("Hidden style note"));
    }

    #[test]
    fn min_severity_can_hide_lower_severity_finding_details_from_summary() {
        let artifact = ReviewArtifact {
            score: Some(4),
            target_score: 5,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::NeedsChanges,
            verdict: "A lower-severity issue still prevents the target score.".to_string(),
            models: vec!["balanced".to_string()],
            estimated_cost_usd: None,
            cost_summary: None,
            metrics: None,
            review_stages: vec![],
            angle_results: vec![],
            angle_errors: vec![],
            findings: vec![Finding {
                id: "rg_001".to_string(),
                angle_id: None,
                scope: FindingScope::Line,
                severity: Severity::P3,
                confidence: 0.9,
                grounding: None,
                file: Some("src/lib.rs".to_string()),
                line: Some(42),
                title: "Target-blocking advisory finding".to_string(),
                detail: None,
                agent_instruction: "Fix this issue before expecting the target score.".to_string(),
            }],
            notes: vec![],
        };

        let summary = render_summary_with_options(
            &artifact,
            SummaryOptions {
                min_severity: Severity::P2,
                ..SummaryOptions::default()
            },
            None,
        )
        .expect("summary renders");

        assert!(!summary.contains("## Target-Blocking Findings"));
        assert!(!summary.contains("Fallback findings:"));
        assert!(summary.contains("Findings at or above P2 are published"));
        assert!(!summary.contains("P3: Target-blocking advisory finding"));
        assert!(!summary.contains("Fix this issue before expecting the target score."));
    }

    #[test]
    fn validation_rejects_empty_cost_component_model() {
        let artifact = ReviewArtifact {
            score: Some(5),
            target_score: 5,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::Passed,
            verdict: "Invalid cost component.".to_string(),
            models: vec!["deepseek/deepseek-v4-flash".to_string()],
            estimated_cost_usd: None,
            cost_summary: Some(CostSummary {
                current_run_usd: 0.0123,
                source: None,
                components: vec![CostComponent {
                    label: "general".to_string(),
                    model: "".to_string(),
                    prompt_tokens: None,
                    completion_tokens: None,
                    estimated_cost_usd: 0.0123,
                }],
            }),
            metrics: None,
            review_stages: vec![],
            angle_results: vec![],
            angle_errors: vec![],
            findings: vec![],
            notes: vec![],
        };

        assert!(matches!(
            artifact.validate(),
            Err(ReviewGateError::InvalidCostComponent { field: "model" })
        ));
    }

    #[test]
    fn validation_rejects_empty_review_angle_fields_with_angle_error() {
        let angle = ReviewAngleResult {
            id: String::new(),
            name: "General".to_string(),
            score: 5,
            status: ReviewStatus::Passed,
            verdict: "Clean.".to_string(),
            model: "deepseek/deepseek-v4-flash".to_string(),
            finding_ids: vec![],
        };

        assert!(matches!(
            angle.validate(),
            Err(ReviewGateError::InvalidReviewAngle { field: "id" })
        ));
    }

    #[test]
    fn summary_does_not_render_agent_instructions_for_findings() {
        let artifact = ReviewArtifact {
            score: Some(3),
            target_score: 5,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::NeedsChanges,
            verdict: "One blocking issue remains.".to_string(),
            models: vec!["balanced".to_string()],
            estimated_cost_usd: None,
            cost_summary: None,
            metrics: None,
            review_stages: vec![],
            angle_results: vec![],
            angle_errors: vec![],
            findings: vec![Finding {
                id: "rg_001".to_string(),
                angle_id: None,
                scope: FindingScope::Line,
                severity: Severity::P2,
                confidence: 0.9,
                grounding: None,
                file: Some("src/lib.rs".to_string()),
                line: Some(42),
                title: "Missing regression test".to_string(),
                detail: None,
                agent_instruction: "Add a regression test for the missing branch.".to_string(),
            }],
            notes: vec![],
        };

        let summary = render_summary(&artifact).expect("summary renders");

        assert!(!summary.contains("## Agent Instructions"));
        assert!(summary.contains("Findings at or above P4 are published"));
        assert!(!summary.contains("Add a regression test for the missing branch."));
    }

    #[test]
    fn computed_status_below_target_needs_changes_instead_of_failed() {
        let artifact = ReviewArtifact {
            score: Some(5),
            target_score: 5,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::Passed,
            verdict: "One blocking issue remains.".to_string(),
            models: vec!["balanced".to_string()],
            estimated_cost_usd: None,
            cost_summary: None,
            metrics: None,
            review_stages: vec![],
            angle_results: vec![],
            angle_errors: vec![],
            findings: vec![Finding {
                id: "rg_001".to_string(),
                angle_id: None,
                scope: FindingScope::Line,
                severity: Severity::P2,
                confidence: 0.9,
                grounding: None,
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

        assert_eq!(artifact.score, Some(3));
        assert_eq!(artifact.status, ReviewStatus::NeedsChanges);
    }

    #[test]
    fn reviewer_errors_make_the_review_inconclusive_without_a_numeric_score() {
        let artifact = ReviewArtifact {
            score: Some(5),
            target_score: 5,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::Passed,
            verdict: "General review passed but another angle failed.".to_string(),
            models: vec!["balanced".to_string()],
            estimated_cost_usd: None,
            cost_summary: None,
            metrics: None,
            review_stages: vec![],
            angle_results: vec![],
            angle_errors: vec![ReviewAngleError {
                angle_id: "adversarial".to_string(),
                angle_name: "Adversarial".to_string(),
                kind: ReviewErrorKind::MalformedResponse,
                retryable: true,
                message: "The reviewer returned an invalid structured response.".to_string(),
                model: "balanced".to_string(),
            }],
            findings: vec![],
            notes: vec![],
        };

        assert!(
            artifact.validate().is_err(),
            "a completed numeric result cannot also carry angle errors"
        );
        let artifact = artifact
            .with_computed_score()
            .expect("computed artifact is valid");

        assert_eq!(artifact.score, None);
        assert_eq!(artifact.status, ReviewStatus::ReviewError);
        assert_eq!(artifact.angle_errors.len(), 1);
        assert!(artifact.angle_errors[0].retryable);

        let mut missing_error_detail = artifact.clone();
        missing_error_detail.angle_errors.clear();
        assert!(
            missing_error_detail.validate().is_err(),
            "review_error requires typed angle error details"
        );
        let mut numeric_review_error = artifact.clone();
        numeric_review_error.score = Some(0);
        assert!(
            numeric_review_error.validate().is_err(),
            "review_error cannot expose a numeric score"
        );
    }

    #[test]
    fn inconclusive_summary_preserves_the_latest_valid_score() {
        let valid = ReviewArtifact {
            score: Some(4),
            target_score: 5,
            reviewed_sha: "valid-sha".to_string(),
            status: ReviewStatus::NeedsChanges,
            verdict: "One valid finding remains.".to_string(),
            models: vec!["balanced".to_string()],
            estimated_cost_usd: None,
            cost_summary: None,
            metrics: None,
            review_stages: vec![],
            angle_results: vec![],
            angle_errors: vec![],
            findings: vec![],
            notes: vec![],
        };
        let previous = SummaryState::for_artifact(&valid, None, 20).expect("valid state");
        let inconclusive = ReviewArtifact {
            score: None,
            target_score: 5,
            reviewed_sha: "error-sha".to_string(),
            status: ReviewStatus::ReviewError,
            verdict: "ReviewGate could not complete every enabled review angle.".to_string(),
            models: vec!["balanced".to_string()],
            estimated_cost_usd: None,
            cost_summary: None,
            metrics: None,
            review_stages: vec![],
            angle_results: vec![],
            angle_errors: vec![ReviewAngleError {
                angle_id: "general".to_string(),
                angle_name: "General".to_string(),
                kind: ReviewErrorKind::Timeout,
                retryable: true,
                message: "The reviewer timed out.".to_string(),
                model: "balanced".to_string(),
            }],
            findings: vec![],
            notes: vec![],
        };

        let summary =
            render_summary_with_options(&inconclusive, SummaryOptions::default(), Some(&previous))
                .expect("summary renders");
        let current = extract_summary_state(&summary)
            .expect("state parses")
            .expect("state exists");

        assert_eq!(current.last_valid_score, Some(4));
        assert_eq!(
            current.last_valid_reviewed_sha.as_deref(),
            Some("valid-sha")
        );
        assert!(summary.contains("Review incomplete"));
        assert!(summary.contains("Latest valid score: 4/5"));
        assert!(!summary.contains("Confidence Score: 0/5"));
    }

    #[test]
    fn legacy_failed_status_deserializes_for_recomputation_only() {
        let raw = serde_json::json!({
            "score": 3,
            "target_score": 5,
            "reviewed_sha": "abc123",
            "status": concat!("fail", "ed"),
            "verdict": "Legacy artifact.",
            "models": ["balanced"],
            "findings": [],
            "notes": []
        });

        let artifact: ReviewArtifact =
            serde_json::from_value(raw).expect("legacy status should deserialize");

        assert_eq!(artifact.status, ReviewStatus::NeedsChanges);

        let artifact = artifact
            .with_computed_score()
            .expect("computed artifact is valid");
        let serialized = serde_json::to_string(&artifact).expect("artifact serializes");

        assert_eq!(artifact.status, ReviewStatus::Passed);
        assert!(!serialized.contains(concat!("\"", "fail", "ed", "\"")));
        assert!(serialized.contains("\"passed\""));
    }

    #[test]
    fn computed_status_uses_fixed_five_point_target() {
        let artifact = ReviewArtifact {
            score: Some(5),
            target_score: 4,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::Passed,
            verdict: "Security issues still need changes.".to_string(),
            models: vec!["balanced".to_string()],
            estimated_cost_usd: None,
            cost_summary: None,
            metrics: None,
            review_stages: vec![],
            angle_results: vec![],
            angle_errors: vec![],
            findings: vec![Finding {
                id: "rg_001".to_string(),
                angle_id: None,
                scope: FindingScope::Line,
                severity: Severity::P3,
                confidence: 0.95,
                grounding: None,
                file: Some("src/lib.rs".to_string()),
                line: Some(42),
                title: "Security issue".to_string(),
                detail: None,
                agent_instruction: "Fix the security issue.".to_string(),
            }],
            notes: vec![],
        };

        let artifact = artifact
            .with_computed_score()
            .expect("computed artifact is valid");

        assert_eq!(artifact.score, Some(4));
        assert_eq!(artifact.target_score, DEFAULT_TARGET_SCORE);
        assert_eq!(artifact.status, ReviewStatus::NeedsChanges);
    }

    #[test]
    fn validation_rejects_out_of_range_confidence() {
        let artifact = ReviewArtifact {
            score: Some(5),
            target_score: 5,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::Passed,
            verdict: "Invalid finding confidence.".to_string(),
            models: vec!["balanced".to_string()],
            estimated_cost_usd: None,
            cost_summary: None,
            metrics: None,
            review_stages: vec![],
            angle_results: vec![],
            angle_errors: vec![],
            findings: vec![Finding {
                id: "rg_001".to_string(),
                angle_id: None,
                scope: FindingScope::Line,
                severity: Severity::P4,
                confidence: 1.2,
                grounding: None,
                file: None,
                line: None,
                title: "Invalid confidence".to_string(),
                detail: None,
                agent_instruction: "Fix the confidence value.".to_string(),
            }],
            notes: vec![],
        };

        assert!(matches!(
            artifact.validate(),
            Err(ReviewGateError::InvalidConfidence(value)) if value == 1.2
        ));
    }

    #[test]
    fn validation_rejects_negative_estimated_cost() {
        let artifact = ReviewArtifact {
            score: Some(5),
            target_score: 5,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::Passed,
            verdict: "Invalid cost.".to_string(),
            models: vec!["balanced".to_string()],
            estimated_cost_usd: Some(-0.01),
            cost_summary: None,
            metrics: None,
            review_stages: vec![],
            angle_results: vec![],
            angle_errors: vec![],
            findings: vec![],
            notes: vec![],
        };

        assert!(matches!(
            artifact.validate(),
            Err(ReviewGateError::InvalidEstimatedCost(value)) if value == -0.01
        ));
    }

    #[test]
    fn model_presets_have_explicit_defaults() {
        assert_eq!(ModelPreset::Cheap.as_str(), "cheap");
        assert_eq!(ModelPreset::Cheap.default_model(), "qwen/qwen3-coder");
        assert_eq!(
            ModelPreset::Balanced.default_model(),
            "deepseek/deepseek-v4-flash"
        );
        assert_eq!(
            ModelPreset::Strong.default_model(),
            "anthropic/claude-sonnet-4"
        );
    }

    #[test]
    fn estimates_cost_from_fallback_model_pricing() {
        let cost = estimate_model_cost_usd("deepseek/deepseek-v4-flash", 1_000_000, 500_000)
            .expect("pricing is valid")
            .expect("fallback pricing exists");

        assert!((cost - 0.18).abs() < f64::EPSILON);
    }

    #[test]
    fn parses_openrouter_model_pricing_response() {
        let response = serde_json::json!({
            "data": [
                {
                    "id": "deepseek/deepseek-v4-flash",
                    "pricing": {
                        "prompt": "0.00000009",
                        "completion": "0.00000018"
                    }
                }
            ]
        });

        let pricing = parse_openrouter_model_pricing(&response, "deepseek/deepseek-v4-flash")
            .expect("pricing parses")
            .expect("model exists");

        assert_eq!(
            pricing,
            ModelPricing {
                prompt_usd_per_million: 0.09,
                completion_usd_per_million: 0.18,
            }
        );
    }

    #[test]
    fn keeps_per_million_model_pricing_values() {
        let response = serde_json::json!({
            "data": [
                {
                    "id": "custom/model",
                    "pricing": {
                        "prompt": 0.09,
                        "completion": 0.18
                    }
                }
            ]
        });

        let pricing = parse_openrouter_model_pricing(&response, "custom/model")
            .expect("pricing parses")
            .expect("model exists");

        assert_eq!(
            pricing,
            ModelPricing {
                prompt_usd_per_million: 0.09,
                completion_usd_per_million: 0.18,
            }
        );
    }

    #[test]
    fn computes_metrics_from_findings_and_cost() {
        let artifact = ReviewArtifact {
            score: Some(3),
            target_score: 5,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::NeedsChanges,
            verdict: "One issue remains.".to_string(),
            models: vec!["deepseek/deepseek-v4-flash".to_string()],
            estimated_cost_usd: None,
            cost_summary: Some(CostSummary {
                current_run_usd: 0.02,
                source: Some(CostSource::FallbackPricing),
                components: vec![],
            }),
            metrics: None,
            review_stages: vec![],
            angle_results: vec![],
            angle_errors: vec![],
            findings: vec![
                Finding {
                    id: "rg_001".to_string(),
                    angle_id: None,
                    scope: FindingScope::Line,
                    severity: Severity::P2,
                    confidence: 0.9,
                    grounding: None,
                    file: Some("src/lib.rs".to_string()),
                    line: Some(42),
                    title: "Missing test".to_string(),
                    detail: None,
                    agent_instruction: "Add the missing test.".to_string(),
                },
                Finding {
                    id: "rg_002".to_string(),
                    angle_id: None,
                    scope: FindingScope::Line,
                    severity: Severity::P4,
                    confidence: 0.8,
                    grounding: None,
                    file: None,
                    line: None,
                    title: "Style note".to_string(),
                    detail: None,
                    agent_instruction: "Consider a rename later.".to_string(),
                },
            ],
            notes: vec![],
        };

        let metrics = compute_metrics(&artifact, Severity::P2);

        assert_eq!(metrics.finding_count, 2);
        assert_eq!(metrics.blocking_finding_count, 1);
        assert_eq!(metrics.inline_eligible_count, 1);
        assert_eq!(metrics.p2_count, 1);
        assert_eq!(metrics.p4_count, 1);
        assert_eq!(metrics.current_run_cost_usd, Some(0.02));
        assert_eq!(metrics.cost_source, CostSource::FallbackPricing);
    }

    #[test]
    fn summary_metrics_are_recomputed_from_render_options() {
        let artifact = ReviewArtifact {
            score: Some(3),
            target_score: 5,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::NeedsChanges,
            verdict: "Clean.".to_string(),
            models: vec!["deepseek/deepseek-v4-flash".to_string()],
            estimated_cost_usd: None,
            cost_summary: None,
            metrics: Some(ReviewMetrics {
                finding_count: 1,
                blocking_finding_count: 0,
                inline_eligible_count: 1,
                p0_count: 0,
                p1_count: 0,
                p2_count: 1,
                p3_count: 0,
                p4_count: 0,
                analyzed_line_count: Some(1_234),
                current_run_cost_usd: None,
                cost_source: CostSource::Unknown,
            }),
            review_stages: vec![],
            angle_results: vec![],
            angle_errors: vec![],
            findings: vec![Finding {
                id: "rg_001".to_string(),
                angle_id: None,
                scope: FindingScope::Line,
                severity: Severity::P2,
                confidence: 0.9,
                grounding: None,
                file: Some("src/lib.rs".to_string()),
                line: Some(42),
                title: "Lower severity finding".to_string(),
                detail: None,
                agent_instruction: "Review when convenient.".to_string(),
            }],
            notes: vec![],
        };

        let summary = render_summary_with_options(
            &artifact,
            SummaryOptions {
                min_severity: Severity::P1,
                ..SummaryOptions::default()
            },
            None,
        )
        .expect("summary renders");

        assert!(summary.contains("Findings: 1 total, 1 blocking, 0 inline candidates"));
        assert!(summary.contains("Changed lines analyzed: 1,234."));
    }

    #[test]
    fn openrouter_secret_debug_is_redacted() {
        let config = OpenRouterConfig::byok("sk-or-secret", ModelPreset::Balanced);

        assert_eq!(config.bearer_header(), "Bearer sk-or-secret");
        assert_eq!(
            config.chat_completions_url(),
            "https://openrouter.ai/api/v1/chat/completions"
        );
        assert_eq!(format!("{:?}", config.api_key), "SecretString([redacted])");
        assert!(!format!("{config:?}").contains("sk-or-secret"));
    }

    #[derive(Debug, Default)]
    struct MockOpenRouterTransport {
        seen_model: Option<String>,
        seen_auth: Option<String>,
    }

    impl OpenRouterTransport for MockOpenRouterTransport {
        type Error = std::convert::Infallible;

        fn send_chat_completion(
            &mut self,
            config: &OpenRouterConfig,
            request: &OpenRouterChatRequest,
        ) -> Result<String, Self::Error> {
            self.seen_model = Some(request.model.clone());
            self.seen_auth = Some(config.bearer_header());
            Ok("mock review".to_string())
        }
    }

    #[test]
    fn openrouter_client_uses_mockable_transport_without_logging_secret() {
        let transport = MockOpenRouterTransport::default();
        let config = OpenRouterConfig::byok("sk-or-secret", ModelPreset::Cheap);
        let mut client = OpenRouterClient::new(config, transport);

        let response = client
            .review_prompt("Review this diff")
            .expect("mock transport succeeds");

        assert_eq!(response, "mock review");
        assert_eq!(
            client.transport.seen_model.as_deref(),
            Some("qwen/qwen3-coder")
        );
        assert_eq!(
            client.transport.seen_auth.as_deref(),
            Some("Bearer sk-or-secret")
        );
    }

    #[test]
    fn grounding_serializes_absent_proof_fields_as_schema_nulls() {
        let grounding = FindingGrounding {
            claim: "Checked claim.".to_string(),
            causal_path: "change -> failure".to_string(),
            test_assessment: "No test covers the path.".to_string(),
            evidence: vec![],
            related_tests: vec![],
            reproduction: None,
            proof: None,
        };

        let value = serde_json::to_value(grounding).expect("grounding serializes");

        assert_eq!(value["reproduction"], serde_json::Value::Null);
        assert_eq!(value["proof"], serde_json::Value::Null);
    }
}
