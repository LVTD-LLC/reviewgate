use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use reviewgate_core::{
    BENCHMARK_REPORT_SCHEMA_VERSION, BenchmarkCaseExpectation, BenchmarkConfigurationRole,
    BenchmarkExpectedFinding, BenchmarkKnownNonFinding, BenchmarkManifest,
    BenchmarkObservedFinding, BenchmarkObservedRun, BenchmarkPipeline, BenchmarkRunScore,
    ConfigurationMetrics, DEFAULT_TARGET_SCORE, ReviewAngleResult, ReviewArtifact, ReviewScope,
    ReviewStatus, configuration_metrics_from_scores, score_benchmark_run,
};
use serde::{Deserialize, Serialize};

use super::{
    CliResult, LiveCostInputs, PullRequestContext, ReviewContext, aggregate_angle_artifacts,
    append_failed_angle_reviews, general_review_angle, ground_artifact_findings,
    resolve_model_cost_inputs, run_live_angle_review_with_cached_pricing, safe_relative_path,
};

const MAX_LIVE_BENCHMARK_REQUESTS: usize = 100;

#[derive(Debug)]
pub struct EvalReplayOptions {
    pub repo: PathBuf,
    pub manifest: PathBuf,
    pub json_out: Option<PathBuf>,
    pub markdown_out: Option<PathBuf>,
    pub live: bool,
    pub model: Option<String>,
    pub openrouter_base_url: Option<String>,
    pub max_cases: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvidenceGroundingCase {
    #[serde(default, alias = "fixture_id")]
    case_id: Option<String>,
    name: String,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    risk_class: Option<String>,
    files: BTreeMap<String, String>,
    diff: String,
    finding: reviewgate_core::Finding,
    expected_blocking: bool,
    #[serde(default)]
    configuration_findings: BTreeMap<String, Vec<reviewgate_core::Finding>>,
    #[serde(default)]
    provenance: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Clone)]
struct EvaluatedRun {
    observed_run: BenchmarkObservedRun,
    score: BenchmarkRunScore,
    estimated_cost_usd: Option<f64>,
    spent_cost_usd: Option<f64>,
    latency_ms: Option<u64>,
    agent_time_ms: Option<u64>,
}

#[derive(Debug, Clone)]
struct EvaluatedCase {
    id: String,
    name: String,
    language: String,
    risk_class: String,
    expected_blocking: bool,
    provenance: Option<serde_json::Map<String, serde_json::Value>>,
    runs: BTreeMap<String, Vec<EvaluatedRun>>,
}

#[derive(Debug, Serialize)]
struct BenchmarkReport {
    schema_version: &'static str,
    manifest_name: String,
    mode: &'static str,
    passed: bool,
    corpus: CorpusReport,
    configurations: Vec<ConfigurationReport>,
    comparison: ComparisonReport,
    threshold_results: Vec<ThresholdResult>,
}

