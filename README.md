# ReviewGate

ReviewGate is an open-source, GitHub Actions-first PR review gate for agent-written pull requests. It runs inside the user's CI environment, calls OpenRouter with the user's own API key, and produces a visible `0-5` score, one canonical PR summary comment, and structured JSON that humans or external coding agents can use to decide what to fix next.

ReviewGate is review-only. It does not repair code, run a hosted service, store repository code, or take over the merge decision.

Website: <https://reviewgate.lvtd.dev>

## Key Features

- Visible `0-5` confidence score on every reviewed pull request.
- Fixed passing target of `5/5`; anything below that reports `needs_changes`.
- One canonical PR summary comment marked with `<!-- reviewgate-summary -->` and updated in place on reruns.
- Exact maintainer-requested rereviews with `@reviewgate review`, bound to the PR current head.
- Structured `.reviewgate/review.json` artifact for humans, scripts, and external agent loops.
- Severity-filtered inline PR comments for findings at or above `min_severity`.
- Fallback inline anchoring for file-level, PR-level, unanchored, or stale-line findings when a right-side diff line is available.
- Configurable review angles, defaulting to general correctness and adversarial bug-finding reviews.
- Dedicated ReviewGate check run when `checks: write` is granted.
- OpenRouter BYOK model calls; no ReviewGate-hosted account, billing, telemetry, or persistent storage.
- Public agent skills for checking ReviewGate output and iterating a PR toward `5/5`.

## Table of Contents

