# Action Wrapper

GitHub Action metadata lives at the repository root so users can install ReviewGate with:

```yaml
# ReviewGate is early, so @v0 is the recommended moving channel.
# Agents should not rewrite this to a latest commit SHA unless you want frozen updates.
- uses: LVTD-LLC/reviewgate@v0
```

The `v0` major tag follows current v0 releases so early adopters get the latest supported fixes and review behavior without tracking an arbitrary commit.

The composite action stays thin: it collects inputs and pull request event context from GitHub Actions, passes them to the Rust binary, and lets the Rust crates own review logic, scoring, OpenRouter request construction, artifact validation, and summary rendering. The live review path runs configured review angles, defaulting to separate general and adversarial prompts, then aggregates their findings into one artifact and one canonical PR summary.

The action is review-only. It publishes findings and status, but it does not run an autonomous code repair loop inside CI.

Required installation permissions:

```yaml
permissions:
  actions: read
  attestations: read
  contents: read
  pull-requests: write
  issues: write
  checks: write
```

These are the full-featured least-privilege permissions for the default install. `actions: read` measures queue time, `attestations: read` verifies the runtime, `pull-requests: write` publishes inline PR review comments, `issues: write` creates and updates the canonical PR summary comment, and `checks: write` publishes the dedicated ReviewGate check run.

Required secret:

```yaml
OPENROUTER_API_KEY
```

The action must update the existing PR summary comment containing `<!-- reviewgate-summary -->` instead of creating duplicate summary comments on every commit.

The optional `.reviewgate.yml` config can define review angles with exactly one instruction source per angle:

```yaml
review_angles:
  - id: correctness
    name: Correctness
    prompt_file: prompts/general.md
  - id: autoreview
    name: Auto Review
    skill: skills/autoreview
```

Use `prompt` for short inline text, `prompt_file` for repo-relative prompt files, and `skill` for a repo-relative skill directory containing `SKILL.md` or a direct `SKILL.md` path. Skill-backed angles pass skill instructions to the reviewing model; ReviewGate does not execute repository scripts, skill tools, or pull request code.

## Inputs

- `mode`: `review` for the normal model-backed path or `rereview` for the exact maintainer comment command. Defaults to `review`.
- `openrouter_api_key`: OpenRouter API key. Required only in `review` mode.
- `review_workflow`: Workflow file name selected by rereview mode. Defaults to `reviewgate.yml`.
- `config`: ReviewGate config path. Defaults to `.reviewgate.yml`.
- `model`: Exact OpenRouter model id. Defaults to ReviewGate's built-in model.
- `min_severity`: Lowest severity published as ReviewGate PR comments. Defaults to `P4`.
- `angle_timeout_seconds`: Maximum OpenRouter time for one review angle. Defaults to `180`.
- `total_timeout_seconds`: Maximum combined OpenRouter time across all review angles. Defaults to `480`.

Scores below `5` are reported as `needs_changes` in the JSON artifact and PR summary. Validated blockers publish a failing ReviewGate check-run conclusion but do not fail the workflow job. Reviewer timeout, empty/malformed output, provider, and transport failures are reported separately as `review_error` with `score: null` and a failing, inconclusive check; they are never represented as code-quality zeroes. Other execution or required publishing failures exit non-zero.

## Runtime

In `review` mode, the composite action first validates that the `openrouter_api_key` input is present, downloads the pinned Linux X64 release binary, and verifies its signed GitHub build provenance. It never compiles ReviewGate in the consumer workflow. It then posts or updates a short `ReviewGate: running` placeholder on pull requests, includes the pull request title and description as separate bounded untrusted scope context, runs the configured review angles within explicit per-angle and total budgets, writes `.reviewgate/review.json` and `.reviewgate/summary.md` into the repository workspace, appends the summary to the GitHub Actions step summary, replaces the placeholder with one canonical PR summary comment, posts eligible findings as inline PR comments when running on a pull request, and publishes a check-run status for review availability when permissions allow.

In `rereview` mode the action does not require `openrouter_api_key`, does not check out PR code, and does not run any model-backed review step. It validates the exact `@reviewgate review` command, uses maintainer association as an early filter, verifies that the actor currently has effective `write`, `maintain`, or `admin` repository permission, and verifies the open PR current head. If that exact head already has a completed canonical review, the request is an idempotent no-op. Otherwise it enumerates the configured workflow's runs with pagination and reruns only the newest completed `pull_request` run for that exact PR and SHA. The resulting review validates repository/PR-bound prior state and analyzes the delta since the previous reviewed SHA.

When updating an existing summary comment, the action reads the previous hidden state payload and re-renders the summary so cumulative run count, reviewed SHAs, and bounded cost history survive reruns. New review artifacts also include the changed-line count and a queue/startup/model/publish timing breakdown.

Inline comments are best-effort and deduped by hidden `<!-- reviewgate-finding:... -->` markers. Stale model-provided line anchors are repaired to matching changed lines when possible, and file-level, PR-level, or unanchored findings are attached to fallback right-side diff lines when needed. If no right-side diff anchor exists or GitHub rejects an inline comment, the full finding remains in `.reviewgate/review.json`; ReviewGate does not create standalone finding comments. Older standalone finding comments with `<!-- reviewgate-finding-comment:... -->` markers are cleaned up on later runs.

Canonical summary publishing is not silent: GitHub API or permission failures emit an Actions error and fail that publish step so maintainers can fix token permissions instead of getting a green run with no PR summary.

## Trigger Guidance

The simplest full install uses one workflow file with separate `pull_request` and `issue_comment` jobs. The event-specific jobs keep review and rereview permissions separate. Use the root README example as the canonical configuration.

The documented default install uses `LVTD-LLC/reviewgate@v0` so repositories receive compatible v0 updates. Pin to an exact commit SHA instead when your repository policy requires immutable third-party action references.

For public repositories, guard the ReviewGate job so it only runs on same-repository PR branches or explicit maintainer-triggered dispatches:

```yaml
jobs:
  reviewgate:
    if: >-
      ${{
        github.event_name == 'workflow_dispatch' ||
        (
          github.event.pull_request.head.repo.full_name == github.repository &&
          github.actor != 'dependabot[bot]'
        )
      }}
```

GitHub does not expose repository secrets to forked PRs or Dependabot PR events, so this guard prevents a ReviewGate run from failing only because `OPENROUTER_API_KEY` is unavailable. Keep untrusted fork review workflows on `pull_request`; do not switch to `pull_request_target` to get secret access.

The rereview job must use:

```yaml
permissions:
  actions: write
  attestations: read
  contents: read
  pull-requests: write
  issues: write

concurrency:
  group: reviewgate-rereview-${{ github.event.comment.id }}
  cancel-in-progress: false
```

The command contract is exact and case-sensitive. Only `OWNER`, `MEMBER`, and `COLLABORATOR` associations pass the early event filter, and the subsequent live permission check requires effective `write`, `maintain`, or `admin` access. The status marker keyed by `comment.id`, together with the concurrency group, suppresses event redelivery; a later command in a new comment remains an intentional retry. Reactions and final feedback updates are best effort and never control whether an eligible rerun is requested.
