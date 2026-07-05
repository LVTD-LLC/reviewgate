# ReviewGate

Open-source AI pre-merge checks for agent-written PRs.

ReviewGate is a GitHub Actions-first, OpenRouter/BYOK PR review tool. The goal is simple: every PR gets a visible 0-5 review score, one canonical summary comment that updates in place, and machine-readable JSON that humans or external coding agents can use to decide what to fix next.

ReviewGate itself is review-only. It does not autonomously repair code inside CI.

Website: <https://reviewgate.lvtd.dev>

This repository is in an early build milestone. The current CLI can validate and render deterministic review artifacts from fixture JSON, and the GitHub Action can run a live pull request review from CI when `OPENROUTER_API_KEY` is configured.

## Product Contract

- Free and fully open source.
- Runs in the user's CI environment.
- Uses OpenRouter/BYOK for model calls.
- Keeps one concise PR summary comment updated with `<!-- reviewgate-summary -->`.
- Emits a visible score like `Confidence Score: 4/5`.
- Runs both a general correctness review and an adversarial bug-finding review in the live action path; the visible score is the lowest effective score across enabled review angles.
- Produces a JSON artifact for humans and external agent loops.
- Posts findings at or above `min_severity` as inline PR comments, anchoring file/PR-level or unanchored findings to fallback right-side diff lines when needed and deduping by stable ReviewGate finding markers.
- Reports whether the review reached a clean `5/5` without failing CI for low scores.
- Publishes a dedicated GitHub check run for review availability when `checks: write` is granted; `needs_changes` reviews conclude `neutral` rather than green `success`.
- Shows cumulative review count, changed lines analyzed, model cost, and latest analyzed commit in the PR summary footer, with full cost/metrics data in the JSON artifact.

## GitHub Action

```yaml
name: ReviewGate

on:
  pull_request:
    types: [opened, synchronize, reopened, ready_for_review]
  workflow_dispatch:

permissions:
  contents: read
  pull-requests: write
  issues: write
  checks: write

concurrency:
  group: reviewgate-${{ github.workflow }}-${{ github.event.pull_request.number || github.run_id }}
  cancel-in-progress: true

jobs:
  review:
    if: >-
      ${{
        github.event_name == 'workflow_dispatch' ||
        (
          github.event.pull_request.head.repo.full_name == github.repository &&
          github.actor != 'dependabot[bot]'
        )
      }}
    runs-on: ubuntu-latest
    timeout-minutes: 20
    steps:
      - uses: actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5
        with:
          fetch-depth: 0
      # ReviewGate is early, so @v0 is the recommended moving channel.
      # Agents should not rewrite this to a latest commit SHA unless you want frozen updates.
      - uses: LVTD-LLC/reviewgate@v0
        with:
          openrouter_api_key: ${{ secrets.OPENROUTER_API_KEY }}
```

The job-level `if` keeps the default install fork-safe: GitHub does not expose repository secrets to forked PRs or Dependabot PR events, so ReviewGate skips those events instead of passing an empty model key into a required review check. Do not move this workflow to `pull_request_target` for untrusted fork code.

The `LVTD-LLC/reviewgate@v0` action reference is the documented default install path so repositories can receive compatible v0 updates. Pin it to an exact commit SHA instead if your repository policy requires immutable third-party action references.

The permissions block is intentionally the full-featured least-privilege set for ReviewGate: `issues: write` updates the canonical PR summary comment, `pull-requests: write` publishes inline review comments, and `checks: write` publishes the ReviewGate check run.

The action:

- posts or updates a short `ReviewGate: running` placeholder comment when PR review starts;
- collects the PR title and description from the GitHub event plus the PR diff from the checked-out repository;
- includes bounded repository context from common instruction files like `AGENTS.md`, `README.md`, `TECH.md`, `PRODUCT.md`, and `.reviewgate.yml`;
- calls OpenRouter with the user's API key;
- runs separate general and adversarial review prompts and aggregates them into one ReviewGate artifact;
- validates the model response as a ReviewGate JSON artifact;
- writes `.reviewgate/review.json` and `.reviewgate/summary.md`;
- appends the summary to the GitHub Actions step summary;
- replaces the running placeholder with one concise PR comment containing `<!-- reviewgate-summary -->`;
- posts findings at or above `min_severity` as inline PR review comments when possible, using fallback right-side diff line anchors for file-level, PR-level, or stale-line findings;
- publishes a check-run status for review availability when permissions allow;
- exits non-zero only when ReviewGate cannot complete the review or a required publishing step fails.

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
  --summary-out .reviewgate/summary.md
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
  --min-severity P2