#[derive(Debug, Serialize)]
struct CorpusReport {
    case_count: usize,
    minimum_case_count: usize,
    blinded: bool,
    repetitions: usize,
    languages: Vec<String>,
    risk_classes: Vec<String>,
    source_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ConfigurationReport {
    id: String,
    role: BenchmarkConfigurationRole,
    pipeline: BenchmarkPipeline,
    metrics: ConfigurationMetrics,
    completion_rate: Option<f64>,
    rereview_stability: Option<f64>,
    rereview_convergence: Option<f64>,
    duplicate_finding_count: usize,
    estimated_cost_usd: Option<f64>,
    spent_cost_usd: Option<f64>,
    mean_latency_ms: Option<f64>,
    agent_time_ms: Option<u64>,
    agent_time_coverage: usize,
    cases: Vec<CaseReport>,
}

#[derive(Debug, Serialize)]
struct CaseReport {
    case_id: String,
    name: String,
    provenance: Option<serde_json::Map<String, serde_json::Value>>,
    expected_blocking: bool,
    review_complete: bool,
    observed_blocking: bool,
    true_blockers: usize,
    false_blockers: usize,
    missed_serious_defects: Vec<String>,
    contradicted_non_findings: Vec<String>,
    unexpected_blockers: Vec<String>,
    duplicate_findings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ComparisonReport {
    baseline_id: String,
    candidate_id: String,
    blocking_precision_delta: Option<f64>,
    serious_defect_recall_delta: Option<f64>,
    false_blockers_per_case_delta: f64,
    contradiction_rate_delta: Option<f64>,
    rereview_stability_delta: Option<f64>,
    estimated_cost_usd_delta: Option<f64>,
    mean_latency_ms_delta: Option<f64>,
}

#[derive(Debug, Serialize)]
struct ThresholdResult {
    name: String,
    passed: bool,
    actual: Option<f64>,
    required: String,
}

pub fn eval_replays(options: EvalReplayOptions) -> CliResult<()> {
    let repo = options
        .repo
        .canonicalize()
        .with_context(|| format!("failed to resolve repository {}", options.repo.display()))?;
    let manifest_path = confined_input_path(&repo, &options.manifest, "benchmark manifest")?;
    let raw = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let manifest: BenchmarkManifest = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    reviewgate_core::validate_manifest(&manifest).map_err(anyhow::Error::msg)?;
    if options.live {
        return evaluate_live(options, repo, manifest);
    }

    let mut cases = Vec::new();
    for source in &manifest.sources {
        let source_path = confined_input_path(&repo, Path::new(&source.path), "benchmark source")?;
        let raw = std::fs::read_to_string(&source_path)
            .with_context(|| format!("failed to read {}", source_path.display()))?;
        let mut source_cases = parse_evidence_cases(&raw)?;
        if let Some(max_cases) = options.max_cases {
            source_cases.truncate(max_cases.saturating_sub(cases.len()));
        }
        cases.extend(evaluate_evidence_cases(
            &source.id,
            source_cases,
            &manifest,
        )?);
        if options
            .max_cases
            .is_some_and(|maximum| cases.len() >= maximum)
        {
            break;
        }
    }
    let report = build_report(&manifest, &cases, "deterministic")?;
    write_reports(
        &report,
        options.json_out.as_deref(),
        options.markdown_out.as_deref(),
    )?;
    if !report.passed {
        bail!("benchmark replacement thresholds did not pass");
    }
    Ok(())
}

fn evaluate_live(
    options: EvalReplayOptions,
    repo: PathBuf,
    manifest: BenchmarkManifest,
) -> CliResult<()> {
    let api_key =
        require_live_api_key(std::env::var(reviewgate_core::OPENROUTER_API_KEY_ENV).ok())?;
    let model = options.model.clone().unwrap_or_else(|| {
        reviewgate_core::ModelPreset::Balanced
            .default_model()
            .to_string()
    });
    let base_url = options
        .openrouter_base_url
        .clone()
        .unwrap_or_else(|| reviewgate_core::OPENROUTER_DEFAULT_BASE_URL.to_string());
    eprintln!(
        "ReviewGate live benchmark: model={model}, maximum configured cost=${:.2}, maximum mean latency={}ms",
        manifest.thresholds.maximum_live_cost_usd, manifest.thresholds.maximum_mean_latency_ms,
    );
    let mut source_cases = Vec::new();
    let mut loaded_case_count = 0usize;
    for source in &manifest.sources {
        let source_path = confined_input_path(&repo, Path::new(&source.path), "benchmark source")?;
        let raw = std::fs::read_to_string(&source_path)
            .with_context(|| format!("failed to read {}", source_path.display()))?;
        let mut cases = parse_evidence_cases(&raw)?;
        if let Some(max_cases) = options.max_cases {
            cases.truncate(max_cases.saturating_sub(loaded_case_count));
        }
        loaded_case_count += cases.len();
        source_cases.push((source.id.clone(), cases));
        if options
            .max_cases
            .is_some_and(|maximum| loaded_case_count >= maximum)
        {
            break;
        }
    }
    validate_live_request_count(loaded_case_count, manifest.repetitions)?;
    let (pricing, source) =
        resolve_model_cost_inputs(&base_url, &api_key, &model, Duration::from_secs(15));
    if pricing.is_none() {
        bail!(
            "live benchmark cannot enforce maximum_live_cost_usd because pricing is unavailable for model `{model}`"
        );
    }
    let mut cumulative_cost_usd = 0.0;
    let cost_inputs = LiveCostInputs { pricing, source };
    let mut cases = Vec::with_capacity(loaded_case_count);
    for (source_id, source_cases) in source_cases {
        let mut live = LiveEvaluationContext {
            manifest: &manifest,
            base_url: &base_url,
            api_key: &api_key,
            model: &model,
            cost_inputs,
            cumulative_cost_usd: &mut cumulative_cost_usd,
        };
        cases.extend(evaluate_live_evidence_cases(
            &source_id,
            source_cases,
            &mut live,
        )?);
    }
    let report = build_report(&manifest, &cases, "live")?;
    write_reports(
        &report,
        options.json_out.as_deref(),
        options.markdown_out.as_deref(),
    )?;
    if !report.passed {
        bail!("live benchmark replacement thresholds did not pass");
    }
    Ok(())
}

fn validate_live_request_count(case_count: usize, repetitions: usize) -> CliResult<()> {
    let request_count = case_count
        .checked_mul(repetitions)
        .context("live benchmark request count overflowed")?;
    if request_count > MAX_LIVE_BENCHMARK_REQUESTS {
        bail!(
            "live benchmark requires {request_count} model requests; maximum is {MAX_LIVE_BENCHMARK_REQUESTS}. Use --max-cases or lower repetitions"
        );
    }
    Ok(())
}

fn require_live_api_key(value: Option<String>) -> CliResult<String> {
    value
        .filter(|value| !value.trim().is_empty())
        .with_context(|| {
            format!(
                "{} is required for explicit live benchmark mode",
                reviewgate_core::OPENROUTER_API_KEY_ENV
            )
        })
}

struct LiveEvaluationContext<'a> {
    manifest: &'a BenchmarkManifest,
    base_url: &'a str,
    api_key: &'a str,
    model: &'a str,
    cost_inputs: LiveCostInputs,
    cumulative_cost_usd: &'a mut f64,
}

fn evaluate_live_evidence_cases(
    source_id: &str,
    cases: Vec<EvidenceGroundingCase>,
    live: &mut LiveEvaluationContext<'_>,
) -> CliResult<Vec<EvaluatedCase>> {
    let mut evaluated = Vec::with_capacity(cases.len());
    for (index, case) in cases.into_iter().enumerate() {
        let case_id = case
            .case_id
            .clone()
            .unwrap_or_else(|| format!("{source_id}-{:03}", index + 1));
        let root = TempCaseRoot::new(&case_id)?;
        write_case_files(root.path(), &case.files)?;
        let context = review_context(&case, &case_id);
        let expectation = case_expectation(&case, &case_id)?;
        let language = case
            .language
            .clone()
            .unwrap_or_else(|| infer_language(case.files.keys()));
        let risk_class = case
            .risk_class
            .clone()
            .unwrap_or_else(|| case.finding.severity.as_str().to_ascii_lowercase());
        let mut runs = live
            .manifest
            .configurations
            .iter()
            .map(|configuration| (configuration.id.clone(), Vec::new()))
            .collect::<BTreeMap<_, _>>();

        for repetition in 1..=live.manifest.repetitions {
            if *live.cumulative_cost_usd >= live.manifest.thresholds.maximum_live_cost_usd {
                bail!(
                    "live benchmark stopped before request {} for case `{case_id}` because cumulative cost ${:.4} reached maximum_live_cost_usd ${:.4}",
                    repetition,
                    *live.cumulative_cost_usd,
                    live.manifest.thresholds.maximum_live_cost_usd,
                );
            }
            let angle = general_review_angle();
            let started = Instant::now();
            let result = run_live_angle_review_with_cached_pricing(
                &context,
                &angle,
                live.base_url,
                live.api_key,
                live.model,
                Duration::from_secs(180),
                live.cost_inputs,
            );
            let estimated_cost_usd = result.estimated_cost_usd;
            let spent_cost_usd = result.spent_cost_usd;
            *live.cumulative_cost_usd += spent_cost_usd.or(estimated_cost_usd).unwrap_or(0.0);
            let (raw_artifact, review_complete) = match result.review {
                Ok(artifact) => (
                    aggregate_angle_artifacts(&case_id, live.model, vec![(angle, artifact)])?,
                    true,
                ),
                Err(failure) => {
                    let mut artifact = aggregate_angle_artifacts(&case_id, live.model, Vec::new())?;
                    append_failed_angle_reviews(&mut artifact, live.model, vec![(angle, failure)])?;
                    (artifact, false)
                }
            };
            let latency_ms = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);

            for configuration in &live.manifest.configurations {
                let mut artifact = raw_artifact.clone();
                if configuration.pipeline == BenchmarkPipeline::EvidenceGate {
                    ground_artifact_findings(root.path(), &context, &mut artifact)?;
                }
                let findings = observed_findings(&artifact);
                let observed_run = BenchmarkObservedRun {
                    case_id: case_id.clone(),
                    repetition,
                    review_complete,
                    findings: findings.clone(),
                };
                let score =
                    score_benchmark_run(&expectation, &observed_run).map_err(anyhow::Error::msg)?;
                runs.get_mut(&configuration.id)
                    .expect("configuration map initialized")
                    .push(EvaluatedRun {
                        observed_run,
                        score,
                        estimated_cost_usd,
                        spent_cost_usd,
                        latency_ms: Some(latency_ms),
                        agent_time_ms: None,
                    });
            }
        }

        evaluated.push(EvaluatedCase {
            id: case_id,
            name: case.name,
            language,
            risk_class,
            expected_blocking: case.expected_blocking,
            provenance: case.provenance,
            runs,
        });
    }
    Ok(evaluated)
}

