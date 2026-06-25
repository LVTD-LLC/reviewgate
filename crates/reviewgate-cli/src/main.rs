use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::process::Stdio;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use reviewgate_core::{
    ModelPreset, OPENROUTER_API_KEY_ENV, OPENROUTER_DEFAULT_BASE_URL, ReviewArtifact, ReviewStatus,
    Severity, SummaryOptions, extract_summary_state, render_summary, render_summary_with_options,
};

const DEFAULT_CONTEXT_FILES: &[&str] = &[
    "AGENTS.md",
    "CLAUDE.md",
    "README.md",
    "TECH.md",
    "PRODUCT.md",
    "STRUCTURE.md",
    ".reviewgate.yml",
];

const MAX_CONTEXT_BYTES_PER_FILE: usize = 20_000;

#[derive(Debug, Parser)]
#[command(name = "reviewgate")]
#[command(about = "Open-source AI review gates for agent-written PRs")]
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
    /// Review the current pull request checkout and write Review Gate artifacts.
    ReviewPr {
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        #[arg(long, default_value = ".reviewgate.yml")]
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
    },
    /// Re-run the latest Review Gate workflow run for a pull request branch.
    Recheck {
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        #[arg(long)]
        pr: Option<String>,
        #[arg(long, default_value = "Review Gate")]
        workflow: String,
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
            summary_min_severity,
            inline_min_severity,
        }),
        Command::RenderSummary {
            input,
            previous_summary,
            summary_out,
            summary_min_severity,
        } => render_summary_command(input, previous_summary, summary_out, summary_min_severity),
        Command::Recheck { repo, pr, workflow } => recheck(repo, pr, workflow),
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum PresetArg {
    Cheap,
    Balanced,
    Strong,
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
    let _inline_min_severity = parse_optional_severity(
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
        parse_model_artifact(&response)?
    };

    let mut artifact = artifact;
    artifact.reviewed_sha = context.reviewed_sha;
    artifact.target_score = target_score;
    artifact.fail_under = fail_under;
    if artifact.models.is_empty() {
        artifact.models = vec![model];
    }
    let artifact = artifact.with_computed_score()?;
    let summary = render_summary_with_options(
        &artifact,
        SummaryOptions {
            summary_min_severity,
            ..SummaryOptions::default()
        },
        None,
    )?;
    let pretty_json = serde_json::to_string_pretty(&artifact)?;

    write_or_print(options.json_out, &pretty_json, "review JSON")?;
    write_or_print(options.summary_out, &summary, "review summary")?;

    if artifact.status == ReviewStatus::Failed && !options.report_only {
        bail!(
            "review score {} is below fail_under {}",
            artifact.score,
            artifact.fail_under
        );
    }

    Ok(())
}

fn render_summary_command(
    input: PathBuf,
    previous_summary: Option<PathBuf>,
    summary_out: Option<PathBuf>,
    summary_min_severity: Option<String>,
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
    let summary = render_summary_with_options(
        &artifact,
        SummaryOptions {
            summary_min_severity,
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
    println!("Triggered Review Gate recheck for PR #{pr_number} {pr_url}");
    if !run_url.is_empty() {
        println!("Rerun: {run_url}");
    }
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
    let schema = include_str!("../../../schemas/review-output.schema.json");
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

fn call_openrouter_with_curl(
    base_url: &str,
    api_key: &str,
    model: &str,
    prompt: &str,
) -> Result<String> {
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let body_path = unique_temp_path("reviewgate-openrouter-body", "json");
    let body = serde_json::json!({
        "model": model,
        "temperature": 0,
        "response_format": { "type": "json_object" },
        "messages": [
            {
                "role": "system",
                "content": "You are Review Gate. Return concise, high-confidence PR review findings as strict JSON."
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
    response
        .pointer("/choices/0/message/content")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
        .context("OpenRouter response did not include choices[0].message.content")
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
    serde_json::from_str(trimmed).context("model response was not a valid Review Gate artifact")
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
            std::env::temp_dir().join(format!("reviewgate-config-test-{}.yml", std::process::id()));
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
        assert!(prompt.contains("Review Gate Review Output"));
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
