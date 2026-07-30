---
layout: ../../layouts/DocsLayout.astro
title: "ReviewGate documentation"
description: "Install, configure, and operate ReviewGate with task-focused documentation for maintainers and coding agents."
heading: "ReviewGate documentation"
lede: "Set up score-centered pull request reviews, consume machine-readable findings, and give coding agents a deterministic path to a fresh 5/5 result."
eyebrow: "DOCUMENTATION / START HERE"
---

## Choose the path that matches your task

ReviewGate is GitHub Actions-first, but it is not GitHub Actions-only. Use the Action for automatic pull request reviews and GitHub publishing. Use the CLI for local reviews, deterministic fixture runs, artifact inspection, structured dispositions, and agent-driven repair loops.

| I want to… | Start here | Successful outcome |
| --- | --- | --- |
| Install ReviewGate in a repository | [GitHub Actions installation](/docs/github-actions) | A pull request receives one canonical summary, inline findings, a check run, and a structured result artifact. |
| Prove the core path without secrets | [Quickstart](/docs/quickstart) | A fixture produces `.reviewgate/review.json` and `.reviewgate/summary.md`. |
| Run reviews from a terminal or coding agent | [CLI installation and commands](/docs/cli) | `reviewgate review-pr` writes local artifacts for the current checkout. |
| Change severity or review angles | [Configuration reference](/docs/configuration) | `.reviewgate.yml` is valid and ReviewGate loads the intended angles. |
| Build an automated repair loop | [Agent workflows](/docs/agent-workflows) | The agent checks exact-head JSON, fixes validated blockers, and stops only on a fresh `5/5` pass. |
| Parse ReviewGate output | [Artifacts and outputs](/docs/artifacts) | The consumer uses the versioned agent result instead of scraping Markdown. |
| Understand the score and reruns | [Features and scoring](/docs/features) | The reader can distinguish blockers, advisory findings, review errors, and rereview state. |
| Debug an installation | [Troubleshooting](/docs/troubleshooting) | The symptom maps to a bounded diagnosis and verification command. |

<h2 id="install">Install ReviewGate</h2>

For automatic pull request reviews, use the [complete GitHub Actions workflow](/docs/github-actions). It documents the OpenRouter secret, fork guard, least-privilege jobs, Action inputs and outputs, current-head rereviews, and verification path.

For local and agent-driven use, [install the Rust CLI](/docs/cli). The deterministic [quickstart](/docs/quickstart) proves scoring and artifact generation without a model key.

## Recommended reading order for AI agents

An agent configuring ReviewGate for the first time should read these pages in order:

1. [Quickstart](/docs/quickstart) for the supported execution modes and first verifiable result.
2. [GitHub Actions](/docs/github-actions) for the production workflow, secret boundary, permissions, and rereview job.
3. [Configuration](/docs/configuration) for the exact accepted fields and unsupported YAML behavior.
4. [Agent workflows](/docs/agent-workflows) for the exact-head repair algorithm and stop conditions.
5. [Artifacts and outputs](/docs/artifacts) for the stable JSON contract.
6. [Security model](/docs/security) before changing events, permissions, or trust boundaries.

The machine-readable route map at [`/llms.txt`](/llms.txt) lists every documentation page and its purpose.

## Product contract in one minute

- ReviewGate produces a visible score from `0` to `5`.
- The passing target is fixed at `5/5`; it is not configurable.
- Only validated blocking findings lower the score. Advisory `P4` findings can coexist with `5/5`.
- A completed result is `passed` or `needs_changes`.
- A reviewer or provider failure is `review_error` with `score: null`; it is not a `0/5` code-quality result.
- The GitHub Action updates one canonical `<!-- reviewgate-summary -->` comment instead of creating a new summary on every run.
- The Action also writes a versioned `reviewgate-agent-result/v1` artifact for external agents.
- Local `review-pr` creates JSON and Markdown files. It does not publish PR comments or check runs by itself.
- ReviewGate is review-only. It does not repair code, merge pull requests, run a hosted service, or store repository code outside the user's CI environment.

## Source-of-truth map

Use the most specific source when documentation and implementation appear to disagree.

| Contract | Authoritative source |
| --- | --- |
| Action inputs, outputs, runtime pin, and wrapper behavior | [`action.yml`](https://github.com/LVTD-LLC/reviewgate/blob/main/action.yml) |
| CLI commands and flags | `reviewgate --help` and `reviewgate <command> --help` |
| Review artifact fields | [`reviewgate-review-output-v3.schema.json`](https://github.com/LVTD-LLC/reviewgate/blob/main/schemas/reviewgate-review-output-v3.schema.json) |
| Stable external-agent result | [`reviewgate-agent-result-v1.schema.json`](https://github.com/LVTD-LLC/reviewgate/blob/main/schemas/reviewgate-agent-result-v1.schema.json) |
| Structured disposition payload | [`reviewgate-agent-dispositions-v1.schema.json`](https://github.com/LVTD-LLC/reviewgate/blob/main/schemas/reviewgate-agent-dispositions-v1.schema.json) |
| Scoring, validation, and summary behavior | `crates/reviewgate-core` |
| Local and CI orchestration | `crates/reviewgate-cli` |
| GitHub publishing behavior | `crates/reviewgate-github` |

<h2 id="review-loop">Run the ReviewGate repair loop</h2>

The [agent workflow](/docs/agent-workflows) retrieves the stable exact-head result, selects only `still_open` findings with a non-null `blocking_reason`, verifies each claim, applies bounded fixes, runs repository checks, and fetches a new result after pushing.

An agent stops only when the current PR head reports `status: "passed"`, `score: 5`, and no open blocker. It stops for human judgment when product intent, permissions, destructive scope, or verification remains unresolved.

## Supported operating modes

| Mode | Model key | Publishes to GitHub | Best for |
| --- | --- | --- | --- |
| GitHub Action `review` | Required | Yes | Automatic PR reviews and canonical GitHub output. |
| GitHub Action `rereview` | Not passed | Requests an eligible rerun | Exact maintainer command `@reviewgate review`. |
| CLI `review-pr` live | Required | No | Local or agent-driven model review. |
| CLI `review-pr --mock-artifact` | Not required | No | Context collection and output testing without model spend. |
| CLI `fixture-review` | Not required | No | Deterministic scoring, validation, and rendering. |
| CLI `check` | GitHub token, not model key | Reads GitHub | Exact-head agent-result retrieval. |
| CLI `review --wait` | GitHub token, not model key | Reruns/joins, waits, and reconciles threads | Bounded first-class external repair loop. |
| CLI `disposition` | GitHub token, not model key | Submits structured state | Evidence-backed finding disposition from an authenticated writer. |

## Next step

For the shortest end-to-end path, continue to the [ReviewGate quickstart](/docs/quickstart). For an existing repository that is ready for production installation, go directly to [Install ReviewGate with GitHub Actions](/docs/github-actions). For a neutral tool-selection checklist, read [Best AI code review tools](/blog/best-ai-code-review-tools).
