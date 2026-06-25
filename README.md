# Review Gate

Open-source AI review gates for agent-written PRs.

Review Gate is a GitHub Actions-first, OpenRouter/BYOK PR review tool. The goal is simple: every PR gets a visible 0-5 review score, one canonical summary comment that updates in place, and machine-readable JSON so coding agents can iterate until the score reaches the target.

This repository is in the first build milestone. The current CLI can validate and render deterministic review artifacts from fixture JSON. OpenRouter review stages and GitHub publishing are next.

## Product Contract

- Free and fully open source.
- Runs in the user's CI environment.
- Uses OpenRouter/BYOK for model calls.
- Keeps one PR summary comment updated with `<!-- review-gate-summary -->`.
- Emits a visible score like `Review Gate: 4/5`.
- Produces a JSON artifact for agent loops.
- Posts inline comments only for high-confidence, line-specific findings.
- Creates a GitHub check run based on a configurable threshold.

## Planned GitHub Action

```yaml
name: Review Gate

on:
  pull_request:
    types: [opened, synchronize, reopened, ready_for_review]
  workflow_dispatch:

permissions:
  contents: read
  pull-requests: write
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
```

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