fn confined_input_path(repo: &Path, input: &Path, label: &str) -> CliResult<PathBuf> {
    let relative = if input.is_absolute() {
        input.strip_prefix(repo).with_context(|| {
            format!(
                "{label} {} must stay inside {}",
                input.display(),
                repo.display()
            )
        })?
    } else {
        input
    };
    let relative = relative.to_string_lossy().replace('\\', "/");
    super::confined_repo_file(repo, &relative).with_context(|| {
        format!("{label} `{relative}` must be a regular non-symlink repository file")
    })
}

fn write_reports(
    report: &BenchmarkReport,
    json_out: Option<&Path>,
    markdown_out: Option<&Path>,
) -> CliResult<()> {
    let json = serde_json::to_string_pretty(report)?;
    if let Some(path) = json_out {
        write_output(path, &json, "benchmark JSON report")?;
    } else {
        println!("{json}");
    }
    if let Some(path) = markdown_out {
        write_output(
            path,
            &render_markdown_report(report),
            "benchmark Markdown report",
        )?;
    }
    Ok(())
}

fn write_output(path: &Path, contents: &str, label: &str) -> CliResult<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    std::fs::write(path, contents)
        .with_context(|| format!("failed to write {label} {}", path.display()))
}

fn render_markdown_report(report: &BenchmarkReport) -> String {
    let mut output = format!(
        "# ReviewGate replacement benchmark\n\n- Result: **{}**\n- Mode: `{}`\n- Corpus: {} cases, {} repetitions, blinded: {}\n\n",
        if report.passed { "PASS" } else { "FAIL" },
        report.mode,
        report.corpus.case_count,
        report.corpus.repetitions,
        report.corpus.blinded,
    );
    output.push_str("| Configuration | Precision | Serious recall | False blockers/case | Contradictions | Stability | Cost (USD) | Mean latency (ms) |\n");
    output.push_str("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for configuration in &report.configurations {
        output.push_str(&format!(
            "| {} | {} | {} | {:.4} | {} | {} | {} | {} |\n",
            configuration.id,
            format_rate(configuration.metrics.blocking_precision),
            format_rate(configuration.metrics.serious_defect_recall),
            configuration.metrics.false_blockers_per_case,
            format_rate(configuration.metrics.contradiction_rate),
            format_rate(configuration.rereview_stability),
            format_optional(
                configuration
                    .spent_cost_usd
                    .or(configuration.estimated_cost_usd)
            ),
            format_optional(configuration.mean_latency_ms),
        ));
    }
    output.push_str("\n## Rollout gate\n\n");
    for threshold in &report.threshold_results {
        output.push_str(&format!(
            "- [{}] `{}`: actual {}, required {}\n",
            if threshold.passed { "x" } else { " " },
            threshold.name,
            format_optional(threshold.actual),
            threshold.required,
        ));
    }
    output
}

