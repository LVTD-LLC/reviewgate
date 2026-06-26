# Review Gate

Open-source AI review gates for agent-written PRs.

Review Gate is a GitHub Actions-first, OpenRouter/BYOK PR review tool. The goal is simple: every PR gets a visible 0-5 review score, one canonical summary comment that updates in place, and machine-readable JSON that humans or external coding agents can use to decide what to fix next.

Review Gate itself is review-only. It does not autonomously repair code inside CI.

This repository is in an early build milestone. The current CLI can validate and render deterministic review artifacts from fixture JSON, and the GitHub Action can run a live pull request review from CI when `OPENROUTER_API_KEY` is configured.

## Product Contract

- Free and fully open source.
- Runs in the user's CI environment.
- Uses OpenRouter/BYOK for model calls.
- Keeps one PR summary comment updated with `<!-- review-gate-summary -->`.
- Emits a visible score like `Review Gate: 4/5`.
- Produces a JSON artifact for humans and external agent loops.
- Posts inline comments only for high-confidence, line-specific findings.
- Creates a GitHub check run based on a configurable threshold.
- Shows model cost in the review artifact and summary.

## GitHub Action

```yaml
name: Review Gate

on:
  pull_request:
    types: [opened, synchronize, reopened, ready_for_review]
  workflow_dispatch:

permissions:
  contents: read
  pull-requests: write
  issues: write
  checks: write

jobs:
  review:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5
        with:
          fetch-depth: 0
      - uses: LVTD-LLC/review-gate@v0
        with:
          openrouter_api_key: ${{ secrets.OPENROUTER_API_KEY }}
          preset: balanced
```

The action:

- collects the PR diff from the checked-out repository;
- includes bounded repository context from common instruction files like `AGENTS.md`, `README.md`, `TECH.md`, `PRODUCT.md`, and `.reviewgate.yml`;
- calls OpenRouter with the user's API key;
- validates the model response as a Review Gate JSON artifact;
- writes `.reviewgate/review.json` and `.reviewgate/summary.md`;
- appends the summary to the GitHub Actions step summary;
- creates or updates one PR comment containing `<!-- review-gate-summary -->`;
- exits non-zero when `score < fail_under` and `gate_mode: job`, unless `report_only: "true"` or `gate_mode: report` is set.

The default workflow runs on PR updates because that is the lowest-friction install path. For teams that want tighter cost control, the next control surface is an explicit recheck path such as `workflow_dispatch`, a PR comment command, or a CLI helper.

## Local Milestone

Render the fixture summary:

```bash
cargo run --locked -p reviewgate-cli -- fixture-review --input fixtures/simple-review.json
```

Write JSON and Markdown artifacts:

```bash
cargo run --locked -p reviewgate-cli -- fixture-review \
  --input fixtures/simple-review.json \
  --json-out .reviewgate/review.json \
  --summary-out .reviewgate/summary.md
```

Run the PR review command against the current checkout with a mock artifact:

```bash
cargo run --locked -p reviewgate-cli -- review-pr \
  --repo . \
  --mock-artifact fixtures/simple-review.json \
  --json-out .reviewgate/review.json \
  --summary-out .reviewgate/summary.md \
  --target-score 5 \
  --fail-under 4 \
  --report-only
```

Run the live OpenRouter path locally:

```bash
OPENROUTER_API_KEY=sk-or-... cargo run --locked -p reviewgate-cli -- review-pr \
  --repo . \
  --json-out .reviewgate/review.json \
  --summary-out .reviewgate/summary.md
```

Render a summary from an existing artifact while carrying forward hidden state from the previous canonical summary:

```bash
cargo run --locked -p reviewgate-cli -- render-summary \
  --input .reviewgate/review.json \
  --previous-summary .reviewgate/previous-summary.md \
  --summary-out .reviewgate/summary.md \
  --summary-min-severity P2
```

Re-run the latest Review Gate workflow run for the current PR branch:

```bash
cargo run --locked -p reviewgate-cli -- recheck
```

## External Agent Contract

Agents should consume the JSON artifact first and use the canonical PR summary as the human-readable fallback. The optional external loop is:

1. Read `.reviewgate/review.json` or the latest summary comment containing `<!-- review-gate-summary -->`.
2. Treat any finding with a score ceiling below `fail_under` as blocking.
3. Apply focused fixes, commit, and push.
4. Trigger or wait for Review Gate to rerun and update the same summary comment. The `reviewgate recheck` helper reruns the latest Review Gate workflow run for a PR branch when GitHub CLI auth is available.
5. Stop when `score >= target_score` and `status == "passed"`, or when human judgment is needed.

`status == "failed"` means the gate should fail CI. `status == "needs_changes"` means non-blocking work remains but the hard floor has not been crossed.

## OpenRouter Boundary

Review Gate is BYOK. The action reads the model key from `OPENROUTER_API_KEY` and must not log the key, request headers, or raw secret values. Model presets are explicit:

- `cheap`: `qwen/qwen3-coder`
- `balanced`: `deepseek/deepseek-v4-flash`
- `strong`: `anthropic/claude-sonnet-4`

Users can pin exact OpenRouter model IDs when they want stability. Preset aliases are intentionally allowed to improve over time.

The current Rust boundary builds deterministic chat-completion requests and is tested with a mock transport. Live HTTP transport is intentionally isolated from scoring and summary rendering.

The first live action implementation uses the `curl` binary available on GitHub-hosted Ubuntu runners for the OpenRouter request. Review Gate sends curl configuration through stdin and writes the non-secret request body to a temp file, so the OpenRouter key and large prompt payload are not exposed through the process argument list.

## Configuration

`.reviewgate.yml` and action inputs currently support:

- `target_score`: score required for a fully passing review.
- `fail_under`: hard floor that fails CI unless `report_only` is enabled.
- `gate_mode`: failed-review behavior. `job` fails the workflow; `report` publishes only. `check` is reserved for a future dedicated Check Run publisher.
- `summary_min_severity`: lowest severity shown in the canonical summary (`P0` through `P4`).
- `inline_min_severity`: lowest severity eligible for future inline comments (`P0` through `P4`).

`report_only: "true"` remains supported as a compatibility alias for `gate_mode: report`.

The canonical summary stores a versioned hidden state payload next to `<!-- review-gate-summary -->`. Reruns preserve reviewed SHAs, run count, and bounded cumulative cost history without relying on visible-text parsing.

## Current Limitations

- Config parsing intentionally supports the stable scalar fields above; richer nested config support comes later.
- Context collection supports common instruction files and the PR diff; full repository indexing is intentionally out of scope for v0.
- Inline comments are not posted yet. The canonical summary comment is the first publishing surface.
- Current-run and cumulative PR cost rendering are modeled in the summary. OpenRouter pricing metadata still needs a richer resolver.
- The action should not be used with `pull_request_target` for untrusted code.

## Repository Layout

```text
crates/reviewgate-core/      Review artifact types, scoring, summary rendering
crates/reviewgate-cli/       Local and CI CLI entrypoints
crates/reviewgate-github/    GitHub publishing primitives
action/                      GitHub Action wrapper
prompts/                     Built-in review stage prompts
schemas/                     JSON artifact schema
fixtures/                    Golden review fixtures
skills/reviewgate-loop/      Public agent loop skill draft
```

## Security Posture

Review Gate treats model output as untrusted text. The default workflow reviews diffs and context; it does not run arbitrary PR code and should not use `pull_request_target` for untrusted forks. GitHub token permissions should stay least-privilege.

The checked-in lockfile is generated from crates.io with `cargo generate-lockfile` and audited in CI before project build/test steps run.
