use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const SUMMARY_MARKER: &str = "<!-- reviewgate-summary -->";
pub const SUMMARY_STATE_PREFIX: &str = "<!-- reviewgate-state ";
pub const SUMMARY_STATE_SUFFIX: &str = " -->";
pub const DEFAULT_COST_HISTORY_LIMIT: usize = 20;
pub const OPENROUTER_API_KEY_ENV: &str = "OPENROUTER_API_KEY";
pub const OPENROUTER_CHAT_COMPLETIONS_PATH: &str = "/chat/completions";
pub const OPENROUTER_DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";
pub const OPENROUTER_MODELS_PATH: &str = "/models";
pub const OPENROUTER_APP_REFERER: &str = "https://github.com/LVTD-LLC/reviewgate";
pub const OPENROUTER_APP_TITLE: &str = "ReviewGate";
pub const OPENROUTER_APP_CATEGORIES: &str = "cli-agent,cloud-agent";

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
    #[error(
        "fail_under must be less than or equal to target_score, got fail_under={fail_under} target_score={target_score}"
    )]
    InvalidThreshold { fail_under: u8, target_score: u8 },
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
    NeedsChanges,
    Failed,
}

impl ReviewStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReviewStatus::Passed => "passed",
            ReviewStatus::NeedsChanges => "needs_changes",
            ReviewStatus::Failed => "failed",
        }
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

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Finding {
    pub id: String,
    pub severity: Severity,
    pub confidence: f64,
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
pub struct ReviewArtifact {
    pub score: u8,
    pub target_score: u8,
    pub fail_under: u8,
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
    pub findings: Vec<Finding>,
    pub notes: Vec<String>,
}

impl ReviewArtifact {
    pub fn validate(&self) -> Result<(), ReviewGateError> {
        validate_score(self.score)?;
        validate_score(self.target_score)?;
        validate_score(self.fail_under)?;
        if self.fail_under > self.target_score {
            return Err(ReviewGateError::InvalidThreshold {
                fail_under: self.fail_under,
                target_score: self.target_score,
            });
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
        for finding in &self.findings {
            finding.validate()?;
        }
        Ok(())
    }

    pub fn with_computed_score(mut self) -> Result<Self, ReviewGateError> {
        self.score = compute_score(&self.findings);
        self.status = if self.score < self.fail_under {
            ReviewStatus::Failed
        } else if self.score >= self.target_score {
            ReviewStatus::Passed
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

pub fn compute_metrics(
    artifact: &ReviewArtifact,
    inline_min_severity: Severity,
    inline_min_confidence: f64,
) -> ReviewMetrics {
    let mut metrics = ReviewMetrics {
        finding_count: artifact.findings.len() as u32,
        blocking_finding_count: 0,
        inline_eligible_count: 0,
        p0_count: 0,
        p1_count: 0,
        p2_count: 0,
        p3_count: 0,
        p4_count: 0,
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
        if finding.is_blocking(artifact.fail_under) {
            metrics.blocking_finding_count += 1;
        }
        if is_inline_comment_eligible(finding, inline_min_severity, inline_min_confidence) {
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

pub fn is_inline_comment_eligible(
    finding: &Finding,
    inline_min_severity: Severity,
    inline_min_confidence: f64,
) -> bool {
    finding.file.is_some()
        && finding.line.is_some()
        && finding.confidence >= inline_min_confidence
        && finding.severity.is_at_or_above(inline_min_severity)
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

        let mut state = SummaryState {
            version: 1,
            last_reviewed_sha: artifact.reviewed_sha.clone(),
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
        validate_estimated_cost(self.cumulative_cost_usd)?;
        for run in &self.cost_history {
            run.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummaryStyle {
    Concise,
    Detailed,
}

impl SummaryStyle {
    pub fn as_str(&self) -> &'static str {
        match self {
            SummaryStyle::Concise => "concise",
            SummaryStyle::Detailed => "detailed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SummaryOptions {
    pub summary_min_severity: Severity,
    pub inline_min_severity: Severity,
    pub inline_min_confidence: f64,
    pub cost_history_limit: usize,
    pub summary_style: SummaryStyle,
}

impl Default for SummaryOptions {
    fn default() -> Self {
        Self {
            summary_min_severity: Severity::P4,
            inline_min_severity: Severity::P2,
            inline_min_confidence: 0.8,
            cost_history_limit: DEFAULT_COST_HISTORY_LIMIT,
            summary_style: SummaryStyle::Concise,
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
    let state: SummaryState = serde_json::from_str(raw)
        .map_err(|error| ReviewGateError::InvalidSummaryState(error.to_string()))?;
    state.validate()?;
    Ok(Some(state))
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
    validate_confidence(options.inline_min_confidence)?;
    let state = SummaryState::for_artifact(artifact, previous_state, options.cost_history_limit)?;
    let state_json = serde_json::to_string(&state)
        .map_err(|error| ReviewGateError::InvalidSummaryState(error.to_string()))?;

    let mut output = String::new();
    render_summary_header(&mut output, artifact, &state_json);
    match options.summary_style {
        SummaryStyle::Concise => {
            render_concise_summary_body(&mut output, artifact, options, &state);
        }
        SummaryStyle::Detailed => {
            render_detailed_summary_body(&mut output, artifact, options, &state);
        }
    }

    Ok(output)
}

fn render_summary_header(output: &mut String, artifact: &ReviewArtifact, state_json: &str) {
    output.push_str(SUMMARY_MARKER);
    output.push_str("\n\n");
    output.push_str(SUMMARY_STATE_PREFIX);
    output.push_str(state_json);
    output.push_str(SUMMARY_STATE_SUFFIX);
    output.push_str("\n\n");
    output.push_str(&format!("# ReviewGate: {}/5\n\n", artifact.score));
}

fn render_concise_summary_body(
    output: &mut String,
    artifact: &ReviewArtifact,
    options: SummaryOptions,
    state: &SummaryState,
) {
    let metrics = compute_metrics(
        artifact,
        options.inline_min_severity,
        options.inline_min_confidence,
    );

    output.push_str(artifact.verdict.trim());
    output.push_str("\n\n");
    output.push_str(&format!(
        "Status: `{}` | Target: {}/5 | Fail under: {}/5 | Reviewed: `{}`\n\n",
        artifact.status.as_str(),
        artifact.target_score,
        artifact.fail_under,
        artifact.reviewed_sha
    ));
    output.push_str(&format!(
        "Cost: {} ({}) | Findings: {} total, {} blocking, {} inline-eligible\n",
        format_cost(state.cumulative_cost_usd),
        format_run_count(state.run_count),
        metrics.finding_count,
        metrics.blocking_finding_count,
        metrics.inline_eligible_count
    ));
    if !artifact.notes.is_empty() {
        output.push_str(&format!(
            "Notes: {} note(s) in the JSON artifact.\n",
            artifact.notes.len()
        ));
    }

    let fallback_findings = fallback_summary_findings(artifact, options);
    if fallback_findings.is_empty() {
        output.push('\n');
        if metrics.finding_count == 0 {
            output.push_str("No findings. Re-run ReviewGate if new commits land.\n");
        } else if metrics.inline_eligible_count > 0 {
            output.push_str(
                "Eligible line-specific findings are posted inline. See the JSON artifact for the full machine-readable review.\n",
            );
        } else {
            output.push_str(
                "No fallback findings meet the summary visibility floor. See the JSON artifact for the full machine-readable review.\n",
            );
        }
        return;
    }

    output.push('\n');
    output.push_str(&format!(
        "{} {} kept here because {} not eligible for inline comments.\n\n",
        fallback_findings.len(),
        if fallback_findings.len() == 1 {
            "finding is"
        } else {
            "findings are"
        },
        if fallback_findings.len() == 1 {
            "it is"
        } else {
            "they are"
        }
    ));
    output.push_str("Fallback findings:\n");
    for finding in fallback_findings {
        output.push_str(&format!(
            "- {}: {}",
            finding.severity.as_str(),
            finding.title
        ));
        append_finding_location(output, finding);
        output.push_str(&format!(" - {}", inline_skip_reason(finding, options)));
        let instruction = finding.agent_instruction.trim();
        if !instruction.is_empty() {
            output.push_str(&format!(". {instruction}"));
        }
        output.push('\n');
    }
}

fn render_detailed_summary_body(
    output: &mut String,
    artifact: &ReviewArtifact,
    options: SummaryOptions,
    state: &SummaryState,
) {
    output.push_str(&format!("Reviewed commit: `{}`  \n", artifact.reviewed_sha));
    output.push_str(&format!("Status: `{}`  \n", artifact.status.as_str()));
    output.push_str(&format!("Target: {}/5  \n", artifact.target_score));
    output.push_str(&format!("Fail under: {}/5  \n", artifact.fail_under));
    output.push_str(&format!(
        "Summary visibility: {} and above  \n",
        options.summary_min_severity.as_str()
    ));
    output.push_str(&format!("Models: {}  \n", artifact.models.join(", ")));
    if let Some(cost_summary) = &artifact.cost_summary {
        output.push_str(&format!(
            "Current run cost: ${:.4}  \n",
            cost_summary.current_run_usd
        ));
    } else if let Some(cost) = artifact.estimated_cost_usd {
        output.push_str(&format!("Estimated model cost: ${cost:.4}\n"));
    }
    output.push('\n');
    output.push_str("## Verdict\n\n");
    output.push_str(&artifact.verdict);
    output.push_str("\n\n");

    if let Some(cost_summary) = &artifact.cost_summary {
        output.push_str("## Cost\n\n");
        output.push_str(&format!(
            "- Current run: ${:.4}\n",
            cost_summary.current_run_usd
        ));
        output.push_str(&format!(
            "- Cumulative PR review cost: ${:.4} across {} run(s)\n",
            state.cumulative_cost_usd, state.run_count
        ));
        if !cost_summary.components.is_empty() {
            output.push_str("- Components:\n");
            for component in &cost_summary.components {
                output.push_str(&format!(
                    "  - {} (`{}`): ${:.4}",
                    component.label, component.model, component.estimated_cost_usd
                ));
                if let (Some(prompt), Some(completion)) =
                    (component.prompt_tokens, component.completion_tokens)
                {
                    output.push_str(&format!(
                        " ({prompt} prompt, {completion} completion tokens)"
                    ));
                }
                output.push('\n');
            }
        }
        if let Some(source) = cost_summary.source {
            output.push_str(&format!("- Source: `{}`\n", source.as_str()));
        }
        output.push('\n');
    }

    let computed_metrics = compute_metrics(
        artifact,
        options.inline_min_severity,
        options.inline_min_confidence,
    );
    let metrics = &computed_metrics;
    {
        output.push_str("## Metrics\n\n");
        output.push_str(&format!(
            "- Findings: {} total, {} blocking, {} inline-eligible\n",
            metrics.finding_count, metrics.blocking_finding_count, metrics.inline_eligible_count
        ));
        output.push_str(&format!(
            "- Severity mix: P0 {}, P1 {}, P2 {}, P3 {}, P4 {}\n",
            metrics.p0_count,
            metrics.p1_count,
            metrics.p2_count,
            metrics.p3_count,
            metrics.p4_count
        ));
        output.push('\n');
    }

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
        .filter(|finding| {
            finding
                .severity
                .is_at_or_above(options.summary_min_severity)
        })
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
    let visible_findings: Vec<&Finding> = artifact
        .findings
        .iter()
        .filter(|finding| {
            let is_blocking = finding.is_blocking(artifact.fail_under);
            let is_visible = finding
                .severity
                .is_at_or_above(options.summary_min_severity);
            is_blocking || is_visible
        })
        .collect();
    if visible_findings.is_empty() {
        output.push_str(
            "No visible findings remain at the configured summary severity floor. Re-run ReviewGate if new commits land.\n",
        );
    } else {
        for (index, finding) in visible_findings.iter().enumerate() {
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
            output.push_str("Re-run ReviewGate after pushing if new commits land.\n");
        } else {
            output.push_str("Fix the blocking findings first. Re-run ReviewGate after pushing.\n");
        }
    }
}

fn fallback_summary_findings(artifact: &ReviewArtifact, options: SummaryOptions) -> Vec<&Finding> {
    artifact
        .findings
        .iter()
        .filter(|finding| {
            finding.is_blocking(artifact.fail_under)
                || finding
                    .severity
                    .is_at_or_above(options.summary_min_severity)
        })
        .filter(|finding| {
            !is_inline_comment_eligible(
                finding,
                options.inline_min_severity,
                options.inline_min_confidence,
            )
        })
        .collect()
}

fn inline_skip_reason(finding: &Finding, options: SummaryOptions) -> &'static str {
    if finding.file.is_none() || finding.line.is_none() {
        "no eligible line anchor"
    } else if !finding.severity.is_at_or_above(options.inline_min_severity) {
        "below the inline severity floor"
    } else if finding.confidence < options.inline_min_confidence {
        "below the inline confidence floor"
    } else {
        "not posted inline"
    }
}

fn append_finding_location(output: &mut String, finding: &Finding) {
    if let (Some(file), Some(line)) = (&finding.file, finding.line) {
        output.push_str(&format!(" (`{}:{}`)", file, line));
    }
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
    fn p0_findings_cap_score_at_one() {
        let findings = vec![Finding {
            id: "rg_001".to_string(),
            severity: Severity::P0,
            confidence: 0.98,
            file: Some("src/auth.rs".to_string()),
            line: Some(7),
            title: "Authentication bypass".to_string(),
            detail: None,
            agent_instruction: "Fix the bypass before merge.".to_string(),
        }];

        assert_eq!(compute_score(&findings), 1);
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
            cost_summary: None,
            metrics: None,
            review_stages: vec![],
            findings: vec![],
            notes: vec![],
        };

        let summary = render_summary_with_options(
            &artifact,
            SummaryOptions {
                summary_style: SummaryStyle::Detailed,
                ..SummaryOptions::default()
            },
            None,
        )
        .expect("summary renders");
        assert!(summary.starts_with(SUMMARY_MARKER));
        assert!(summary.contains("# ReviewGate: 4/5"));
    }

    #[test]
    fn fixture_renders_default_concise_summary_shape() {
        let artifact: ReviewArtifact =
            serde_json::from_str(include_str!("../../../fixtures/simple-review.json"))
                .expect("fixture parses");
        let artifact = artifact.with_computed_score().expect("score computes");

        let summary = render_summary(&artifact).expect("summary renders");

        assert!(summary.contains("Cost: $0.08 (1 run)"));
        assert!(summary.contains("Findings: 2 total, 0 blocking, 1 inline-eligible"));
        assert!(!summary.contains("## Cost"));
        assert!(!summary.contains("## Metrics"));
        assert!(!summary.contains("## Blocking Findings"));
        assert!(!summary.contains("## Non-Blocking Notes"));
        assert!(!summary.contains("## Agent Instructions"));
        assert!(!summary.contains("Missing regression test for retry exhaustion"));
        assert!(summary.contains("Helper name is slightly vague"));
        assert!(summary.contains("below the inline severity floor"));
    }

    #[test]
    fn renders_structured_cost_summary() {
        let artifact = ReviewArtifact {
            score: 5,
            target_score: 5,
            fail_under: 4,
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
            findings: vec![],
            notes: vec![],
        };

        let summary = render_summary_with_options(
            &artifact,
            SummaryOptions {
                summary_style: SummaryStyle::Detailed,
                ..SummaryOptions::default()
            },
            None,
        )
        .expect("summary renders");

        assert!(summary.contains("Current run cost: $0.0123"));
        assert!(summary.contains("- general (`deepseek/deepseek-v4-flash`): $0.0123"));

        let concise = render_summary(&artifact).expect("concise summary renders");
        assert!(concise.contains("Cost: $0.01 (1 run)"));
        assert!(!concise.contains("## Cost"));
        assert!(!concise.contains("- general (`deepseek/deepseek-v4-flash`): $0.0123"));
    }

    #[test]
    fn extracts_and_carries_hidden_summary_state() {
        let artifact = ReviewArtifact {
            score: 5,
            target_score: 5,
            fail_under: 4,
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
        assert!(second.contains("Cost: $0.03 (2 runs)"));
    }

    #[test]
    fn summary_visibility_floor_hides_lower_severity_findings() {
        let artifact = ReviewArtifact {
            score: 4,
            target_score: 5,
            fail_under: 4,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::NeedsChanges,
            verdict: "One visible issue remains.".to_string(),
            models: vec!["balanced".to_string()],
            estimated_cost_usd: None,
            cost_summary: None,
            metrics: None,
            review_stages: vec![],
            findings: vec![
                Finding {
                    id: "rg_001".to_string(),
                    severity: Severity::P2,
                    confidence: 0.9,
                    file: None,
                    line: None,
                    title: "Visible reliability issue".to_string(),
                    detail: None,
                    agent_instruction: "Fix the reliability issue.".to_string(),
                },
                Finding {
                    id: "rg_002".to_string(),
                    severity: Severity::P4,
                    confidence: 0.9,
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
                summary_min_severity: Severity::P2,
                ..SummaryOptions::default()
            },
            None,
        )
        .expect("summary renders");

        assert!(summary.contains("Visible reliability issue"));
        assert!(summary.contains("no eligible line anchor"));
        assert!(!summary.contains("Hidden style note"));
    }

    #[test]
    fn summary_visibility_floor_never_hides_blocking_findings() {
        let artifact = ReviewArtifact {
            score: 4,
            target_score: 5,
            fail_under: 5,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::Failed,
            verdict: "A lower-severity issue still fails the configured gate.".to_string(),
            models: vec!["balanced".to_string()],
            estimated_cost_usd: None,
            cost_summary: None,
            metrics: None,
            review_stages: vec![],
            findings: vec![Finding {
                id: "rg_001".to_string(),
                severity: Severity::P3,
                confidence: 0.9,
                file: Some("src/lib.rs".to_string()),
                line: Some(42),
                title: "Gate-failing advisory finding".to_string(),
                detail: None,
                agent_instruction: "Fix or lower the configured gate policy.".to_string(),
            }],
            notes: vec![],
        };

        let summary = render_summary_with_options(
            &artifact,
            SummaryOptions {
                summary_min_severity: Severity::P2,
                ..SummaryOptions::default()
            },
            None,
        )
        .expect("summary renders");

        assert!(!summary.contains("## Blocking Findings"));
        assert!(summary.contains("Fallback findings:"));
        assert!(summary.contains("P3: Gate-failing advisory finding"));
        assert!(summary.contains("Fix or lower the configured gate policy."));
    }

    #[test]
    fn validation_rejects_empty_cost_component_model() {
        let artifact = ReviewArtifact {
            score: 5,
            target_score: 5,
            fail_under: 4,
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
            findings: vec![],
            notes: vec![],
        };

        assert!(matches!(
            artifact.validate(),
            Err(ReviewGateError::InvalidCostComponent { field: "model" })
        ));
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
            cost_summary: None,
            metrics: None,
            review_stages: vec![],
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

        assert!(!summary.contains("## Agent Instructions"));
        assert!(summary.contains("Eligible line-specific findings are posted inline."));

        let detailed = render_summary_with_options(
            &artifact,
            SummaryOptions {
                summary_style: SummaryStyle::Detailed,
                ..SummaryOptions::default()
            },
            None,
        )
        .expect("detailed summary renders");
        assert!(detailed.contains("## Agent Instructions"));
        assert!(
            detailed
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
            cost_summary: None,
            metrics: None,
            review_stages: vec![],
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

        let summary = render_summary_with_options(
            &artifact,
            SummaryOptions {
                summary_style: SummaryStyle::Detailed,
                ..SummaryOptions::default()
            },
            None,
        )
        .expect("summary renders");

        assert!(summary.contains("1. P3: Clarify the README example."));
        assert!(summary.contains("Re-run ReviewGate after pushing if new commits land."));
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
            cost_summary: None,
            metrics: None,
            review_stages: vec![],
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
    fn computed_status_treats_fail_under_as_hard_floor() {
        let artifact = ReviewArtifact {
            score: 5,
            target_score: 4,
            fail_under: 4,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::Passed,
            verdict: "Target score cannot bypass the failure threshold.".to_string(),
            models: vec!["balanced".to_string()],
            estimated_cost_usd: None,
            cost_summary: None,
            metrics: None,
            review_stages: vec![],
            findings: vec![Finding {
                id: "rg_001".to_string(),
                severity: Severity::P1,
                confidence: 0.95,
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

        assert_eq!(artifact.score, 2);
        assert_eq!(artifact.status, ReviewStatus::Failed);
    }

    #[test]
    fn validation_rejects_fail_under_above_target_score() {
        let artifact = ReviewArtifact {
            score: 5,
            target_score: 2,
            fail_under: 4,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::Passed,
            verdict: "Invalid thresholds.".to_string(),
            models: vec!["balanced".to_string()],
            estimated_cost_usd: None,
            cost_summary: None,
            metrics: None,
            review_stages: vec![],
            findings: vec![],
            notes: vec![],
        };

        assert!(matches!(
            artifact.validate(),
            Err(ReviewGateError::InvalidThreshold {
                fail_under: 4,
                target_score: 2
            })
        ));
    }

    #[test]
    fn validation_rejects_out_of_range_confidence() {
        let artifact = ReviewArtifact {
            score: 5,
            target_score: 5,
            fail_under: 4,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::Passed,
            verdict: "Invalid finding confidence.".to_string(),
            models: vec!["balanced".to_string()],
            estimated_cost_usd: None,
            cost_summary: None,
            metrics: None,
            review_stages: vec![],
            findings: vec![Finding {
                id: "rg_001".to_string(),
                severity: Severity::P4,
                confidence: 1.2,
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
            score: 5,
            target_score: 5,
            fail_under: 4,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::Passed,
            verdict: "Invalid cost.".to_string(),
            models: vec!["balanced".to_string()],
            estimated_cost_usd: Some(-0.01),
            cost_summary: None,
            metrics: None,
            review_stages: vec![],
            findings: vec![],
            notes: vec![],
        };

        assert!(matches!(
            artifact.validate(),
            Err(ReviewGateError::InvalidEstimatedCost(value)) if value == -0.01
        ));
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
            cost_summary: None,
            metrics: None,
            review_stages: vec![],
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

        let summary = render_summary_with_options(
            &artifact,
            SummaryOptions {
                summary_style: SummaryStyle::Detailed,
                ..SummaryOptions::default()
            },
            None,
        )
        .expect("summary renders");

        assert!(summary.contains("## Blocking Findings\n\nNone."));
        assert!(summary.contains("- P2: Missing regression test"));
        assert!(!summary.contains("Fix the blocking findings first."));
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
            score: 3,
            target_score: 5,
            fail_under: 4,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::Failed,
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
            findings: vec![
                Finding {
                    id: "rg_001".to_string(),
                    severity: Severity::P2,
                    confidence: 0.9,
                    file: Some("src/lib.rs".to_string()),
                    line: Some(42),
                    title: "Missing test".to_string(),
                    detail: None,
                    agent_instruction: "Add the missing test.".to_string(),
                },
                Finding {
                    id: "rg_002".to_string(),
                    severity: Severity::P4,
                    confidence: 0.8,
                    file: None,
                    line: None,
                    title: "Style note".to_string(),
                    detail: None,
                    agent_instruction: "Consider a rename later.".to_string(),
                },
            ],
            notes: vec![],
        };

        let metrics = compute_metrics(&artifact, Severity::P2, 0.8);

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
            score: 5,
            target_score: 5,
            fail_under: 3,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::Passed,
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
                current_run_cost_usd: None,
                cost_source: CostSource::Unknown,
            }),
            review_stages: vec![],
            findings: vec![Finding {
                id: "rg_001".to_string(),
                severity: Severity::P2,
                confidence: 0.9,
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
                inline_min_severity: Severity::P1,
                summary_style: SummaryStyle::Detailed,
                ..SummaryOptions::default()
            },
            None,
        )
        .expect("summary renders");

        assert!(summary.contains("Findings: 1 total, 0 blocking, 0 inline-eligible"));
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
}