fn format_rate(value: Option<f64>) -> String {
    value
        .map(|value| format!("{:.2}%", value * 100.0))
        .unwrap_or_else(|| "n/a".to_string())
}

fn format_optional(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.4}"))
        .unwrap_or_else(|| "n/a".to_string())
}

fn evaluate_evidence_cases(
    source_id: &str,
    cases: Vec<EvidenceGroundingCase>,
    manifest: &BenchmarkManifest,
) -> CliResult<Vec<EvaluatedCase>> {
    let mut evaluated = Vec::with_capacity(cases.len());
    for (index, case) in cases.into_iter().enumerate() {
        let case_id = case
            .case_id
            .clone()
            .unwrap_or_else(|| format!("{source_id}-{:03}", index + 1));
        let root = TempCaseRoot::new(&case_id)?;
        write_case_files(root.path(), &case.files)?;
        let context = review_context(&case, &case_id);
        let expectation = case_expectation(&case, &case_id)?;
        let language = case
            .language
            .clone()
            .unwrap_or_else(|| infer_language(case.files.keys()));
        let risk_class = case
            .risk_class
            .clone()
            .unwrap_or_else(|| case.finding.severity.as_str().to_ascii_lowercase());
        let mut runs = BTreeMap::new();

        for configuration in &manifest.configurations {
            let configured_findings = case
                .configuration_findings
                .get(&configuration.id)
                .cloned()
                .unwrap_or_else(|| vec![case.finding.clone()]);
            let mut configuration_runs = Vec::with_capacity(manifest.repetitions);
            let mut artifact = artifact_from_findings(&case_id, configured_findings)?;
            if configuration.pipeline == BenchmarkPipeline::EvidenceGate {
                ground_artifact_findings(root.path(), &context, &mut artifact)?;
            }
            let observed_run = BenchmarkObservedRun {
                case_id: case_id.clone(),
                repetition: 1,
                review_complete: artifact.angle_errors.is_empty(),
                findings: observed_findings(&artifact),
            };
            let score =
                score_benchmark_run(&expectation, &observed_run).map_err(anyhow::Error::msg)?;
            let run = EvaluatedRun {
                observed_run,
                score,
                estimated_cost_usd: Some(0.0),
                spent_cost_usd: Some(0.0),
                latency_ms: Some(0),
                agent_time_ms: None,
            };
            for repetition in 1..=manifest.repetitions {
                let mut repeated_run = run.clone();
                repeated_run.observed_run.repetition = repetition;
                configuration_runs.push(repeated_run);
            }
            runs.insert(configuration.id.clone(), configuration_runs);
        }

        evaluated.push(EvaluatedCase {
            id: case_id,
            name: case.name,
            language,
            risk_class,
            expected_blocking: case.expected_blocking,
            provenance: case.provenance,
            runs,
        });
    }
    Ok(evaluated)
}

fn write_case_files(root: &Path, files: &BTreeMap<String, String>) -> CliResult<()> {
    for (relative, contents) in files {
        let Some(path) = safe_relative_path(relative) else {
            bail!("benchmark case file `{relative}` must be a safe repo-relative path");
        };
        let path = root.join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        std::fs::write(&path, contents)
            .with_context(|| format!("failed to write benchmark file {}", path.display()))?;
    }
    Ok(())
}

fn review_context(case: &EvidenceGroundingCase, case_id: &str) -> ReviewContext {
    ReviewContext {
        reviewed_sha: case_id.to_string(),
        scope: ReviewScope::Local,
        previous_state: None,
        convergence_delta: reviewgate_core::ConvergenceDelta::first_review(case_id),
        pull_request: PullRequestContext::default(),
        changed_files: case.files.keys().cloned().collect(),
        diff: case.diff.clone(),
        analyzed_line_count: case
            .diff
            .lines()
            .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
            .count()
            .try_into()
            .unwrap_or(u32::MAX),
        data_integrity_review_needed: false,
        context_files: vec![],
    }
}

fn case_expectation(
    case: &EvidenceGroundingCase,
    case_id: &str,
) -> CliResult<BenchmarkCaseExpectation> {
    let grounding = case
        .finding
        .grounding
        .as_ref()
        .with_context(|| format!("benchmark case `{case_id}` finding must have grounding"))?;
    let expected = BenchmarkExpectedFinding {
        semantic_key: grounding.semantic_key.clone(),
        adjudicated_claim: grounding.claim.clone(),
        serious: case.expected_blocking,
        expected_blocking: case.expected_blocking,
    };
    Ok(if case.expected_blocking {
        BenchmarkCaseExpectation {
            case_id: case_id.to_string(),
            expected_findings: vec![expected],
            known_non_findings: vec![],
        }
    } else {
        BenchmarkCaseExpectation {
            case_id: case_id.to_string(),
            expected_findings: vec![],
            known_non_findings: vec![BenchmarkKnownNonFinding {
                semantic_key: expected.semantic_key,
                adjudicated_claim: expected.adjudicated_claim,
            }],
        }
    })
}

