use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::process::Stdio;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use shipcheck_core::{
    CostComponent, CostSource, CostSummary, ModelPreset, ModelPricing, OPENROUTER_API_KEY_ENV,
    OPENROUTER_DEFAULT_BASE_URL, OPENROUTER_MODELS_PATH, ReviewArtifact, ReviewStage, ReviewStatus,
    Severity, SummaryOptions, compute_metrics, estimate_model_cost_usd, extract_summary_state,
    fallback_model_pricing, parse_openrouter_model_pricing, render_summary,
    render_summary_with_options,
};

const DEFAULT_CONTEXT_FILES: &[&str] = &[
    "AGENTS.md",
    "CLAUDE.md",
    "README.md",
    "TECH.md",
    "PRODUCT.md",
    "STRUCTURE.md",
    ".shipcheck.yml",
];

const MAX_CONTEXT_BYTES_PER_FILE: usize = 20_000;

#[derive(Debug, Parser)]
#[command(name = "shipcheck")]
#[command(about = "Open-source AI pre-merge checks for agent-written PRs")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate fixture JSON, compute score/status, and render the PR summary.
    FixtureReview {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        json_out: Option<PathBuf>,
        #[arg(long)]
        summary_out: Option<PathBuf>,
    },
    /// Review the current pull request checkout and write Shipcheck artifacts.
    ReviewPr {
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        #[arg(long, default_value = ".shipcheck.yml")]
        config: PathBuf,
        #[arg(long)]
        json_out: Option<PathBuf>,
        #[arg(long)]
        summary_out: Option<PathBuf>,
        #[arg(long)]
        target_score: Option<u8>,
        #[arg(long)]
        fail_under: Option<u8>,
        #[arg(long, value_enum, default_value = "balanced")]
        preset: PresetArg,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        openrouter_base_url: Option<String>,
        #[arg(long)]
        mock_artifact: Option<PathBuf>,
        #[arg(long)]
        report_only: bool,
        #[arg(long, value_enum, default_value = "job")]
        gate_mode: GateModeArg,
        #[arg(long)]
        summary_min_severity: Option<String>,
        #[arg(long)]
        inline_min_severity: Option<String>,
    },
    /// Render a summary from an existing artifact, optionally carrying forward hidden state.
    RenderSummary {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        previous_summary: Option<PathBuf>,
        #[arg(long)]
        summary_out: Option<PathBuf>,
        #[arg(long)]
        summary_min_severity: Option<String>,
        #[arg(long)]
        inline_min_severity: Option<String>,
    },
    /// Re-run the latest Shipcheck workflow run for a pull request branch.
    Recheck {
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        #[arg(long)]
        pr: Option<String>,
        #[arg(long, default_value = "Shipcheck")]
        workflow: String,
    },
    /// Evaluate committed review artifact fixtures without publishing anything.
    EvalFixtures {
        #[arg(long, default_value = "fixtures")]
        dir: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::FixtureReview {
            input,
            json_out,
            summary_out,
        } => fixture_review(input, json_out, summary_out),
        Command::ReviewPr {
            repo,
            config,
            json_out,
            summary_out,
            target_score,
            fail_under,
            preset,
            model,
            openrouter_base_url,
            mock_artifact,
            report_only,
            gate_mode,
            summary_min_severity,
            inline_min_severity,
        } => review_pr(ReviewPrOptions {
            repo,
            config,
            json_out,
            summary_out,
            target_score,
            fail_under,
            preset: preset.into(),
            model,
            openrouter_base_url,
            mock_artifact,
            report_only,
            gate_mode: gate_mode.into(),
            summary_min_severity,
            inline_min_severity,
        }),
        Command::RenderSummary {
            input,
            previous_summary,
            summary_out,
            summary_min_severity,
            inline_min_severity,
        } => render_summary_command(
            input,
            previous_summary,
            summary_out,
            summary_min_severity,
            inline_min_severity,
        ),
        Command::Recheck { repo, pr, workflow } => recheck(repo, pr, workflow),
        Command::EvalFixtures { dir } => eval_fixtures(dir),
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum PresetArg {
    Cheap,
    Balanced,
    Strong,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum GateModeArg {
    Job,
    Report,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GateMode {
    Job,
    Report,
}

impl From<GateModeArg> for GateMode {
    fn from(value: GateModeArg) -> Self {
        match value {
            GateModeArg::Job => GateMode::Job,
            GateModeArg::Report => GateMode::Report,
        }
    }
}

impl From<PresetArg> for ModelPreset {
    fn from(value: PresetArg) -> Self {
        match value {
            PresetArg::Cheap => ModelPreset::Cheap,
            PresetArg::Balanced => ModelPreset::Balanced,
            PresetArg::Strong => ModelPreset::Strong,
        }
    }
}

fn fixture_review(
    input: PathBuf,
    json_out: Option<PathBuf>,
    summary_out: Option<PathBuf>,
) -> Result<()> {
    let raw = fs::read_to_string(&input)
        .with_context(|| format!("failed to read fixture {}", input.display()))?;
    let artifact: ReviewArtifact = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse fixture {}", input.display()))?;
    let artifact = artifact.with_computed_score()?;
    let summary = render_summary(&artifact)?;
    let pretty_json = serde_json::to_string_pretty(&artifact)?;

    if let Some(path) = json_out {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, pretty_json)
            .with_context(|| format!("failed to write {}", path.display()))?;
    } else {
        println!("{pretty_json}");
    }

    if let Some(path) = summary_out {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, summary).with_context(|| format!("failed to write {}", path.display()))?;
    } else {
        println!("\n{summary}");
    }

    if artifact.status == ReviewStatus::Failed {
        bail!(
            "review score {} is below fail_under {}",
            artifact.score,
            artifact.fail_under
        );
    }

    Ok(())
}

#[derive(Debug)]
struct ReviewPrOptions {
    repo: PathBuf,
    config: PathBuf,
    json_out: Option<PathBuf>,
    summary_out: Option<PathBuf>,
    target_score: Option<u8>,
    fail_under: Option<u8>,
    preset: ModelPreset,
    model: Option<String>,
    openrouter_base_url: Option<String>,
    mock_artifact: Option<PathBuf>,
    report_only: bool,
    gate_mode: GateMode,
    summary_min_severity: Option<String>,
    inline_min_severity: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ReviewConfigValues {
    target_score: Option<u8>,
    fail_under: Option<u8>,
    summary_min_severity: Option<Severity>,
    inline_min_severity: Option<Severity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ContextFile {
    path: String,
    contents: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReviewContext {
    reviewed_sha: String,
    changed_files: Vec<String>,
    diff: String,
    context_files: Vec<ContextFile>,
}

fn review_pr(options: ReviewPrOptions) -> Result<()> {
    let repo = options.repo.canonicalize().unwrap_or(options.repo.clone());
    let config_path = resolve_repo_path(&repo, &options.config);
    let config_values = read_config_values(&config_path)?;
    let target_score = options
        .target_score
        .or(config_values.target_score)
        .unwrap_or(5);
    let fail_under = options.fail_under.or(config_values.fail_under).unwrap_or(4);
    let summary_min_severity = parse_optional_severity(
        options.summary_min_severity.as_deref(),
        "summary_min_severity",
    )?
    .or(config_values.summary_min_severity)
    .unwrap_or(Severity::P4);
    let inline_min_severity = parse_optional_severity(
        options.inline_min_severity.as_deref(),
        "inline_min_severity",
    )?
    .or(config_values.inline_min_severity)
    .unwrap_or(Severity::P2);
    let context = collect_review_context(&repo)?;
    let model = options
        .model
        .clone()
        .unwrap_or_else(|| options.preset.default_model().to_string());

    let artifact = if let Some(mock_artifact) = options.mock_artifact {
        read_mock_artifact(&mock_artifact)?
    } else {
        let api_key = std::env::var(OPENROUTER_API_KEY_ENV)
            .with_context(|| format!("{OPENROUTER_API_KEY_ENV} is required for live review"))?;
        let base_url = options
            .openrouter_base_url
            .clone()
            .unwrap_or_else(|| OPENROUTER_DEFAULT_BASE_URL.to_string());
        let prompt = build_review_prompt(&context, target_score, fail_under);
        let response = call_openrouter_with_curl(&base_url, &api_key, &model, &prompt)?;
        let mut artifact = parse_model_artifact(&response.content)?;
        let (model_pricing, cost_source) = if let Ok(Some(pricing)) =
            fetch_openrouter_model_pricing_with_curl(&base_url, &api_key, &model)
        {
            (Some(pricing), Some(CostSource::OpenRouterUsage))
        } else {
            (
                fallback_model_pricing(&model),
                Some(CostSource::FallbackPricing),
            )
        };
        apply_usage_cost_summary(
            &mut artifact,
            &model,
            response.usage,
            model_pricing,
            cost_source,
        );
        artifact
    };

    let mut artifact = artifact;
    artifact.reviewed_sha = context.reviewed_sha.clone();
    artifact.target_score = target_score;
    artifact.fail_under = fail_under;
    if artifact.models.is_empty() {
        artifact.models = vec![model];
    }
    artifact.review_stages = select_review_stages(&context, &artifact.models[0]);
    let mut artifact = artifact.with_computed_score()?;
    artifact.metrics = Some(compute_metrics(&artifact, inline_min_severity));
    let summary = render_summary_with_options(
        &artifact,
        SummaryOptions {
            summary_min_severity,
            inline_min_severity,
            ..SummaryOptions::default()
        },
        None,
    )?;
    let pretty_json = serde_json::to_string_pretty(&artifact)?;

    write_or_print(options.json_out, &pretty_json, "review JSON")?;
    write_or_print(options.summary_out, &summary, "review summary")?;

    if should_fail_review(
        artifact.status.clone(),
        options.report_only,
        options.gate_mode,
    ) {
        bail!(
            "review score {} is below fail_under {}",
            artifact.score,
            artifact.fail_under
        );
    }

    Ok(())
}

fn select_review_stages(context: &ReviewContext, model: &str) -> Vec<ReviewStage> {
    let mut stages = vec![ReviewStage {
        name: "general".to_string(),
        model: model.to_string(),
        status: "ran".to_string(),
        reason: "Always run a general correctness review.".to_string(),
        estimated_cost_usd: None,
    }];

    let changed = context.changed_files.join("\n").to_ascii_lowercase();
    let changed_path_matches = |predicate: fn(&str) -> bool| {
        context
            .changed_files
            .iter()
            .map(|path| path.to_ascii_lowercase())
            .any(|path| predicate(&path))
    };
    let mut add_stage = |name: &str, reason: &str| {
        stages.push(ReviewStage {
            name: name.to_string(),
            model: model.to_string(),
            status: "selected".to_string(),
            reason: reason.to_string(),
            estimated_cost_usd: None,
        });
    };
    if changed.contains("test") || changed.contains("fixture") {
        add_stage("testability", "Changed paths touch tests or fixtures.");
    }
    if changed.contains("migration") || changed.contains("schema") {
        add_stage("migrations", "Changed paths touch migrations or schemas.");
    }
    if changed.contains("security") || changed.contains("auth") || changed.contains("token") {
        add_stage(
            "security",
            "Changed paths touch security-sensitive code or docs.",
        );
    }
    if changed_path_matches(|path| {
        path.contains("readme")
            || path.starts_with("docs/")
            || path.contains("/docs/")
            || path.ends_with(".md")
    }) {
        add_stage("docs", "Changed paths include documentation.");
    }
    if changed.contains("frontend") || changed.contains(".tsx") || changed.contains(".css") {
        add_stage("frontend", "Changed paths look frontend-facing.");
    }
    if changed.contains("action.yml") || changed.contains("cargo.toml") || changed.contains("api") {
        add_stage(
            "compatibility",
            "Changed paths affect public integration surfaces.",
        );
    }

    stages
}

fn should_fail_review(status: ReviewStatus, report_only: bool, gate_mode: GateMode) -> bool {
    status == ReviewStatus::Failed && !report_only && gate_mode == GateMode::Job
}

fn render_summary_command(
    input: PathBuf,
    previous_summary: Option<PathBuf>,
    summary_out: Option<PathBuf>,
    summary_min_severity: Option<String>,
    inline_min_severity: Option<String>,
) -> Result<()> {
    let raw = fs::read_to_string(&input)
        .with_context(|| format!("failed to read artifact {}", input.display()))?;
    let artifact: ReviewArtifact = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse artifact {}", input.display()))?;
    let artifact = artifact.with_computed_score()?;
    let previous_state = if let Some(path) = previous_summary {
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read previous summary {}", path.display()))?;
        extract_summary_state(&raw)?
    } else {
        None
    };
    let summary_min_severity =
        parse_optional_severity(summary_min_severity.as_deref(), "summary_min_severity")?
            .unwrap_or(Severity::P4);
    let inline_min_severity =
        parse_optional_severity(inline_min_severity.as_deref(), "inline_min_severity")?
            .unwrap_or(Severity::P2);
    let summary = render_summary_with_options(
        &artifact,
        SummaryOptions {
            summary_min_severity,
            inline_min_severity,
            ..SummaryOptions::default()
        },
        previous_state.as_ref(),
    )?;

    write_or_print(summary_out, &summary, "review summary")?;
    Ok(())
}

fn recheck(repo: PathBuf, pr: Option<String>, workflow: String) -> Result<()> {
    let repo = repo.canonicalize().unwrap_or(repo);
    let pr_ref = pr.unwrap_or_else(|| "current branch".to_string());
    let pr_json = if pr_ref == "current branch" {
        gh(
            &repo,
            [
                "pr",
                "view",
                "--json",
                "number,headRefName,url",
                "--jq",
                "{number:.number,headRefName:.headRefName,url:.url}",
            ],
        )?
    } else {
        gh(
            &repo,
            [
                "pr",
                "view",
                &pr_ref,
                "--json",
                "number,headRefName,url",
                "--jq",
                "{number:.number,headRefName:.headRefName,url:.url}",
            ],
        )?
    };
    let pr_value: serde_json::Value =
        serde_json::from_str(&pr_json).context("failed to parse gh pr view output")?;
    let head_ref = pr_value
        .get("headRefName")
        .and_then(serde_json::Value::as_str)
        .context("gh pr view did not return headRefName")?;
    let pr_number = pr_value
        .get("number")
        .and_then(serde_json::Value::as_u64)
        .context("gh pr view did not return PR number")?;
    let pr_url = pr_value
        .get("url")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");

    let runs_json = gh(
        &repo,
        [
            "run",
            "list",
            "--workflow",
            &workflow,
            "--branch",
            head_ref,
            "--limit",
            "1",
            "--json",
            "databaseId,url,status,conclusion,headBranch",
        ],
    )?;
    let runs: Vec<serde_json::Value> =
        serde_json::from_str(&runs_json).context("failed to parse gh run list output")?;
    let Some(run) = runs.first() else {
        bail!("no {workflow:?} workflow runs found for PR #{pr_number} branch {head_ref:?}");
    };
    let run_id = run
        .get("databaseId")
        .and_then(serde_json::Value::as_u64)
        .context("workflow run did not include databaseId")?;
    let run_id = run_id.to_string();
    gh(&repo, ["run", "rerun", &run_id])?;
    let run_url = run
        .get("url")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    println!("Triggered Shipcheck recheck for PR #{pr_number} {pr_url}");
    if !run_url.is_empty() {
        println!("Rerun: {run_url}");
    }
    Ok(())
}

fn eval_fixtures(dir: PathBuf) -> Result<()> {
    let mut artifacts = Vec::new();
    for entry in fs::read_dir(&dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read fixture {}", path.display()))?;
        let artifact: ReviewArtifact = serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse fixture {}", path.display()))?;
        artifacts.push((path, artifact.with_computed_score()?));
    }

    let total = artifacts.len();
    let mut total_cost = 0.0;
    let mut finding_count = 0usize;
    let mut blocking_count = 0usize;
    let mut score_sum = 0u64;
    for (_, artifact) in &artifacts {
        score_sum += u64::from(artifact.score);
        let metrics = compute_metrics(artifact, Severity::P2);
        finding_count += metrics.finding_count as usize;
        blocking_count += metrics.blocking_finding_count as usize;
        if let Some(cost) = metrics.current_run_cost_usd {
            total_cost += cost;
        }
    }
    let average_score = if total == 0 {
        0.0
    } else {
        score_sum as f64 / total as f64
    };

    let report = serde_json::json!({
        "fixture_count": total,
        "average_score": average_score,
        "finding_count": finding_count,
        "blocking_finding_count": blocking_count,
        "estimated_cost_usd": total_cost,
        "fixtures": artifacts.iter().map(|(path, artifact)| {
            let metrics = compute_metrics(artifact, Severity::P2);
            serde_json::json!({
                "path": path.display().to_string(),
                "reviewed_sha": &artifact.reviewed_sha,
                "score": artifact.score,
                "status": artifact.status.as_str(),
                "finding_count": metrics.finding_count,
                "blocking_finding_count": metrics.blocking_finding_count,
                "estimated_cost_usd": metrics.current_run_cost_usd
            })
        })
        .collect::<Vec<_>>()
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn resolve_repo_path(repo: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo.join(path)
    }
}

fn read_config_values(path: &Path) -> Result<ReviewConfigValues> {
    if !path.exists() {
        return Ok(ReviewConfigValues::default());
    }

    let raw =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut values = ReviewConfigValues::default();
    for line in raw.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = value
            .split('#')
            .next()
            .unwrap_or(value)
            .trim()
            .trim_matches('"');
        match key {
            "target_score" => values.target_score = Some(parse_score(value, "target_score")?),
            "fail_under" => values.fail_under = Some(parse_score(value, "fail_under")?),
            "summary_min_severity" => {
                values.summary_min_severity = Some(parse_severity(value, "summary_min_severity")?)
            }
            "inline_min_severity" => {
                values.inline_min_severity = Some(parse_severity(value, "inline_min_severity")?)
            }
            _ => {}
        }
    }
    Ok(values)
}

fn parse_score(value: &str, field: &str) -> Result<u8> {
    let parsed = value
        .parse::<u8>()
        .with_context(|| format!("{field} must be an integer score, got {value:?}"))?;
    if parsed <= 5 {
        Ok(parsed)
    } else {
        bail!("{field} must be between 0 and 5, got {parsed}")
    }
}

fn parse_optional_severity(value: Option<&str>, field: &str) -> Result<Option<Severity>> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(|value| parse_severity(value, field))
        .transpose()
}

fn parse_severity(value: &str, field: &str) -> Result<Severity> {
    Severity::parse(value).with_context(|| format!("{field} must be one of P0, P1, P2, P3, P4"))
}

fn collect_review_context(repo: &Path) -> Result<ReviewContext> {
    let reviewed_sha = git(repo, ["rev-parse", "HEAD"])?;
    let base_ref = std::env::var("GITHUB_BASE_REF").ok();
    let diff_base = if let Some(base) = base_ref.as_ref() {
        Some(
            git(repo, ["merge-base", "HEAD", &format!("origin/{base}")]).with_context(|| {
                format!(
                    "failed to find merge-base for origin/{base}; configure actions/checkout with fetch-depth: 0"
                )
            })?,
        )
    } else {
        None
    };

    let diff = if let Some(base) = diff_base.as_deref() {
        git(repo, ["diff", "--unified=80", &format!("{base}...HEAD")])?
    } else {
        git(repo, ["show", "--format=", "--unified=80", "HEAD"])?
    };
    let changed_files_raw = if let Some(base) = diff_base.as_deref() {
        git(repo, ["diff", "--name-only", &format!("{base}...HEAD")])?
    } else {
        git(repo, ["show", "--format=", "--name-only", "HEAD"])?
    };
    let changed_files = changed_files_raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect();

    Ok(ReviewContext {
        reviewed_sha,
        changed_files,
        diff,
        context_files: collect_context_files(repo)?,
    })
}

fn git<const N: usize>(repo: &Path, args: [&str; N]) -> Result<String> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .with_context(|| format!("failed to run git in {}", repo.display()))?;
    if !output.status.success() {
        bail!(
            "git command failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn gh<const N: usize>(repo: &Path, args: [&str; N]) -> Result<String> {
    let output = ProcessCommand::new("gh")
        .current_dir(repo)
        .args(args)
        .output()
        .with_context(|| format!("failed to run gh in {}", repo.display()))?;
    if !output.status.success() {
        bail!(
            "gh command failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn collect_context_files(repo: &Path) -> Result<Vec<ContextFile>> {
    let mut files = Vec::new();
    for relative in DEFAULT_CONTEXT_FILES {
        let Some(path) = safe_relative_path(relative) else {
            continue;
        };
        let full_path = repo.join(&path);
        if !full_path.is_file() {
            continue;
        }
        let mut contents = fs::read_to_string(&full_path)
            .with_context(|| format!("failed to read {}", full_path.display()))?;
        truncate_context_contents(&mut contents, MAX_CONTEXT_BYTES_PER_FILE);
        files.push(ContextFile {
            path: relative.to_string(),
            contents,
        });
    }
    Ok(files)
}

fn truncate_context_contents(contents: &mut String, max_bytes: usize) {
    if contents.len() <= max_bytes {
        return;
    }

    let truncate_at = (0..=max_bytes)
        .rev()
        .find(|&index| contents.is_char_boundary(index))
        .unwrap_or(0);
    contents.truncate(truncate_at);
    contents.push_str("\n[truncated]\n");
}

fn safe_relative_path(path: &str) -> Option<PathBuf> {
    let candidate = Path::new(path);
    if candidate.is_absolute()
        || candidate
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        None
    } else {
        Some(candidate.to_path_buf())
    }
}

fn build_review_prompt(context: &ReviewContext, target_score: u8, fail_under: u8) -> String {
    let schema = include_str!("../../../schemas/shipcheck-review-output.schema.json");
    let mut prompt = String::new();
    prompt.push_str("Review this pull request. Return only JSON matching the schema below. ");
    prompt.push_str("Do not include Markdown fences or prose outside the JSON.\n\n");
    prompt.push_str(&format!(
        "reviewed_sha: {}\ntarget_score: {}\nfail_under: {}\n\n",
        context.reviewed_sha, target_score, fail_under
    ));
    prompt.push_str("JSON schema:\n");
    prompt.push_str(schema);
    prompt.push_str("\n\nChanged files:\n");
    for file in &context.changed_files {
        prompt.push_str("- ");
        prompt.push_str(file);
        prompt.push('\n');
    }
    prompt.push_str("\nContext files:\n");
    for file in &context.context_files {
        prompt.push_str(&format!("\n--- {} ---\n", file.path));
        prompt.push_str(&file.contents);
        prompt.push('\n');
    }
    prompt.push_str("\nDiff:\n```diff\n");
    prompt.push_str(&context.diff);
    prompt.push_str("\n```\n");
    prompt
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OpenRouterUsage {
    prompt_tokens: u64,
    completion_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OpenRouterCompletion {
    content: String,
    usage: Option<OpenRouterUsage>,
}

fn call_openrouter_with_curl(
    base_url: &str,
    api_key: &str,
    model: &str,
    prompt: &str,
) -> Result<OpenRouterCompletion> {
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let body_path = unique_temp_path("shipcheck-openrouter-body", "json");
    let body = serde_json::json!({
        "model": model,
        "temperature": 0,
        "response_format": { "type": "json_object" },
        "messages": [
            {
                "role": "system",
                "content": "You are Shipcheck. Return concise, high-confidence PR review findings as strict JSON."
            },
            {
                "role": "user",
                "content": prompt
            }
        ]
    });
    fs::write(&body_path, body.to_string())
        .with_context(|| format!("failed to write {}", body_path.display()))?;

    let curl_config = format!(
        "fail-with-body\nsilent\nshow-error\nrequest = \"POST\"\nurl = \"{}\"\nheader = \"Authorization: Bearer {}\"\nheader = \"Content-Type: application/json\"\ndata-binary = \"@{}\"\n",
        curl_config_quote(&url),
        curl_config_quote(api_key),
        curl_config_quote(&body_path.display().to_string()),
    );
    let mut child = ProcessCommand::new("curl")
        .arg("--config")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to execute curl for OpenRouter request")?;
    let mut stdin = child.stdin.take().context("failed to open curl stdin")?;
    stdin
        .write_all(curl_config.as_bytes())
        .context("failed to write curl config")?;
    drop(stdin);
    let output = child
        .wait_with_output()
        .context("failed to wait for curl")?;
    let _ = fs::remove_file(&body_path);

    if !output.status.success() {
        bail!(
            "OpenRouter request failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let response: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("OpenRouter response was not valid JSON")?;
    let content = response
        .pointer("/choices/0/message/content")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
        .context("OpenRouter response did not include choices[0].message.content")?;
    let usage = parse_openrouter_usage(&response);
    Ok(OpenRouterCompletion { content, usage })
}

fn fetch_openrouter_model_pricing_with_curl(
    base_url: &str,
    api_key: &str,
    model: &str,
) -> Result<Option<ModelPricing>> {
    let url = format!(
        "{}{}",
        base_url.trim_end_matches('/'),
        OPENROUTER_MODELS_PATH
    );
    let curl_config = format!(
        "fail-with-body\nsilent\nshow-error\nrequest = \"GET\"\nurl = \"{}\"\nheader = \"Authorization: Bearer {}\"\n",
        curl_config_quote(&url),
        curl_config_quote(api_key),
    );
    let mut child = ProcessCommand::new("curl")
        .arg("--config")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to execute curl for OpenRouter models request")?;
    let mut stdin = child.stdin.take().context("failed to open curl stdin")?;
    stdin
        .write_all(curl_config.as_bytes())
        .context("failed to write curl config")?;
    drop(stdin);
    let output = child
        .wait_with_output()
        .context("failed to wait for curl")?;
    if !output.status.success() {
        bail!(
            "OpenRouter models request failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let response: serde_json::Value = serde_json::from_slice(&output.stdout)
        .context("OpenRouter models response was not valid JSON")?;
    parse_openrouter_model_pricing(&response, model)
        .context("OpenRouter models response had invalid pricing")
}

fn parse_openrouter_usage(response: &serde_json::Value) -> Option<OpenRouterUsage> {
    Some(OpenRouterUsage {
        prompt_tokens: response
            .pointer("/usage/prompt_tokens")
            .and_then(serde_json::Value::as_u64)?,
        completion_tokens: response
            .pointer("/usage/completion_tokens")
            .and_then(serde_json::Value::as_u64)?,
    })
}

fn apply_usage_cost_summary(
    artifact: &mut ReviewArtifact,
    model: &str,
    usage: Option<OpenRouterUsage>,
    pricing: Option<ModelPricing>,
    source: Option<CostSource>,
) {
    if artifact.cost_summary.is_some() {
        return;
    }
    let Some(usage) = usage else {
        return;
    };
    let cost = if let Some(pricing) = pricing {
        match pricing.estimate_cost_usd(usage.prompt_tokens, usage.completion_tokens) {
            Ok(cost) => cost,
            Err(_) => return,
        }
    } else if let Ok(Some(cost)) =
        estimate_model_cost_usd(model, usage.prompt_tokens, usage.completion_tokens)
    {
        cost
    } else {
        artifact.notes.push(format!(
            "OpenRouter returned token usage for `{model}`, but Shipcheck has no pricing fallback for that model."
        ));
        return;
    };
    artifact.estimated_cost_usd = Some(cost);
    artifact.cost_summary = Some(CostSummary {
        current_run_usd: cost,
        source,
        components: vec![CostComponent {
            label: "openrouter_review".to_string(),
            model: model.to_string(),
            prompt_tokens: Some(usage.prompt_tokens),
            completion_tokens: Some(usage.completion_tokens),
            estimated_cost_usd: cost,
        }],
    });
}

fn unique_temp_path(prefix: &str, extension: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "{prefix}-{}-{}.{}",
        std::process::id(),
        monotonic_nanos(),
        extension
    ));
    path
}

fn monotonic_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

fn curl_config_quote(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(['\n', '\r'], "")
}

fn parse_model_artifact(raw: &str) -> Result<ReviewArtifact> {
    let trimmed = strip_json_fence(raw.trim());
    serde_json::from_str(trimmed)
        .or_else(|_| extract_review_artifact_json(trimmed))
        .context("model response was not a valid Shipcheck artifact")
}

fn strip_json_fence(raw: &str) -> &str {
    let Some(stripped) = raw.strip_prefix("```") else {
        return raw;
    };
    let stripped = stripped.strip_prefix("json").unwrap_or(stripped);
    stripped
        .trim()
        .strip_suffix("```")
        .unwrap_or(stripped)
        .trim()
}

fn extract_review_artifact_json(raw: &str) -> serde_json::Result<ReviewArtifact> {
    for (start, _) in raw.match_indices('{') {
        let mut depth = 0u32;
        let mut in_string = false;
        let mut escaped = false;
        for (offset, ch) in raw[start..].char_indices() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    in_string = false;
                }
                continue;
            }
            match ch {
                '"' => in_string = true,
                '{' => depth += 1,
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        let end = start + offset + ch.len_utf8();
                        let candidate = &raw[start..end];
                        if let Ok(artifact) = serde_json::from_str(candidate) {
                            return Ok(artifact);
                        }
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    serde_json::from_str(raw)
}

fn read_mock_artifact(path: &Path) -> Result<ReviewArtifact> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read mock artifact {}", path.display()))?;
    serde_json::from_str(&raw).context("mock artifact was not valid JSON")
}

fn write_or_print(path: Option<PathBuf>, contents: &str, label: &str) -> Result<()> {
    if let Some(path) = path {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, contents)
            .with_context(|| format!("failed to write {}", path.display()))?;
    } else {
        println!("{contents}");
    }
    eprintln!("wrote {label}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_review_config_values() {
        let raw = "review:\n  target_score: 5 # perfect review\n  fail_under: 4\n  summary_min_severity: P2\n  inline_min_severity: P1\n";
        let path =
            std::env::temp_dir().join(format!("shipcheck-config-test-{}.yml", std::process::id()));
        fs::write(&path, raw).expect("write temp config");

        let values = read_config_values(&path).expect("parse config");
        fs::remove_file(&path).ok();

        assert_eq!(
            values,
            ReviewConfigValues {
                target_score: Some(5),
                fail_under: Some(4),
                summary_min_severity: Some(Severity::P2),
                inline_min_severity: Some(Severity::P1),
            }
        );
    }

    #[test]
    fn parses_severity_case_insensitively() {
        assert_eq!(
            parse_severity("p3", "summary_min_severity").expect("valid severity"),
            Severity::P3
        );
        assert!(parse_severity("medium", "summary_min_severity").is_err());
    }

    #[test]
    fn strips_json_markdown_fence() {
        assert_eq!(
            strip_json_fence("```json\n{\"score\":5}\n```"),
            "{\"score\":5}"
        );
    }

    #[test]
    fn repairs_model_artifact_wrapped_in_text_with_extra_braces() {
        let raw = r#"Here is the review with prose {not json} before it:
{
  "score": 5,
  "target_score": 5,
  "fail_under": 4,
  "reviewed_sha": "abc123",
  "status": "passed",
  "verdict": "Clean.",
  "models": ["deepseek/deepseek-v4-flash"],
  "findings": [],
  "notes": []
}
Thanks {also not json}."#;

        let artifact = parse_model_artifact(raw).expect("wrapped artifact repairs");

        assert_eq!(artifact.score, 5);
        assert_eq!(artifact.verdict, "Clean.");
    }

    #[test]
    fn parses_openrouter_usage_from_response() {
        let response = serde_json::json!({
            "choices": [{"message": {"content": "{}"}}],
            "usage": {
                "prompt_tokens": 1200,
                "completion_tokens": 300
            }
        });

        let usage = parse_openrouter_usage(&response).expect("usage exists");

        assert_eq!(
            usage,
            OpenRouterUsage {
                prompt_tokens: 1200,
                completion_tokens: 300,
            }
        );
    }

    #[test]
    fn applies_usage_cost_summary_from_fallback_pricing() {
        let mut artifact = ReviewArtifact {
            score: 5,
            target_score: 5,
            fail_under: 4,
            reviewed_sha: "abc123".to_string(),
            status: ReviewStatus::Passed,
            verdict: "Clean.".to_string(),
            models: vec![],
            estimated_cost_usd: None,
            cost_summary: None,
            metrics: None,
            review_stages: vec![],
            findings: vec![],
            notes: vec![],
        };

        apply_usage_cost_summary(
            &mut artifact,
            "deepseek/deepseek-v4-flash",
            Some(OpenRouterUsage {
                prompt_tokens: 1_000_000,
                completion_tokens: 500_000,
            }),
            None,
            Some(CostSource::FallbackPricing),
        );

        let summary = artifact.cost_summary.expect("cost summary added");
        assert!((summary.current_run_usd - 0.18).abs() < f64::EPSILON);
        assert_eq!(summary.source, Some(CostSource::FallbackPricing));
    }

    #[test]
    fn gate_mode_controls_failed_review_exit_decision() {
        assert!(should_fail_review(
            ReviewStatus::Failed,
            false,
            GateMode::Job
        ));
        assert!(!should_fail_review(
            ReviewStatus::Failed,
            false,
            GateMode::Report
        ));
        assert!(!should_fail_review(
            ReviewStatus::Failed,
            true,
            GateMode::Job
        ));
        assert!(!should_fail_review(
            ReviewStatus::NeedsChanges,
            false,
            GateMode::Job
        ));
    }

    #[test]
    fn selects_docs_stage_for_root_markdown_paths() {
        let context = ReviewContext {
            reviewed_sha: "abc123".to_string(),
            changed_files: vec!["CHANGELOG.md".to_string(), "src/lib.rs".to_string()],
            diff: String::new(),
            context_files: vec![],
        };

        let stages = select_review_stages(&context, "deepseek/deepseek-v4-flash");

        assert!(stages.iter().any(|stage| stage.name == "docs"));
    }

    #[test]
    fn rejects_parent_dir_context_paths() {
        assert!(safe_relative_path("../secret").is_none());
        assert!(safe_relative_path("/tmp/secret").is_none());
        assert_eq!(
            safe_relative_path("README.md").as_deref(),
            Some(Path::new("README.md"))
        );
    }

    #[test]
    fn truncates_context_on_utf8_char_boundary() {
        let mut contents = "aaaaébbbb".to_string();

        truncate_context_contents(&mut contents, 5);

        assert_eq!(contents, "aaaa\n[truncated]\n");
    }

    #[test]
    fn prompt_contains_thresholds_schema_and_diff() {
        let context = ReviewContext {
            reviewed_sha: "abc123".to_string(),
            changed_files: vec!["src/lib.rs".to_string()],
            diff: "diff --git a/src/lib.rs b/src/lib.rs".to_string(),
            context_files: vec![ContextFile {
                path: "README.md".to_string(),
                contents: "Read me".to_string(),
            }],
        };

        let prompt = build_review_prompt(&context, 5, 4);

        assert!(prompt.contains("reviewed_sha: abc123"));
        assert!(prompt.contains("target_score: 5"));
        assert!(prompt.contains("fail_under: 4"));
        assert!(prompt.contains("Shipcheck Review Output"));
        assert!(prompt.contains("diff --git"));
    }

    #[test]
    fn curl_config_quote_escapes_quotes_and_backslashes() {
        assert_eq!(
            curl_config_quote("sk-\"secret\"\\value\n"),
            "sk-\\\"secret\\\"\\\\value"
        );
    }
}
