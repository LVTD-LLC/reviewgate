use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use reviewgate_core::{
    ModelPreset, OPENROUTER_API_KEY_ENV, OPENROUTER_DEFAULT_BASE_URL, ReviewArtifact, ReviewStatus,
    render_summary,
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
        }),
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
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ReviewConfigValues {
    target_score: Option<u8>,
    fail_under: Option<u8>,
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
    let summary = render_summary(&artifact)?;
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
        let value = value.trim().trim_matches('"');
        match key {
            "target_score" => values.target_score = Some(parse_score(value, "target_score")?),
            "fail_under" => values.fail_under = Some(parse_score(value, "fail_under")?),
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

fn collect_review_context(repo: &Path) -> Result<ReviewContext> {
    let reviewed_sha = git(repo, ["rev-parse", "HEAD"])?;
    let base_ref = std::env::var("GITHUB_BASE_REF").ok();
    let diff_base = base_ref
        .as_ref()
        .and_then(|base| git(repo, ["merge-base", "HEAD", &format!("origin/{base}")]).ok());

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
        if contents.len() > MAX_CONTEXT_BYTES_PER_FILE {
            contents.truncate(MAX_CONTEXT_BYTES_PER_FILE);
            contents.push_str("\n[truncated]\n");
        }
        files.push(ContextFile {
            path: relative.to_string(),
            contents,
        });
    }
    Ok(files)
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
    let output = ProcessCommand::new("curl")
        .arg("--fail-with-body")
        .arg("--silent")
        .arg("--show-error")
        .arg("--request")
        .arg("POST")
        .arg("--header")
        .arg(format!("Authorization: Bearer {api_key}"))
        .arg("--header")
        .arg("Content-Type: application/json")
        .arg("--data")
        .arg(body.to_string())
        .arg(url)
        .output()
        .context("failed to execute curl for OpenRouter request")?;
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
        let raw = "review:\n  target_score: 5\n  fail_under: 4\n";
        let path =
            std::env::temp_dir().join(format!("reviewgate-config-test-{}.yml", std::process::id()));
        fs::write(&path, raw).expect("write temp config");

        let values = read_config_values(&path).expect("parse config");
        fs::remove_file(&path).ok();

        assert_eq!(
            values,
            ReviewConfigValues {
                target_score: Some(5),
                fail_under: Some(4)
            }
        );
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
}