fn artifact_from_findings(
    reviewed_sha: &str,
    findings: Vec<reviewgate_core::Finding>,
) -> CliResult<ReviewArtifact> {
    let findings = findings
        .into_iter()
        .map(|mut finding| {
            finding.angle_id = Some("general".to_string());
            finding
        })
        .collect::<Vec<_>>();
    let finding_ids = findings.iter().map(|finding| finding.id.clone()).collect();
    let artifact = ReviewArtifact {
        score: None,
        target_score: DEFAULT_TARGET_SCORE,
        reviewed_sha: reviewed_sha.to_string(),
        status: ReviewStatus::ReviewError,
        verdict: "Captured benchmark response.".to_string(),
        models: vec!["benchmark/captured".to_string()],
        estimated_cost_usd: Some(0.0),
        cost_summary: None,
        metrics: None,
        review_stages: vec![],
        angle_results: vec![ReviewAngleResult {
            id: "general".to_string(),
            name: "General".to_string(),
            score: DEFAULT_TARGET_SCORE,
            status: ReviewStatus::Passed,
            verdict: "Captured benchmark response.".to_string(),
            model: "benchmark/captured".to_string(),
            finding_ids,
        }],
        angle_errors: vec![],
        findings,
        disposition_updates: vec![],
        tracked_findings: vec![],
        notes: vec![],
    };
    artifact.with_computed_score().map_err(Into::into)
}

fn observed_findings(artifact: &ReviewArtifact) -> Vec<BenchmarkObservedFinding> {
    artifact
        .findings
        .iter()
        .map(|finding| BenchmarkObservedFinding {
            semantic_key: finding
                .grounding
                .as_ref()
                .map(|grounding| grounding.semantic_key.clone())
                .unwrap_or_else(|| reviewgate_core::semantic_fingerprint(finding)),
            blocking: finding.is_blocking(DEFAULT_TARGET_SCORE),
        })
        .collect()
}

fn infer_language<'a>(paths: impl Iterator<Item = &'a String>) -> String {
    let mut languages = BTreeSet::new();
    for path in paths {
        let language = if path.ends_with(".rs") {
            "rust"
        } else if path.ends_with(".go") {
            "go"
        } else if path.ends_with(".py") {
            "python"
        } else if path.ends_with(".ts") || path.ends_with(".tsx") {
            "typescript"
        } else if path.ends_with(".js") || path.ends_with(".mjs") {
            "javascript"
        } else if path.ends_with(".yml") || path.ends_with(".yaml") {
            "yaml"
        } else if path.ends_with(".sh") {
            "shell"
        } else {
            continue;
        };
        languages.insert(language);
    }
    if languages.is_empty() {
        "other".to_string()
    } else {
        languages.into_iter().collect::<Vec<_>>().join("+")
    }
}

struct TempCaseRoot {
    path: PathBuf,
}

impl TempCaseRoot {
    fn new(case_id: &str) -> CliResult<Self> {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let sequence = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let safe_id = case_id
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character
                } else {
                    '-'
                }
            })
            .collect::<String>();
        let path = std::env::temp_dir().join(format!(
            "reviewgate-benchmark-{}-{sequence}-{safe_id}",
            std::process::id()
        ));
        std::fs::create_dir(&path)
            .with_context(|| format!("failed to create {}", path.display()))?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempCaseRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn parse_evidence_cases(raw: &str) -> CliResult<Vec<EvidenceGroundingCase>> {
    let value: serde_json::Value =
        serde_json::from_str(raw).context("failed to parse evidence benchmark source")?;
    let shared_provenance = value
        .get("provenance")
        .map(|provenance| {
            provenance
                .as_object()
                .cloned()
                .context("shared benchmark provenance must be an object")
        })
        .transpose()?;
    let cases = value.get("cases").cloned().unwrap_or(value);
    let mut cases: Vec<EvidenceGroundingCase> =
        serde_json::from_value(cases).context("failed to parse evidence benchmark cases")?;
    if let Some(shared) = shared_provenance {
        for case in &mut cases {
            case.provenance = Some(merge_json_objects(shared.clone(), case.provenance.take()));
        }
    }
    Ok(cases)
}

fn merge_json_objects(
    mut shared: serde_json::Map<String, serde_json::Value>,
    case: Option<serde_json::Map<String, serde_json::Value>>,
) -> serde_json::Map<String, serde_json::Value> {
    if let Some(case) = case {
        shared.extend(case);
    }
    shared
}