```

Re-run the latest ReviewGate workflow run for the current PR branch:

```bash
cargo run --locked -p reviewgate-cli -- recheck
```

Evaluate committed artifacts without publishing:

```bash
cargo run --locked -p reviewgate-cli -- eval-fixtures --dir fixtures
```

## External Agent Contract

Agents should consume the JSON artifact first and use ReviewGate PR comments as the human-readable fallback. The optional external loop is:

1. Read `.reviewgate/review.json` or the latest summary comment containing `<!-- reviewgate-summary -->`.
2. Treat any finding with a score ceiling below `5` as score-affecting.
3. Apply focused fixes, commit, and push.
4. Trigger or wait for ReviewGate to rerun and update the same summary comment. The `reviewgate recheck` helper reruns the latest ReviewGate workflow run for a PR branch when GitHub CLI auth is available.
5. Stop when `score == 5` and `status == "passed"`, or when human judgment is needed.

`status == "needs_changes"` means the review completed but the score is below `5/5`. The action does not fail CI for this status, and the ReviewGate check run uses a neutral conclusion.

Finding `scope` describes the finding's target, not whether it can be published inline. `scope: "line"` findings should include a file and changed line when the issue belongs to one exact diff line; ReviewGate repairs stale line anchors to changed lines when possible. `scope: "file"` and `scope: "pr"` findings stay broad in the JSON artifact but are still published as inline PR comments by anchoring to a fallback right-side diff line when needed.

## Agent Skills

ReviewGate includes two public agent skills:

- `skills/check-reviewgate/`: inspect a PR's ReviewGate score, status, JSON artifact, canonical summary, and inline findings without starting a repair loop.
- `skills/reviewgate-loop/`: iterate on ReviewGate findings until the PR reaches `5/5`, or stop when human judgment is needed.

Install both skills with the external `skills` CLI. `npx` downloads and runs the CLI package without requiring a global install:

```bash
npx skills add LVTD-LLC/reviewgate
```

Use `--skill check-reviewgate` or `--skill reviewgate-loop` to install only one skill, `--global` for a user-level install, or `--agent <agent-name>` when installing for a specific supported agent. To inspect the available skills before installing, run:

```bash
npx skills add LVTD-LLC/reviewgate --list
```

## OpenRouter Boundary

ReviewGate is BYOK. The action reads the model key from `OPENROUTER_API_KEY` and must not log the key, request headers, or raw secret values. The default model is `deepseek/deepseek-v4-flash`; users can pin an exact OpenRouter model ID with the `model` action input when they want stability.

The current Rust boundary builds deterministic chat-completion requests and is tested with a mock transport. Live HTTP transport is intentionally isolated from scoring and summary rendering.

The first live action implementation uses the `curl` binary available on GitHub-hosted Ubuntu runners for the OpenRouter request. ReviewGate sends curl configuration through stdin and writes the non-secret request body to a temp file, so the OpenRouter key and large prompt payload are not exposed through the process argument list.

ReviewGate sends stable OpenRouter attribution headers on chat and model-pricing requests: `HTTP-Referer: https://github.com/LVTD-LLC/reviewgate`, `X-OpenRouter-Title: ReviewGate`, and `X-OpenRouter-Categories: cli-agent,cloud-agent`. These identify ReviewGate traffic without exposing user secrets.

## Configuration

`.reviewgate.yml` currently supports one scalar key:

- `min_severity`: lowest severity published as ReviewGate PR comments (`P0` through `P4`). Defaults to `P4`, the least restrictive supported severity.

Action inputs currently support `openrouter_api_key`, `config`, `model`, and `min_severity`.

Review scores below `5` produce `status: "needs_changes"` in the JSON artifact and summary. They produce a neutral ReviewGate check-run conclusion, but they do not fail the GitHub Actions job.

The canonical summary stores a versioned hidden state payload next to `<!-- reviewgate-summary -->`. Reruns preserve reviewed SHAs, run count, and bounded cumulative cost history without relying on visible-text parsing. The visible summary is intentionally short: title, verdict, left-aligned confidence score, per-angle score table when multiple review angles run, compact finding counts, collapsed Important Files Changed and Flowchart sections, and a tiny footer with review count, changed lines analyzed when known, total cost, and latest analyzed commit. Finding detail lives in inline ReviewGate PR comments and the JSON artifact.

## Current Limitations

- Config parsing intentionally supports only the stable scalar field above; richer nested config support comes later.
- Review angle selection is not configurable yet. The live action path currently runs the built-in `general` and `adversarial` angles.
- Context collection supports the PR title, PR description, common instruction files, and the PR diff; full repository indexing is intentionally out of scope for v0.
- Inline comments are best-effort: stale model-provided line anchors are repaired to matching changed lines when possible, and file/PR-level or unanchored findings are anchored to fallback right-side diff lines. If no right-side diff anchor exists or GitHub rejects an inline comment, the full finding remains in JSON; ReviewGate does not create standalone finding comments.
- Current-run and cumulative PR cost rendering are modeled in the concise summary. OpenRouter pricing metadata still needs a richer resolver.
- The action should not be used with `pull_request_target` for untrusted code.

## Repository Layout

```text
crates/reviewgate-core/      Review artifact types, scoring, summary rendering
crates/reviewgate-cli/       Local and CI CLI entrypoints
crates/reviewgate-github/    GitHub publishing primitives
action/                      GitHub Action wrapper
prompts/                     Built-in review stage prompts
docs/evaluation.md           Offline fixture evaluation workflow
docs/external-agent-workflow.md Optional repair-agent workflow
docs/release-v0.1.0.md       Release readiness checklist
docs/v0-smoke.md             Fresh consumer workflow smoke test for moved v0 tags
schemas/                     JSON artifact schema
fixtures/                    Golden review fixtures
skills/check-reviewgate/     Public agent PR inspection skill
skills/reviewgate-loop/      Public agent loop skill
```

## Security Posture

ReviewGate treats model output as untrusted text. The default workflow reviews diffs and context; it does not run arbitrary PR code and should not use `pull_request_target` for untrusted forks. GitHub token permissions should stay least-privilege.

The checked-in lockfile is generated from crates.io with `cargo generate-lockfile` and audited in CI before project build/test steps run.