- [Tech Stack](#tech-stack)
- [Project Status](#project-status)
- [GitHub Action Quick Start](#github-action-quick-start)
- [Prerequisites](#prerequisites)
- [Getting Started Locally](#getting-started-locally)
- [Architecture](#architecture)
- [Review Lifecycle](#review-lifecycle)
- [Maintainer-Requested Rereviews](#maintainer-requested-rereviews)
- [Scoring Model](#scoring-model)
- [JSON Artifact Contract](#json-artifact-contract)
- [Configuration](#configuration)
- [Environment Variables](#environment-variables)
- [Available Commands](#available-commands)
- [Testing](#testing)
- [Marketing Site](#marketing-site)
- [Deployment](#deployment)
- [External Agent Workflow](#external-agent-workflow)
- [Security Model](#security-model)
- [Troubleshooting](#troubleshooting)
- [Contributing](#contributing)
- [License](#license)

## Tech Stack

| Area | Technology |
| --- | --- |
| Core language | Rust 1.96.0, edition 2024 |
| Workspace | Cargo workspace with three crates |
| CLI | `clap`-based Rust binary named `reviewgate` |
| Serialization | `serde` and `serde_json` |
| Error handling | `anyhow` in CLI, `thiserror` in core |
| GitHub Action | Composite action in `action.yml` |
| Model provider | OpenRouter chat completions API, BYOK |
| HTTP transport | `curl` subprocess in the live CLI path |
| GitHub API transport | GitHub CLI (`gh`) from the Rust CLI publishing commands |
| Public schema | JSON Schema draft 2020-12 in `schemas/` |
| Marketing site | Astro 7, TypeScript 6, npm, Node 24 |
| Site deployment | Docker image from `deployment/Dockerfile`, GHCR, CapRover |
| CI | GitHub Actions |
| License | Apache-2.0 |

There is no application database, queue, cache, or hosted backend in the ReviewGate product path. ReviewGate runs in GitHub Actions, writes local files under `.reviewgate/`, publishes GitHub comments/checks, and exits.

## Project Status

This repository is in an early v0 milestone. The current implementation can:

- run a live PR review from GitHub Actions when `OPENROUTER_API_KEY` is configured;
- run deterministic fixture and mock-artifact paths locally without a model key;
- render concise canonical PR summaries;
- publish inline finding comments and a ReviewGate check run from CI;
- publish and install public agent skills;
- build and deploy the static marketing site.

Important current limitations:

- The live review path defaults to the built-in `general` and `adversarial` review angles, and `.reviewgate.yml` can override the angle list.
- Config parsing intentionally supports the documented scalar and review angle fields, not arbitrary YAML features.
- Full-repository indexing is out of scope for v0. ReviewGate uses the PR diff, changed file list, PR title/body, and bounded context from common instruction files.
- Inline comments are best-effort. If GitHub rejects an inline comment or no right-side diff anchor exists, the complete finding remains in JSON.
- Review scores below `5/5` do not fail the GitHub Actions job. Validated blockers report `needs_changes` and publish a failing ReviewGate check-run conclusion.

## GitHub Action Quick Start

1. Create an OpenRouter API key.
2. Add it to the target repository as a GitHub Actions secret named `OPENROUTER_API_KEY`.
3. Add this workflow as `.github/workflows/reviewgate.yml`.

```yaml
name: ReviewGate

on:
  pull_request:
    types: [opened, synchronize, reopened, ready_for_review]
  issue_comment:
    types: [created]

jobs:
  review:
    if: >-
      ${{
        github.event.pull_request.head.repo.full_name == github.repository &&
        github.actor != 'dependabot[bot]'
      }}
    runs-on: ubuntu-latest
    timeout-minutes: 20
    permissions:
      contents: read
      pull-requests: write
      issues: write
      checks: write
    concurrency:
      group: reviewgate-${{ github.workflow }}-${{ github.event.pull_request.number }}
      cancel-in-progress: true
    steps:
      - uses: actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5
        with:
          fetch-depth: 0
          persist-credentials: false

      # ReviewGate is early, so @v0 is the recommended moving channel.
      # Pin to an exact commit SHA if your repository policy requires immutable actions.
      - uses: LVTD-LLC/reviewgate@v0
        with:
          openrouter_api_key: ${{ secrets.OPENROUTER_API_KEY }}
          min_severity: P4

  rereview:
    if: >-
      ${{
        github.event_name == 'issue_comment' &&
        github.event.action == 'created' &&
        github.event.issue.pull_request &&
        github.event.issue.state == 'open' &&
        github.event.comment.body == '@reviewgate review' &&
        contains(fromJSON('["OWNER","MEMBER","COLLABORATOR"]'), github.event.comment.author_association)
      }}
    runs-on: ubuntu-latest
    timeout-minutes: 5
    permissions:
      actions: write
      pull-requests: write
      issues: write
    concurrency:
      group: reviewgate-rereview-${{ github.event.comment.id }}
      cancel-in-progress: false
    steps:
      - uses: LVTD-LLC/reviewgate@v0
        with:
          mode: rereview
          review_workflow: reviewgate.yml
```

Name the workflow file `reviewgate.yml`, or set `review_workflow` to its file name. The rereview job intentionally does not check out PR code and never receives `OPENROUTER_API_KEY`.

The fork-safety guard is intentional. GitHub does not expose repository secrets to untrusted fork PRs or Dependabot PR events, so the default review job skips those events instead of running ReviewGate with an empty model key. A rereview command can only rerun a completed, exact-PR ReviewGate run that already exists for the PR current head. Do not switch this workflow to `pull_request_target` for untrusted fork code.

### Required Permissions

| Permission | Why ReviewGate needs it |
| --- | --- |
| `contents: read` | Check out the repository and inspect the PR diff/context. |
| `issues: write` | Create or update the canonical PR summary comment. GitHub PR comments use the issues comments API. |
| `pull-requests: write` | Publish inline PR review comments for findings. |
| `checks: write` | Publish the dedicated ReviewGate check run. |

If `checks: write` is omitted, the review can still write JSON and summary comments, but the check-run publishing step cannot succeed. If `issues: write` is omitted, canonical summary publishing fails visibly because the summary is product-critical.

The rereview job has a separate least-privilege boundary: `actions: write` to enumerate and rerun the selected workflow run, `pull-requests: write` to verify the open PR/current head and reserve the command with a bot-owned PR comment, and `issues: write` for the acknowledgement reaction and bounded feedback. GitHub may reject PR conversation writes when the job grants only `pull-requests: read`, even though PR comments use the issues-comments API.

### Action Inputs

| Input | Required | Default | Description |
| --- | --- | --- | --- |
| `mode` | No | `review` | `review` runs the normal model-backed review; `rereview` handles the exact maintainer comment command. |
| `openrouter_api_key` | In `review` mode | None | OpenRouter API key. Pass `${{ secrets.OPENROUTER_API_KEY }}`. Never pass it to a rereview job. |
| `review_workflow` | No | `reviewgate.yml` | Workflow file name selected by rereview mode. |
| `config` | No | `.reviewgate.yml` | Path to ReviewGate config in the checked-out repository. |
| `model` | No | Built-in balanced model | Exact OpenRouter model ID. Leave empty to use the default. |
| `min_severity` | No | `P4` | Lowest severity published as inline PR comments. One of `P0`, `P1`, `P2`, `P3`, `P4`. |

The built-in default model is `deepseek/deepseek-v4-flash`. The Rust model preset mapping also defines `qwen/qwen3-coder` for cheap runs and `anthropic/claude-sonnet-4` for strong runs, but the public action currently exposes exact model override rather than a preset input.

### Runner Requirements

The documented workflow uses `ubuntu-latest`, which includes Git, Cargo/Rust tooling, `curl`, and the GitHub CLI. If you run ReviewGate on a self-hosted runner, make sure the runner has:

- Git;
- Rustup or Rust/Cargo capable of using the repository's `1.96.0` toolchain;
- `curl`;
- GitHub CLI `gh`.

## Prerequisites

For local development on a fresh machine:

- Git.
- Rustup and Cargo.
- Rust toolchain `1.96.0` with `rustfmt` and `clippy`.
- `curl` for live OpenRouter calls from the CLI.
- `jq` for inspecting generated JSON in examples.
- GitHub CLI `gh` for `recheck` and GitHub publishing commands.
- Node.js 24 and npm for the Astro marketing site.
- Docker if you want to build the production site image locally.
- `cargo-audit` for the full CI-equivalent check suite.
- An OpenRouter API key only when running live model-backed reviews.

Install common tools on macOS with Homebrew:

```bash
brew install git jq gh rustup-init node docker
rustup-init
rustup toolchain install 1.96.0 --component rustfmt --component clippy
cargo install cargo-audit --locked
```

Install common tools on Ubuntu:

```bash
sudo apt-get update
sudo apt-get install -y curl git jq build-essential
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup toolchain install 1.96.0 --component rustfmt --component clippy
cargo install cargo-audit --locked
```

For Node, use a version manager or your package manager. CI and the Docker build use Node 24:

```bash
nvm install 24
nvm use 24
```

## Getting Started Locally

### 1. Clone the Repository

```bash
git clone https://github.com/LVTD-LLC/reviewgate.git
cd reviewgate
```

### 2. Confirm the Rust Toolchain

The repository pins the toolchain in `rust-toolchain.toml`.

```bash
rustup show
rustc --version
cargo --version
```

Expected Rust version:

```text
rustc 1.96.0
```

### 3. Fetch and Build Rust Dependencies

```bash
cargo fetch --locked
cargo build --locked --workspace
```

The workspace intentionally has a small dependency surface:

- `anyhow`
- `clap`
- `serde`
- `serde_json`
- `thiserror`

### 4. Render the Fixture Review Without Any Secrets

This is the fastest way to prove the core CLI, scoring, JSON serialization, and summary rendering path works.

```bash
cargo run --locked -p reviewgate-cli -- fixture-review \
  --input fixtures/simple-review.json \
  --json-out .reviewgate/review.json \
  --summary-out .reviewgate/summary.md
```

Inspect the computed artifact:

```bash
jq '{score, status, reviewed_sha, findings: (.findings | length)}' .reviewgate/review.json
```

The fixture intentionally starts with `score: 5`, but contains a `P2` finding. The CLI recomputes the score deterministically, so the output score becomes `3/5` and status becomes `needs_changes`.

Inspect the summary:

```bash
sed -n '1,160p' .reviewgate/summary.md
```

Generated files under `.reviewgate/` are local outputs. Do not commit them unless a task explicitly asks for committed sample output.

### 5. Run the Mock PR Review Path

The mock path exercises PR context collection and artifact rendering without calling OpenRouter.

```bash
cargo run --locked -p reviewgate-cli -- review-pr \
  --repo . \
  --mock-artifact fixtures/simple-review.json \
  --json-out .reviewgate/review.json \
  --summary-out .reviewgate/summary.md
```

When `GITHUB_BASE_REF` is unset, ReviewGate uses `git show HEAD` as the local diff source. In GitHub Actions, `GITHUB_BASE_REF` is set for PRs and ReviewGate uses the merge base between `HEAD` and `origin/$GITHUB_BASE_REF`.

### 6. Run the Live Local Review Path

Only use this when you intentionally want to spend OpenRouter credits.

```bash
export OPENROUTER_API_KEY=sk-or-...

cargo run --locked -p reviewgate-cli -- review-pr \
  --repo . \
  --json-out .reviewgate/review.json \
  --summary-out .reviewgate/summary.md
```

To force the diff base locally:

```bash
git fetch origin main
GITHUB_BASE_REF=main OPENROUTER_API_KEY=sk-or-... cargo run --locked -p reviewgate-cli -- review-pr \
  --repo . \
  --json-out .reviewgate/review.json \
  --summary-out .reviewgate/summary.md
```

### 7. Install the CLI Locally

The Cargo package binary is named `reviewgate`.

```bash
cargo install --path crates/reviewgate-cli --locked
reviewgate --help
```

Inside this repository, you can always use the Cargo form instead:

```bash
cargo run --locked -p reviewgate-cli -- --help
```

### 8. Run the Site Locally

```bash
cd site
npm ci
npm run dev
```

Open <http://localhost:4321>. The site is a static Astro marketing page for `reviewgate.lvtd.dev`.

Run the site checks:

```bash
npm run check
npm run build
npm run preview
```

## Architecture

### Directory Structure

```text
.
+-- action.yml                         # Composite GitHub Action entrypoint
+-- action/                            # Action wrapper documentation
+-- crates/
|   +-- reviewgate-core/               # Artifact types, scoring, validation, OpenRouter types, summary rendering
|   +-- reviewgate-cli/                # Local and CI command-line orchestration
|   +-- reviewgate-github/             # GitHub summary, inline comment, and check-run planning primitives
+-- deployment/
|   +-- Dockerfile                     # Static site production image
|   +-- nginx.conf                     # Nginx config for the static site
+-- docs/                              # Evaluation, release, smoke-test, and external-agent docs
+-- fixtures/                          # Deterministic review artifacts for local and CI tests
+-- prompts/                           # Built-in review prompt text
+-- schemas/                           # Machine-readable JSON artifact contracts
+-- scripts/                           # Repository validation scripts
+-- site/                              # Astro marketing site
+-- skills/
|   +-- check-reviewgate/              # Public skill for inspecting ReviewGate output
|   +-- reviewgate-loop/               # Public skill for iterating a PR toward 5/5
+-- PRODUCT.md                         # Product constraints and non-goals
+-- TECH.md                            # Stack, commands, and integration boundaries
+-- STRUCTURE.md                       # File placement and ownership rules
+-- README.md                          # User-facing install, local dev, architecture, and deployment docs
```

### Workspace Crates

#### `reviewgate-core`

`crates/reviewgate-core` owns deterministic product logic:

- `ReviewArtifact`, `Finding`, `Severity`, `ReviewStatus`, metrics, cost, stage, and angle types.
- Validation for scores, confidence, costs, stages, angle results, and summary state.
- Severity-to-score math.
- Effective score computation across findings and review angles.
- Summary rendering for the canonical PR comment.
- Hidden summary state encoding and decoding.
- OpenRouter request/client boundary types and secret-redacted debug behavior.
- Fallback model pricing and OpenRouter model-pricing parsing.

Anything that affects the score, status, JSON contract, or summary shape should generally live here and have focused tests.

#### `reviewgate-cli`

`crates/reviewgate-cli` owns command parsing and side effects:

- fixture rendering;
- local/mock/live PR review orchestration;
- Git diff and changed-file collection;
- bounded context-file collection;
- PR title/body extraction from GitHub event JSON;
- OpenRouter calls through `curl`;
- model artifact parsing and JSON repair;
- multi-angle aggregation;
- config parsing;
- GitHub summary, findings, and check-run publishing commands;
- safe current-head `recheck` and comment-triggered `request-rereview` via the GitHub API;
- fixture evaluation.

The CLI depends on `reviewgate-core` for deterministic review logic and on `reviewgate-github` for GitHub publishing plans.

#### `reviewgate-github`

`crates/reviewgate-github` owns GitHub-specific planning that can be tested without network calls:

- canonical summary comment selection;
- create/update/no-op planning for `<!-- reviewgate-summary -->`;
- duplicate bot-authored summary comment cleanup planning;
- inline finding marker encoding and decoding;
- existing inline comment dedupe;
- changed-line parsing from unified diffs;
- right-side inline anchor repair and fallback allocation;
- inline finding comment body rendering;
- stale standalone finding comment cleanup detection.

Network calls to GitHub happen in the CLI through `gh`; reusable publishing logic belongs in this crate.

### Action Wrapper

`action.yml` is intentionally thin. It:

1. validates the `openrouter_api_key` input;
2. publishes a temporary `ReviewGate: running` start signal;
3. runs `reviewgate-cli review-pr`;
4. publishes inline findings best-effort;
5. publishes or updates the canonical summary comment;
6. publishes a ReviewGate check run under `always()`.

The action runs the Rust CLI from `$GITHUB_ACTION_PATH`, not from the checked-out PR workspace. That keeps product logic in Rust while letting users install ReviewGate as a normal composite GitHub Action. In `rereview` mode it only validates the comment event and requests a safe current-head rerun; it does not run model-backed review steps.

### Marketing Site

`site/` is an Astro static site. It contains:

- `src/pages/index.astro` for the landing page;
- `src/components/ReviewPanel.astro` for the example scorecard;
- `src/layouts/BaseLayout.astro` for metadata and global layout;
- `src/styles/global.css` for the visual system.

The production site is built into a static Nginx image by `deployment/Dockerfile`.

### Prompts

Prompt files live in `prompts/`:

| Prompt | Purpose |
| --- | --- |
| `general.md` | General correctness, reliability, compatibility, and maintainability review. |
| `adversarial.md` | Skeptical bug-finding pass for high-confidence defects. |
| `testability.md` | Regression coverage and brittle-test concerns. |
| `migrations.md` | Migration safety, destructive operations, backfills, and rollback gaps. |
| `security.md` | Auth, secret, injection, SSRF, path traversal, and dangerous workflow patterns. |
| `docs.md` | Documentation changes needed for public API/config/install behavior. |
| `frontend.md` | UI state, accessibility, overflow, controls, and empty/loading/error states. |
| `compatibility.md` | CLI flags, schemas, public APIs, config, artifacts, and documented behavior. |

The live action defaults to the built-in `general` and `adversarial` review angles. Repositories can replace that default list with `.reviewgate.yml` `review_angles` entries backed by inline prompts, prompt files, or local skill instructions.

## Review Lifecycle

### Live GitHub Action Flow

1. A pull request opens, updates, reopens, or is marked ready for review.
2. The workflow checks out the repository with `fetch-depth: 0`.
3. The composite action validates that `openrouter_api_key` is non-empty.
4. ReviewGate creates or updates a short running placeholder comment.
5. The CLI collects review context:
   - reviewed SHA;
   - PR title and body from `GITHUB_EVENT_PATH`;
   - changed files;
   - unified diff;
   - changed-line count;
   - bounded context files such as `AGENTS.md`, `README.md`, `TECH.md`, `PRODUCT.md`, `STRUCTURE.md`, and `.reviewgate.yml`;
   - complete current-head contents for every changed text file within explicit file-count and aggregate-byte limits, plus bounded sibling tests and referenced local reusable workflows. Reviews fail closed instead of silently dropping or truncating changed-file contents when those limits are exceeded.
6. PR title and body are passed as separate untrusted scope context. They help understand intent but are not reviewer instructions.
7. ReviewGate calls OpenRouter once for each enabled built-in review angle.
8. Each model response is parsed as strict ReviewGate JSON. If needed, the parser can strip Markdown fences or extract the first valid JSON object from prose-wrapped output.
9. Potential P0-P3 blockers pass a read-only evidence gate before they can affect scoring or publication:
   - exact path, side, one-based line, and full-line excerpt references must match the checked-out head (`new`) or a deleted diff line (`old`);
   - at least one reference must be a changed line in the reviewed diff;
   - a causal path and test assessment are required;
   - P0-P1 require a concrete reproduction or exceptional proof;
   - checked platform-contract contradictions become auditable non-blocking notes, while prompts direct self-retracting or uncertain claims away from blocking findings.
10. Angle artifacts are aggregated:
   - findings receive angle prefixes and `angle_id`;
   - per-angle scores are recorded;
   - costs are added across model calls;
   - failed angles become typed, sanitized `angle_errors` and never become numeric code scores.
11. The top-level score and status are recomputed deterministically. Any reviewer failure produces `status: "review_error"` and `score: null`.
12. ReviewGate writes:
   - `.reviewgate/review.json`;
   - `.reviewgate/summary.md`.
13. Eligible findings are published as inline PR comments when possible, including their checked claim, causal path, and evidence references.
14. The final summary replaces the running placeholder or updates the existing canonical summary comment.
15. A check run reports review availability:
   - `success` for `passed`;
   - `failure` for `needs_changes`;
   - `failure` for `review_error` or if the review artifact cannot be read.

### Summary Comment Flow

The canonical summary always contains:

```html
<!-- reviewgate-summary -->
```

It also stores hidden JSON state in an HTML comment with:

```html
<!-- reviewgate-state ... -->
```

That hidden state tracks:

- state version;
- last reviewed SHA;
- latest valid score, status, and reviewed SHA;
- bounded list of reviewed SHAs;
- run count;
- cumulative cost;
- bounded cost history.

Reruns use this state to preserve cumulative PR history. Legacy summaries created before latest-valid fields existed recover their visible score once during migration so an inconclusive rerun cannot erase it.

### Inline Finding Flow

Inline comments contain hidden markers:

```html
<!-- reviewgate-finding:... -->
```

ReviewGate uses those markers to avoid duplicate comments across reruns. Finding IDs are percent-encoded in markers so schema-valid IDs can safely round-trip.

For each finding at or above `min_severity`, ReviewGate tries to publish an inline comment:

1. If the finding points to an exact changed right-side line, use that line.
2. If the model line is stale but the file has matching changed-line text, repair the anchor.
3. If the finding is file-level or unanchored, use a fallback right-side diff line in the same file when possible.
4. If the finding is PR-level or the file has no usable anchor, use a fallback right-side diff line elsewhere in the PR when possible.
5. If no right-side anchor exists or GitHub rejects the comment, keep the finding in JSON and warn in logs. ReviewGate does not create standalone finding comments for these cases.

Older standalone finding comments with `<!-- reviewgate-finding-comment:... -->` markers are cleaned up by later runs.

## Maintainer-Requested Rereviews

ReviewGate supports one deliberately narrow command:

```text
@reviewgate review
```

The public contract is:

- The event must be `issue_comment.created` on an open pull request.
- The entire comment body must exactly equal `@reviewgate review`. Matching is case- and whitespace-sensitive; edits, aliases, and conversational text do not trigger.
- The comment author association is an early filter and must be `OWNER`, `MEMBER`, or `COLLABORATOR`. ReviewGate then verifies the actor's current repository permission through GitHub and requires effective `write`, `maintain`, or `admin` access. Actors with only `read` or `triage` access are ignored before comments are created or workflow runs are enumerated.
- ReviewGate fetches the open PR from the base repository and binds selection to its current `head.sha`.
- Run discovery follows every GitHub API page for the configured workflow file. A run is eligible only when it belongs to the exact repository and workflow, used the `pull_request` event, is completed, includes the exact PR number in `pull_requests`, and has the exact current head SHA. The newest eligible run is rerun.
- Branch names are never used as PR identity. A run for another PR with the same branch name, a stale SHA, a foreign repository, a non-PR event, or an in-progress run is never selected.
- If no eligible run exists, ReviewGate posts a bounded `no_eligible_run` response and exits without requesting a review.
- Redelivery of the same GitHub comment event is suppressed by a bot-owned marker keyed to `comment.id`. The documented concurrency group serializes duplicate deliveries before this check. A new later command has a new comment ID and intentionally requests another rereview.
- If the canonical state already records a completed review for the exact current head, a later command returns `already_reviewed_current_head` without spending model time or rerunning the workflow.
- On a new head, ReviewGate validates the prior state against the exact repository and PR, reviews the delta since the latest completed `last_valid_reviewed_sha`, and carries forward the finding dispositions described in [Rereview convergence](docs/rereview-convergence.md).
- The acknowledgement reaction and final feedback update are best-effort after the idempotency marker exists. A reaction failure never blocks an otherwise valid rerun.

The rereview job runs in the base repository context, does not check out PR code, and does not receive model secrets. It only requests a rerun of a previously approved ReviewGate `pull_request` run. `pull_request_target` is neither needed nor recommended.

`reviewgate request-rereview` emits a small JSON result with `status` and `reason`; ignored comments exit successfully, while discovery, authorization-boundary, and rerun failures exit non-zero after bounded feedback where permissions allow.

## Scoring Model

ReviewGate uses a fixed `5/5` passing target.

### Severity Score Ceilings

| Severity | Score ceiling | Meaning |
| --- | ---: | --- |
| `P0` | `1/5` | Critical issue. |
| `P1` | `2/5` | High-severity issue. |
| `P2` | `3/5` | Material issue that should block a clean review. |
| `P3` | `4/5` | Lower-severity but still score-affecting issue. |
| `P4` | `5/5` | Advisory. Does not block a `5/5` score by itself. |

The finding-derived score is the minimum score ceiling across validated blockers, or `5` when there are no blockers. Classification, severity, confidence, and evidence status are evaluated by the checked [finding policy](docs/finding-policy.md). Every successful angle score is derived from the findings it references, and the top-level score is derived from the complete finding set. A completed review with no score-affecting findings therefore cannot report `0/5`.

Before publishing findings, the canonical summary, or the GitHub check, ReviewGate validates that the completed score and status match the structured findings, each angle owns and references its complete finding set, and `reviewed_sha` equals the current PR head. A stale or contradictory artifact is replaced with a sanitized, non-retryable `artifact_validation` angle error for the current head, using the existing `malformed_response` error kind; untrusted verdict, note, and finding text from the invalid artifact is not published. The prepared artifact also replaces `.reviewgate/review.json`, so agents and GitHub surfaces consume the same safe state.

If an angle times out, returns empty or malformed output, or fails at the provider or transport boundary, the run is inconclusive. ReviewGate records a typed `angle_errors` entry, sets `status` to `review_error`, and sets `score` to `null`; it never turns reviewer failure into a code-quality zero. The canonical summary preserves and labels the latest valid score instead of replacing it with the inconclusive run.

### Status

| Score | Status |
| --- | --- |
| `5` | `passed` |
| `0` through `4` | `needs_changes` |
| no score because review was inconclusive | `review_error` |

The legacy JSON status value `failed` can still deserialize for recomputation, but current artifacts serialize `needs_changes`.

### Workflow Result

A low score is not a CLI or workflow execution failure. ReviewGate reports the low score in JSON, summary, inline comments, and the check run. The workflow exits non-zero only when ReviewGate cannot complete the review or a required publishing step fails.

## JSON Artifact Contract

The machine-readable artifact is written to:

```text
.reviewgate/review.json
```

The current public schema lives at:

```text
schemas/reviewgate-review-output-v3.schema.json
```

Version 3 adds structured finding grounding and deterministic policy fields.
ReviewGate requires grounding before a high-confidence P0-P3 defect, security,
or reliability risk can affect scoring. Version 2 added
`review_error`, a nullable score, and typed `angle_errors`. The immutable
`schemas/reviewgate-review-output.schema.json` and
`schemas/reviewgate-review-output-v2.schema.json` remain available for older
consumers. Consumers should use version 3 for artifacts produced by the release
that introduces evidence grounding and later.

Required top-level fields:

| Field | Type | Description |
| --- | --- | --- |
| `score` | integer `0..5` or null | Score derived from validated structured findings, or null for `review_error`. |
| `reviewed_sha` | string | Commit SHA reviewed by this artifact. In PR events, this is the PR head SHA. |
| `status` | `"passed"`, `"needs_changes"`, or `"review_error"` | Completed outcomes derive from the fixed `5/5` target; reviewer failures use `review_error`. |
| `verdict` | string | Concise overall verdict. Concrete defects mentioned here should also appear as findings. |
| `models` | string array | Model IDs used by the review. |
| `findings` | finding array | Structured findings. |
| `notes` | string array | Non-finding review notes. |

Optional top-level fields:

| Field | Type | Description |
| --- | --- | --- |
| `estimated_cost_usd` | number or null | Current run estimated cost. |
| `cost_summary` | object or null | Current cost plus per-component cost details. |
| `metrics` | object or null | Finding counts, severity counts, inline candidate count, analyzed line count, and cost source. |
| `review_stages` | array | Review stages that ran or were selected for reporting. |
| `angle_results` | array | Per-angle score and status derived from the angle-owned `finding_ids`, plus verdict and model. |
| `angle_errors` | array | Sanitized typed failures with angle, kind, retryability, message, and model. |

Finding fields:

| Field | Type | Description |
| --- | --- | --- |
| `id` | string | Stable machine-readable finding ID. |
| `angle_id` | string or null | Review angle that produced the finding, such as `general` or `adversarial`. |
| `scope` | `line`, `file`, or `pr` | Semantic target of the finding. It is not the publishing mode. |
| `severity` | `P0` through `P4` | Severity that determines the score ceiling. |
| `confidence` | number `0..1` | Model confidence. Blocking requires confidence of at least `0.85`; inline publishing can still include advisory findings. |
| `classification` | `defect`, `security`, `reliability_risk`, `contract_ambiguity`, or `suggestion` | Finding kind. Contract ambiguities and suggestions are advisory. |
| `evidence_gate_result` | `passed`, `failed`, or `not_required` | Deterministic evidence-gate outcome. Only `passed` can block. |
| `blocking_reason` | `validated_defect`, `validated_security`, `validated_reliability_risk`, or null | Auditable deterministic reason the finding blocks. Advisory findings use null. |
| `grounding` | object or null | Required before an eligible P0-P3 can block; contains a stable `semantic_key`, the checked claim, causal path, test assessment, exact evidence (`new` for current-head lines, `old` for deleted diff lines), related tests, P0-P1 reproduction/proof, and rereview fields. A prior still-open identity is fixed only when the delta replaces its prior checked evidence (or restores evidence previously deleted) and `resolution_disposition: fixed`, a non-empty `resolution_evidence_summary`, and exact changed current-head proof show the prior failure is gone. |
| `file` | string or null | Target file when known. |
| `line` | integer or null | Right-side changed line for line findings when known. |
| `title` | string | Short finding title. |
| `detail` | string or null | Supporting explanation. |
| `agent_instruction` | string | Actionable instruction for a human or external agent. |

Example artifact inspection:

```bash
jq -r '
  "score: \(if .score == null then "unavailable" else "\(.score)/5" end)",
  "status: \(.status)",
  "reviewed_sha: \(.reviewed_sha)",
  "findings: \(.findings | length)"
' .reviewgate/review.json
```

List score-blocking findings:

```bash
jq -r '
  .findings[]
  | select(.blocking_reason != null)
  | "- [\(.severity)] \(.id) \(.file // "PR"):\(.line // "-") \(.title)\n  \(.agent_instruction)"
' .reviewgate/review.json
```

List failing review angles:

```bash
jq -r '
  .angle_results[]?
  | select(.score < 5 or .status != "passed")
  | "- \(.name): \(.score)/5 \(.status) - \(.verdict)"
' .reviewgate/review.json
```

List retryable reviewer errors:

```bash
jq -r '
  .angle_errors[]?
  | select(.retryable)
  | "- \(.angle_name): \(.kind) - \(.message)"
' .reviewgate/review.json
```

## Configuration

ReviewGate looks for `.reviewgate.yml` by default. You can pass a different path with the action `config` input or CLI `--config`.

`.reviewgate.yml` supports `min_severity` and optional `review_angles`.

```yaml
min_severity: P2
review_angles:
  - id: general
    name: General
    prompt_file: prompts/general.md
    reason: Always run a general correctness review.
  - id: autoreview
    name: Auto Review
    skill: skills/autoreview
```

| Key | Values | Default | Effect |
| --- | --- | --- | --- |
| `min_severity` | `P0`, `P1`, `P2`, `P3`, `P4` | `P4` | Lowest severity published as inline PR comments and counted as inline-eligible in summaries. |
| `review_angles` | YAML list | Built-in `general` and `adversarial` angles | Replaces the default review angle list when present. |

Each configured review angle requires `id` plus exactly one instruction source.

| Angle field | Required | Description |
| --- | --- | --- |
| `id` | Yes | Stable angle ID. Must contain only ASCII letters, numbers, `_`, or `-`. |
| `name` | No | Human-readable name. Defaults to a humanized version of `id`. |
| `reason` | No | Reason shown in review stage metadata. Defaults based on the instruction source. |
| `prompt` | One of `prompt`, `prompt_file`, `skill` | Short inline prompt text. |
| `prompt_file` | One of `prompt`, `prompt_file`, `skill` | Repo-relative Markdown/text file containing angle instructions. `prompt_path` is accepted as an alias. |
| `skill` | One of `prompt`, `prompt_file`, `skill` | Repo-relative skill directory containing `SKILL.md`, or a direct repo-relative path to a `SKILL.md` file. `skill_path` and `skill_file` are accepted as aliases. |

Configured paths must stay inside the repository and cannot contain `..`. For skill-backed review angles, ReviewGate passes the skill instructions into the model prompt as review angle instructions. It does not execute bundled scripts, tests, tools, or PR code.

The config parser is intentionally small. It supports the documented scalar and list fields above, quoted scalar values, comments, and simple nested list entries. Block scalars such as `|` and `>` are rejected; use `prompt_file` for long prompts.

Removed keys such as `target_score`, `summary_min_severity`, `inline_min_severity`, `inline_min_confidence`, `summary_style`, `fail_under`, `report_only`, and `gate_mode` are ignored with migration warnings.

The passing target is not configurable. ReviewGate always aims for `5/5`.

## Environment Variables

### Local Review Environment

| Variable | Required | Used by | Description |
| --- | --- | --- | --- |
| `OPENROUTER_API_KEY` | Live review only | `review-pr` | OpenRouter key for live model calls. Not needed for fixture or mock paths. |
| `GH_TOKEN` | GitHub publishing/recheck/rereview | CLI via `gh` | Token used by GitHub CLI commands. Preferred in GitHub Actions. |
| `GITHUB_TOKEN` | GitHub publishing/recheck/rereview | CLI fallback | Alternate token name accepted by publishing helpers. |
| `GITHUB_BASE_REF` | Optional | diff collection | Base branch name. When present, ReviewGate diffs `merge-base HEAD origin/$GITHUB_BASE_REF...HEAD`. |
| `GITHUB_EVENT_PATH` | Optional | PR context | Path to GitHub event JSON. Used to read PR title, body, number, and head SHA. |
| `GITHUB_EVENT_NAME` | Publishing and rereview paths | publish/request commands | Publishing commands operate on `pull_request`; rereview requests require `issue_comment`. |
| `GITHUB_REPOSITORY` | Publishing paths | publish commands | Repository in `OWNER/REPO` form. |
| `GITHUB_STEP_SUMMARY` | Optional | `publish-summary` | File path where the action appends the rendered summary. |
| `GITHUB_SERVER_URL` | Optional | `publish-check-run` | Defaults to `https://github.com`. Used for check-run details URL. |
| `GITHUB_RUN_ID` | Optional | `publish-check-run` | Used to build the check-run details URL. |

### Action Wrapper Internals

The composite action maps inputs into these environment variables while invoking the Rust CLI:

| Variable | Source |
| --- | --- |
| `REVIEWGATE_CONFIG` | `inputs.config` |
| `REVIEWGATE_MODEL` | `inputs.model` |
| `REVIEWGATE_MIN_SEVERITY` | `inputs.min_severity` |
| `OPENROUTER_API_KEY` | `inputs.openrouter_api_key` |
| `GH_TOKEN` | `${{ github.token }}` for publishing steps |

Users normally set action inputs instead of these internal variables.

### Production Site Deployment Secrets

The deployment workflow uses these repository secrets:

| Secret | Purpose |
| --- | --- |
| `CAPROVER_SERVER` | CapRover server URL. |
| `APP_TOKEN` | CapRover app deploy token. |
| `GITHUB_TOKEN` | Built-in token used to push the site image to GHCR. |

There are no database, Redis, SMTP, or application server secrets.

## Available Commands

Use the Cargo form during development:

```bash
cargo run --locked -p reviewgate-cli -- <subcommand>
```

After `cargo install --path crates/reviewgate-cli --locked`, use:

```bash
reviewgate <subcommand>
```

### ReviewGate CLI

| Command | Purpose |
| --- | --- |
| `fixture-review --input <path>` | Validate fixture JSON, recompute score/status, and render JSON plus summary. |
| `review-pr --repo <path>` | Collect PR context, run mock or live review, and write artifacts. |
| `render-summary --input <path>` | Render a summary from an existing artifact. Can carry hidden state from a previous summary. |
| `recheck --repo <path> --workflow <selector>` | Safely rerun the newest completed ReviewGate run for the exact PR current head using `gh`. The selector may be an exact numeric workflow ID, workflow file name/path, or display name; non-numeric selectors must match exactly and unambiguously. |
| `request-rereview --workflow <file>` | Validate an `issue_comment` event and safely request the exact PR current-head rerun. |
| `eval-fixtures --dir <path>` | Evaluate committed artifact fixtures without publishing. |
| `publish-start-signal` | Action-internal command to create/update the running placeholder summary. |
| `publish-findings` | Action-internal command to publish eligible findings as inline PR comments. |
| `publish-summary` | Action-internal command to publish/update the canonical summary and append the step summary. |
| `publish-check-run` | Action-internal command to publish review availability as a GitHub check run. |

Common local commands:

```bash
# Render fixture JSON and summary artifacts.
cargo run --locked -p reviewgate-cli -- fixture-review \
  --input fixtures/simple-review.json \
  --json-out .reviewgate/review.json \
  --summary-out .reviewgate/summary.md

# Review the current checkout with a mock artifact.
cargo run --locked -p reviewgate-cli -- review-pr \
  --repo . \
  --mock-artifact fixtures/simple-review.json \
  --json-out .reviewgate/review.json \
  --summary-out .reviewgate/summary.md

# Run a live OpenRouter-backed review.
OPENROUTER_API_KEY=sk-or-... cargo run --locked -p reviewgate-cli -- review-pr \
  --repo . \
  --json-out .reviewgate/review.json \
  --summary-out .reviewgate/summary.md

# Render a summary while carrying forward hidden state from a previous canonical summary.
cargo run --locked -p reviewgate-cli -- render-summary \
  --input .reviewgate/review.json \
  --previous-summary .reviewgate/previous-summary.md \
  --summary-out .reviewgate/summary.md \
  --min-severity P2

# Rerun the newest eligible ReviewGate workflow for the current PR head.
cargo run --locked -p reviewgate-cli -- recheck

# Handle the current GitHub Actions issue_comment event.
GITHUB_EVENT_NAME=issue_comment cargo run --locked -p reviewgate-cli -- \
  request-rereview --workflow reviewgate.yml

# Evaluate all JSON fixtures in fixtures/.
cargo run --locked -p reviewgate-cli -- eval-fixtures --dir fixtures
```

### Repository Checks

| Command | Description |
| --- | --- |
| `bash scripts/validate-skills.sh` | Validate public agent skill frontmatter, fenced Markdown, and shell snippets. |
| `cargo fmt --all --check` | Check Rust formatting. |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | Run Rust lint checks with warnings denied. |
| `cargo test --locked --workspace` | Run all Rust tests. |
| `cargo audit` | Check `Cargo.lock` for RustSec advisories. |
| `cd site && npm ci` | Install site dependencies from `package-lock.json`. |
| `cd site && npm run check` | Run Astro type/content checks. |
| `cd site && npm run build` | Build the static site. |
| `docker build -f deployment/Dockerfile -t reviewgate-site .` | Build the production static site image. |

Full pre-PR validation:

```bash
bash scripts/validate-skills.sh
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo run --locked -p reviewgate-cli -- fixture-review \
  --input fixtures/simple-review.json \
  --json-out .reviewgate/review.json \
  --summary-out .reviewgate/summary.md
cargo audit

cd site
npm ci
npm run check
npm run build
```

Remember that the fixture command writes `.reviewgate/review.json` and `.reviewgate/summary.md`. Those generated local outputs should not be committed by default.

## Testing

### Rust Tests

Run the entire Rust suite:

```bash
cargo test --locked --workspace
```

Run one crate:

```bash
cargo test --locked -p reviewgate-core
cargo test --locked -p reviewgate-cli
cargo test --locked -p reviewgate-github
```

Run one test by name:

```bash
cargo test --locked -p reviewgate-core renders_canonical_summary_marker_and_score
```

Test placement follows repository ownership:

- core scoring, validation, cost, and summary tests live next to `crates/reviewgate-core/src/lib.rs`;
- CLI orchestration and prompt/context behavior tests live next to `crates/reviewgate-cli/src/main.rs`;
- GitHub publishing plan and inline anchor tests live next to `crates/reviewgate-github/src/lib.rs`;
- reusable deterministic examples live in `fixtures/`.

### Site Tests

```bash
cd site
npm ci
npm run check
npm run build
```

### Skill Validation

```bash
bash scripts/validate-skills.sh
```

This checks the public `skills/check-reviewgate` and `skills/reviewgate-loop` packages.

### Fixture Milestone

CI requires the artifact-writing fixture form:

```bash
cargo run --locked -p reviewgate-cli -- fixture-review \
  --input fixtures/simple-review.json \
  --json-out .reviewgate/review.json \
  --summary-out .reviewgate/summary.md
```

The stdout-only form is useful for manual inspection, but it does not verify artifact output paths:

```bash
cargo run --locked -p reviewgate-cli -- fixture-review --input fixtures/simple-review.json
```

### Live Integration Testing

Live OpenRouter and GitHub API flows should not be required by default tests. Use them deliberately:

```bash
OPENROUTER_API_KEY=sk-or-... cargo run --locked -p reviewgate-cli -- review-pr \
  --repo . \
  --json-out .reviewgate/review.json \
  --summary-out .reviewgate/summary.md
```

GitHub publishing commands need a pull request event environment, `GH_TOKEN` or `GITHUB_TOKEN`, and `gh` authentication. In ordinary local development, prefer mock planning tests and fixture rendering.

## Marketing Site

The site is static and separate from the ReviewGate action runtime.

### Local Development

```bash
cd site
npm ci
npm run dev
```

Open <http://localhost:4321>.

### Static Build

```bash
cd site
npm run check
npm run build
npm run preview
```

### Production Image

Build from the repository root:

```bash
docker build -f deployment/Dockerfile -t reviewgate-site .
docker run --rm -p 8080:80 reviewgate-site
```

Open <http://localhost:8080>.

The Dockerfile uses a Node 24 Alpine build stage, runs `npm ci` and `npm run build`, then serves `site/dist` with `nginx:stable-alpine`.

## Deployment

ReviewGate has two deployment surfaces:

1. Consumer installation of the GitHub Action.
2. Deployment of the static marketing site.

### GitHub Action Distribution

The public action metadata is `action.yml` at the repository root. Users install it with:

```yaml
- uses: LVTD-LLC/reviewgate@v0
  with:
    openrouter_api_key: ${{ secrets.OPENROUTER_API_KEY }}
```

The `v0` major tag is the recommended moving channel during early releases. Pin to an exact commit SHA when a consuming repository requires immutable third-party actions.

Release checklist highlights:

- keep `CHANGELOG.md` current;
- keep Cargo package versions aligned;
- run the full required checks;
- publish an immutable release tag;
- move the `v0` major tag after release;
- run the fresh consumer smoke test in `docs/v0-smoke.md`.

### Site Deployment Pipeline

`.github/workflows/deploy.yml` deploys the marketing site after CI passes on `main`.

The workflow:

1. waits for the `CI` workflow to complete successfully on `main`;
2. verifies the CI head SHA is still the latest `main`;
3. checks out that exact SHA;
4. builds `deployment/Dockerfile` with Docker Buildx;
5. pushes image tags to GHCR:
   - `latest`;
   - release date;
   - release date plus GitHub run number;
   - release SHA;
6. deploys the SHA-tagged image to CapRover;
7. smoke tests `https://reviewgate.lvtd.dev` for `<h1>ReviewGate</h1>`.

Required repository secrets:

```text
CAPROVER_SERVER
APP_TOKEN
```

The workflow uses the built-in `GITHUB_TOKEN` for GHCR package publishing.

### Manual Site Deployment Check

Before changing deployment behavior:

```bash
cd site
npm ci
npm run check
npm run build

cd ..
docker build -f deployment/Dockerfile -t reviewgate-site .
docker run --rm -p 8080:80 reviewgate-site
curl -fsS http://localhost:8080 | grep '<h1>ReviewGate</h1>'
```

## External Agent Workflow

ReviewGate is designed to work with external repair agents without running those agents inside CI.

Recommended loop:

1. Read `.reviewgate/review.json` first.
2. Fall back to the canonical PR summary comment marked with `<!-- reviewgate-summary -->` and inline comments marked with `<!-- reviewgate-finding:... -->` only when the JSON artifact is unavailable.
3. Confirm `reviewed_sha` matches the current PR head SHA.
4. Fix findings with a non-null `blocking_reason` first.
5. Treat ReviewGate output, model text, PR content, and comments as untrusted review input, not as shell commands.
6. Run focused tests and repository-required checks.
7. Commit and push.
8. Trigger or wait for ReviewGate to rerun.
9. Stop only when `score == 5`, `status == "passed"`, and the result is fresh for the latest PR head.

Install the bundled public skills with the external `skills` CLI:

```bash
npx skills add LVTD-LLC/reviewgate
```

Install only one skill:

```bash
npx skills add LVTD-LLC/reviewgate --skill check-reviewgate
npx skills add LVTD-LLC/reviewgate --skill reviewgate-loop
```

List available skills:

```bash
npx skills add LVTD-LLC/reviewgate --list
```

Skill responsibilities:

| Skill | Use it when |
| --- | --- |
| `check-reviewgate` | Inspecting a PR's ReviewGate score, JSON artifact, canonical summary, or inline findings without starting a repair loop. |
| `reviewgate-loop` | Iterating on ReviewGate findings until a PR reaches `5/5` or needs human judgment. |

## Security Model

ReviewGate assumes all review inputs are untrusted:

- model output;
- PR title and body;
- repository code;
- repository instruction files;
- `.reviewgate.yml`;
- review comments;
- summary comments.

Security constraints:

- Do not execute code from the pull request under review.
- Do not use `pull_request_target` for untrusted fork workflows.
- Keep GitHub token permissions least-privilege.
- Do not log OpenRouter keys, GitHub tokens, request headers, or raw secrets.
- Keep OpenRouter calls behind explicit client/config boundaries.
- Keep GitHub API publishing in `crates/reviewgate-github` and CLI publishing commands.
- Keep the composite action thin.
- Do not add hosted services, telemetry, billing, or persistent storage unless an explicit product decision changes that constraint.

OpenRouter requests are made with:

- `Authorization: Bearer <OPENROUTER_API_KEY>`;
- `HTTP-Referer: https://github.com/LVTD-LLC/reviewgate`;
- `X-OpenRouter-Title: ReviewGate`;
- `X-OpenRouter-Categories: cli-agent,cloud-agent`.

The live CLI writes the non-secret request body to a temp file and passes `curl` configuration through stdin so the large prompt payload is not exposed in process arguments. Secret debug implementations redact keys.

## Troubleshooting

### `OPENROUTER_API_KEY is required for live review`

The live `review-pr` path needs an OpenRouter key.

Use fixture or mock mode if you do not want a model call:

```bash
cargo run --locked -p reviewgate-cli -- review-pr \
  --repo . \
  --mock-artifact fixtures/simple-review.json
```

For live review:

```bash
export OPENROUTER_API_KEY=sk-or-...
```

In GitHub Actions, add `OPENROUTER_API_KEY` as a repository secret and pass it through `openrouter_api_key`.

### ReviewGate Skips Fork or Dependabot PRs

This is expected with the recommended workflow guard. GitHub does not expose repository secrets to untrusted fork or Dependabot PR events. The quick-start workflow intentionally does not use `workflow_dispatch` as a PR review fallback because ReviewGate's GitHub publishing path relies on `pull_request` event payloads.

Do not switch to `pull_request_target` for untrusted code.

### `failed to find merge-base for origin/<base>`

The checkout probably does not have enough history.

Use:

```yaml
- uses: actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5
  with:
    fetch-depth: 0
    persist-credentials: false
```

For local testing:

```bash
git fetch origin main
GITHUB_BASE_REF=main cargo run --locked -p reviewgate-cli -- review-pr --repo . --mock-artifact fixtures/simple-review.json
```

### Low Score but Green Workflow

That is intentional. ReviewGate distinguishes review results from execution failures.

- `score < 5` means `status: "needs_changes"`.
- The ReviewGate check run conclusion is `failure`.
- The workflow exits successfully if review and required publishing completed.
- Failure to publish the ReviewGate check exits non-zero instead of silently leaving an older gate result in place.
- Reviewer failures produce `status: "review_error"`, `score: null`, and a failing ReviewGate check without blaming the PR.
- Other execution or publishing failures exit non-zero.

Read `.reviewgate/review.json` or the canonical summary to decide what to fix next.

### Summary Comment Duplicates

ReviewGate only treats bot-authored comments containing `<!-- reviewgate-summary -->` as canonical summary candidates. It chooses the best existing summary by hidden state and deletes bot-authored duplicates. User-authored comments containing the marker are ignored for ownership safety.

If duplicates persist, check that:

- the workflow grants `issues: write`;
- the action runs as `github-actions[bot]`;
- the summary publish step is not hidden behind `continue-on-error`;
- an older custom workflow is not publishing its own summaries.

### No Inline Finding Comments

Check:

- `pull-requests: write` permission is present;
- `min_severity` is not filtering the findings out;
- the findings have right-side diff anchors or at least some fallback right-side diff line exists;
- GitHub did not reject the comment payload.

Even when inline publishing fails, the full findings remain in `.reviewgate/review.json`.

### `gh` Authentication Errors

Publishing and `recheck` commands use the GitHub CLI.

```bash
gh auth status
```

For CI or non-interactive shells:

```bash
export GH_TOKEN=...
```

For private repositories, the token must be able to read PRs and comments. Publishing needs write access to PR comments, issue comments, and check runs.

### `cargo audit: command not found`

Install it:

```bash
cargo install cargo-audit --locked
```

Then rerun:

```bash
cargo audit
```

### Rust Version Mismatch

Install the pinned toolchain:

```bash
rustup toolchain install 1.96.0 --component rustfmt --component clippy
rustup override set 1.96.0
```

The repository also contains `rust-toolchain.toml`, so `cargo` should automatically select the pinned toolchain when Rustup is active.

### Site Build Fails

Use Node 24 and reinstall from the lockfile:

```bash
cd site
rm -rf node_modules
npm ci
npm run check
npm run build
```

### Config Values Appear Ignored

ReviewGate supports the documented `min_severity` scalar and `review_angles` list. It does not support arbitrary YAML features or removed config keys. These removed keys are intentionally ignored:

- `target_score`
- `summary_min_severity`
- `inline_min_severity`
- `inline_min_confidence`
- `summary_style`
- `fail_under`
- `report_only`
- `gate_mode`
- `publish_inline_comments`

Use:

```yaml
min_severity: P2
review_angles:
  - id: general
    prompt_file: prompts/general.md
  - id: adversarial
    prompt_file: prompts/adversarial.md
```

### Generated `.reviewgate/` Files Show Up in Git

They are local artifacts. Inspect them, but do not commit them by default.

```bash
git status --short .reviewgate
```

If you generated them only for local testing, remove your local outputs before preparing a commit.

## Contributing

Read these steering files before changing code:

- `PRODUCT.md`
- `TECH.md`
- `STRUCTURE.md`
- `AGENTS.md`

Contribution expectations:

- Do not commit directly to `main`.
- Keep changes small and reviewable.
- Update `CHANGELOG.md` for user-visible or repo-process changes.
- Keep score and summary behavior deterministic and well tested.
- Add focused tests when changing scoring, summary rendering, schema compatibility, GitHub publishing, action behavior, or agent-facing contracts.
- Keep the action wrapper thin; product logic belongs in Rust crates.
- Avoid network calls in default tests.
- Do not commit generated `.reviewgate/` outputs unless explicitly requested.
- Do not introduce hosted services, telemetry, billing, or persistent storage without an approved product decision.

For code placement:

- deterministic scoring, validation, and rendering go in `crates/reviewgate-core`;
- CLI orchestration and file IO go in `crates/reviewgate-cli`;
- GitHub API planning goes in `crates/reviewgate-github`;
- website code goes in `site/`;
- deployment support goes in `deployment/`;
- prompts go in `prompts/`;
- JSON contracts go in `schemas/`;
- fixtures go in `fixtures/`;
- public agent skills go in `skills/`.

## License

ReviewGate is licensed under the Apache License, Version 2.0. See `LICENSE`.