fn build_report(
    manifest: &BenchmarkManifest,
    cases: &[EvaluatedCase],
    mode: &'static str,
) -> CliResult<BenchmarkReport> {
    if cases.is_empty() {
        bail!("benchmark corpus contains no cases");
    }
    let mut configurations = Vec::with_capacity(manifest.configurations.len());
    for configuration in &manifest.configurations {
        let mut scores = Vec::with_capacity(cases.len());
        let mut completed = 0;
        let mut stable = 0;
        let mut converged = 0;
        let mut duplicate_finding_count = 0;
        let mut estimated_cost = 0.0;
        let mut estimated_cost_coverage = 0;
        let mut spent_cost = 0.0;
        let mut spent_cost_coverage = 0;
        let mut latency_total = 0u128;
        let mut latency_coverage = 0;
        let mut agent_time_total = 0u64;
        let mut agent_time_coverage = 0;
        let mut case_reports = Vec::with_capacity(cases.len());

        for case in cases {
            let runs = case.runs.get(&configuration.id).with_context(|| {
                format!(
                    "benchmark case `{}` has no runs for configuration `{}`",
                    case.id, configuration.id
                )
            })?;
            let canonical = runs.first().with_context(|| {
                format!(
                    "benchmark case `{}` has no repetitions for configuration `{}`",
                    case.id, configuration.id
                )
            })?;
            scores.push(canonical.score.clone());
            let review_complete = runs.iter().all(|run| run.observed_run.review_complete);
            completed += usize::from(review_complete);
            duplicate_finding_count += canonical.score.duplicate_findings.len();
            stable += usize::from(runs_are_stable(runs));
            let final_run = runs.last().expect("non-empty runs");
            let final_matches_expectation = if case.expected_blocking {
                final_run.score.true_blockers > 0 && final_run.score.false_blockers == 0
            } else {
                final_run.score.observed_blockers == 0
            };
            converged += usize::from(final_matches_expectation);
            for run in runs {
                if let Some(cost) = run.estimated_cost_usd {
                    estimated_cost += cost;
                    estimated_cost_coverage += 1;
                }
                if let Some(cost) = run.spent_cost_usd {
                    spent_cost += cost;
                    spent_cost_coverage += 1;
                }
                if let Some(latency) = run.latency_ms {
                    latency_total += u128::from(latency);
                    latency_coverage += 1;
                }
                if let Some(agent_time) = run.agent_time_ms {
                    agent_time_total = agent_time_total.saturating_add(agent_time);
                    agent_time_coverage += 1;
                }
            }
            case_reports.push(CaseReport {
                case_id: canonical.observed_run.case_id.clone(),
                name: case.name.clone(),
                provenance: case.provenance.clone(),
                expected_blocking: case.expected_blocking,
                review_complete,
                observed_blocking: canonical.score.observed_blockers > 0,
                true_blockers: canonical.score.true_blockers,
                false_blockers: canonical.score.false_blockers,
                missed_serious_defects: canonical.score.missed_serious_defects.clone(),
                contradicted_non_findings: canonical.score.contradicted_non_findings.clone(),
                unexpected_blockers: canonical.score.unexpected_blockers.clone(),
                duplicate_findings: canonical.score.duplicate_findings.clone(),
            });
        }
        case_reports.sort_by(|left, right| left.case_id.cmp(&right.case_id));
        let case_count = cases.len();
        configurations.push(ConfigurationReport {
            id: configuration.id.clone(),
            role: configuration.role,
            pipeline: configuration.pipeline,
            metrics: configuration_metrics_from_scores(&scores),
            completion_rate: ratio(completed, case_count),
            rereview_stability: ratio(stable, case_count),
            rereview_convergence: ratio(converged, case_count),
            duplicate_finding_count,
            estimated_cost_usd: (estimated_cost_coverage > 0).then_some(estimated_cost),
            spent_cost_usd: (spent_cost_coverage > 0).then_some(spent_cost),
            mean_latency_ms: (latency_coverage > 0)
                .then(|| latency_total as f64 / latency_coverage as f64),
            agent_time_ms: (agent_time_coverage > 0).then_some(agent_time_total),
            agent_time_coverage,
            cases: case_reports,
        });
    }

    let baseline = configurations
        .iter()
        .find(|configuration| configuration.role == BenchmarkConfigurationRole::Baseline)
        .context("benchmark report has no baseline configuration")?;
    let candidate = configurations
        .iter()
        .find(|configuration| configuration.role == BenchmarkConfigurationRole::Candidate)
        .context("benchmark report has no candidate configuration")?;
    let comparison = ComparisonReport {
        baseline_id: baseline.id.clone(),
        candidate_id: candidate.id.clone(),
        blocking_precision_delta: optional_delta(
            candidate.metrics.blocking_precision,
            baseline.metrics.blocking_precision,
        ),
        serious_defect_recall_delta: optional_delta(
            candidate.metrics.serious_defect_recall,
            baseline.metrics.serious_defect_recall,
        ),
        false_blockers_per_case_delta: candidate.metrics.false_blockers_per_case
            - baseline.metrics.false_blockers_per_case,
        contradiction_rate_delta: optional_delta(
            candidate.metrics.contradiction_rate,
            baseline.metrics.contradiction_rate,
        ),
        rereview_stability_delta: optional_delta(
            candidate.rereview_stability,
            baseline.rereview_stability,
        ),
        estimated_cost_usd_delta: optional_delta(
            candidate.estimated_cost_usd,
            baseline.estimated_cost_usd,
        ),
        mean_latency_ms_delta: optional_delta(candidate.mean_latency_ms, baseline.mean_latency_ms),
    };
    let threshold_results = threshold_results(manifest, baseline, candidate);
    let corpus_complete = cases.len() >= manifest.minimum_case_count;
    let passed = corpus_complete && threshold_results.iter().all(|result| result.passed);
    let languages = cases
        .iter()
        .map(|case| case.language.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let risk_classes = cases
        .iter()
        .map(|case| case.risk_class.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    Ok(BenchmarkReport {
        schema_version: BENCHMARK_REPORT_SCHEMA_VERSION,
        manifest_name: manifest.name.clone(),
        mode,
        passed,
        corpus: CorpusReport {
            case_count: cases.len(),
            minimum_case_count: manifest.minimum_case_count,
            blinded: manifest.blinded,
            repetitions: manifest.repetitions,
            languages,
            risk_classes,
            source_ids: manifest
                .sources
                .iter()
                .map(|source| source.id.clone())
                .collect(),
        },
        configurations,
        comparison,
        threshold_results,
    })
}

fn runs_are_stable(runs: &[EvaluatedRun]) -> bool {
    let Some(first) = runs.first() else {
        return false;
    };
    let mut expected = first.observed_run.findings.clone();
    expected.sort();
    runs.iter().skip(1).all(|run| {
        let mut actual = run.observed_run.findings.clone();
        actual.sort();
        actual == expected
    })
}

fn threshold_results(
    manifest: &BenchmarkManifest,
    baseline: &ConfigurationReport,
    candidate: &ConfigurationReport,
) -> Vec<ThresholdResult> {
    let thresholds = &manifest.thresholds;
    vec![
        minimum_threshold(
            "blocking_precision",
            candidate.metrics.blocking_precision,
            thresholds.minimum_blocking_precision,
        ),
        minimum_threshold(
            "serious_defect_recall",
            candidate.metrics.serious_defect_recall,
            thresholds.minimum_serious_defect_recall,
        ),
        maximum_threshold(
            "false_blockers_per_case",
            Some(candidate.metrics.false_blockers_per_case),
            thresholds.maximum_false_blockers_per_case,
        ),
        maximum_threshold(
            "contradiction_rate",
            candidate.metrics.contradiction_rate,
            thresholds.maximum_contradiction_rate,
        ),
        minimum_threshold(
            "rereview_stability",
            candidate.rereview_stability,
            thresholds.minimum_rereview_stability,
        ),
        minimum_threshold(
            "completion_rate",
            candidate.completion_rate,
            thresholds.minimum_completion_rate,
        ),
        maximum_threshold(
            "live_cost_usd",
            candidate.spent_cost_usd.or(candidate.estimated_cost_usd),
            thresholds.maximum_live_cost_usd,
        ),
        maximum_threshold(
            "mean_latency_ms",
            candidate.mean_latency_ms,
            thresholds.maximum_mean_latency_ms as f64,
        ),
        ThresholdResult {
            name: "no_blocking_precision_regression".to_string(),
            passed: match (
                candidate.metrics.blocking_precision,
                baseline.metrics.blocking_precision,
            ) {
                (Some(candidate), Some(baseline)) => candidate >= baseline,
                (Some(_), None) => true,
                _ => false,
            },
            actual: optional_delta(
                candidate.metrics.blocking_precision,
                baseline.metrics.blocking_precision,
            ),
            required: "candidate delta >= 0".to_string(),
        },
    ]
}

fn minimum_threshold(name: &str, actual: Option<f64>, minimum: f64) -> ThresholdResult {
    ThresholdResult {
        name: name.to_string(),
        passed: actual.is_some_and(|actual| actual >= minimum),
        actual,
        required: format!(">= {minimum}"),
    }
}

fn maximum_threshold(name: &str, actual: Option<f64>, maximum: f64) -> ThresholdResult {
    ThresholdResult {
        name: name.to_string(),
        passed: actual.is_some_and(|actual| actual <= maximum),
        actual,
        required: format!("<= {maximum}"),
    }
}

fn optional_delta(candidate: Option<f64>, baseline: Option<f64>) -> Option<f64> {
    candidate
        .zip(baseline)
        .map(|(candidate, baseline)| candidate - baseline)
}

fn ratio(numerator: usize, denominator: usize) -> Option<f64> {
    (denominator != 0).then(|| numerator as f64 / denominator as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reviewgate_core::{
        BENCHMARK_MANIFEST_SCHEMA_VERSION, BenchmarkConfiguration, BenchmarkSource,
        BenchmarkSourceKind, BenchmarkThresholds,
    };

    fn manifest() -> BenchmarkManifest {
        BenchmarkManifest {
            schema_version: BENCHMARK_MANIFEST_SCHEMA_VERSION.to_string(),
            name: "replacement gate".to_string(),
            blinded: true,
            minimum_case_count: 30,
            repetitions: 2,
            thresholds: BenchmarkThresholds {
                minimum_blocking_precision: 0.9,
                minimum_serious_defect_recall: 0.8,
                maximum_false_blockers_per_case: 0.1,
                maximum_contradiction_rate: 0.1,
                minimum_rereview_stability: 0.95,
                minimum_completion_rate: 1.0,
                maximum_live_cost_usd: 5.0,
                maximum_mean_latency_ms: 120_000,
            },
            configurations: vec![
                BenchmarkConfiguration {
                    id: "baseline".to_string(),
                    role: BenchmarkConfigurationRole::Baseline,
                    pipeline: BenchmarkPipeline::RawModel,
                },
                BenchmarkConfiguration {
                    id: "candidate".to_string(),
                    role: BenchmarkConfigurationRole::Candidate,
                    pipeline: BenchmarkPipeline::EvidenceGate,
                },
            ],
            sources: vec![BenchmarkSource {
                id: "grounding".to_string(),
                kind: BenchmarkSourceKind::EvidenceGrounding,
                path: "fixtures/evidence-grounding/regressions.json".to_string(),
            }],
        }
    }

    #[test]
    fn evidence_corpus_replays_twice_through_baseline_and_candidate_pipelines() {
        let cases = parse_evidence_cases(include_str!(
            "../../../fixtures/evidence-grounding/regressions.json"
        ))
        .expect("fixtures parse");
        let evaluated =
            evaluate_evidence_cases("grounding", cases, &manifest()).expect("fixtures replay");

        assert!(evaluated.len() >= 30);
        assert!(
            evaluated.iter().all(|case| {
                case.runs["baseline"].len() == 2 && case.runs["candidate"].len() == 2
            })
        );
        assert!(evaluated.iter().any(|case| {
            case.runs["baseline"][0].score.false_blockers > 0
                && case.runs["candidate"][0].score.false_blockers == 0
        }));
    }

    #[test]
    fn report_compares_baseline_and_candidate_with_stable_resource_coverage() {
        let cases = parse_evidence_cases(include_str!(
            "../../../fixtures/evidence-grounding/regressions.json"
        ))
        .expect("fixtures parse");
        let manifest = manifest();
        let evaluated =
            evaluate_evidence_cases("grounding", cases, &manifest).expect("fixtures replay");

        let report = build_report(&manifest, &evaluated, "deterministic").expect("report builds");
        let first = serde_json::to_string_pretty(&report).expect("report serializes");
        let second = serde_json::to_string_pretty(&report).expect("report serializes again");

        assert_eq!(first, second);
        assert_eq!(report.corpus.case_count, 41);
        assert_eq!(report.configurations.len(), 2);
        assert_eq!(report.comparison.baseline_id, "baseline");
        assert_eq!(report.comparison.candidate_id, "candidate");
        assert!(
            report.comparison.blocking_precision_delta > Some(0.0),
            "candidate should eliminate adjudicated false blockers"
        );
        assert!(
            report.configurations[1]
                .rereview_stability
                .is_some_and(|rate| rate == 1.0)
        );
        assert_eq!(report.configurations[1].agent_time_ms, None);
        assert_eq!(report.configurations[1].agent_time_coverage, 0);
    }

    #[test]
    fn report_uses_finding_level_precision_and_fails_incomplete_repetitions() {
        let cases = parse_evidence_cases(include_str!(
            "../../../fixtures/evidence-grounding/regressions.json"
        ))
        .expect("fixtures parse");
        let manifest = manifest();
        let mut evaluated =
            evaluate_evidence_cases("grounding", cases, &manifest).expect("fixtures replay");
        let candidate_runs = evaluated[0]
            .runs
            .get_mut("candidate")
            .expect("candidate runs");
        candidate_runs[0].score.observed_blockers = 2;
        candidate_runs[0].score.true_blockers = 1;
        candidate_runs[0].score.false_blockers = 1;
        candidate_runs[1].observed_run.review_complete = false;

        let report = build_report(&manifest, &evaluated, "live").expect("report builds");
        let candidate = &report.configurations[1];

        assert!(candidate.metrics.false_positives >= 1);
        assert!(
            candidate
                .metrics
                .blocking_precision
                .is_some_and(|value| value < 1.0)
        );
        assert!(candidate.completion_rate.is_some_and(|value| value < 1.0));
        assert!(!report.passed);
        assert!(
            report
                .threshold_results
                .iter()
                .any(|threshold| { threshold.name == "completion_rate" && !threshold.passed })
        );
    }

    #[test]
    fn live_mode_requires_an_explicit_non_blank_api_key() {
        assert!(
            require_live_api_key(None)
                .expect_err("missing key")
                .to_string()
                .contains("OPENROUTER_API_KEY")
        );
        assert!(require_live_api_key(Some("  ".to_string())).is_err());
        assert_eq!(
            require_live_api_key(Some("test-key".to_string())).expect("key accepted"),
            "test-key"
        );
    }

    #[test]
    fn live_mode_bounds_model_request_count_before_dispatch() {
        assert!(validate_live_request_count(44, 2).is_ok());
        assert!(
            validate_live_request_count(51, 2)
                .expect_err("request count must be bounded")
                .to_string()
                .contains("maximum is 100")
        );
    }

    #[test]
    fn source_provenance_is_carried_into_each_pr_53_case() {
        let cases = parse_evidence_cases(include_str!("../../../fixtures/evaluation/pr-53.json"))
            .expect("PR 53 cases parse");

        assert_eq!(cases.len(), 3);
        assert!(cases.iter().all(|case| {
            case.provenance
                .as_ref()
                .and_then(|provenance| provenance.get("head_sha"))
                == Some(&serde_json::json!(
                    "0d0ba08e4f7211f1d10141bb8cd7b362bf77d934"
                ))
        }));
        assert_eq!(
            cases[0]
                .provenance
                .as_ref()
                .and_then(|provenance| provenance.get("external_comparison"))
                .and_then(serde_json::Value::as_object)
                .and_then(|comparison| comparison.get("availability")),
            Some(&serde_json::json!("unavailable"))
        );
    }

    #[test]
    fn source_provenance_must_be_an_object() {
        let mut value: serde_json::Value =
            serde_json::from_str(include_str!("../../../fixtures/evaluation/pr-53.json"))
                .expect("PR 53 JSON");
        value["cases"][0]["provenance"] = serde_json::json!("not-an-object");

        assert!(parse_evidence_cases(&value.to_string()).is_err());
    }
}
