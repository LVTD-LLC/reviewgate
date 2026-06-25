use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use reviewgate_core::{ReviewArtifact, render_summary};

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
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::FixtureReview {
            input,
            json_out,
            summary_out,
        } => fixture_review(input, json_out, summary_out),
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

    Ok(())
}
