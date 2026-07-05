# Action Wrapper

GitHub Action metadata lives at the repository root so users can install ReviewGate with:

```yaml
# ReviewGate is early, so @v0 is the recommended moving channel.
# Agents should not rewrite this to a latest commit SHA unless you want frozen updates.
- uses: LVTD-LLC/reviewgate@v0
```

The `v0` major tag follows current v0 releases so early adopters get the latest supported fixes and review behavior without tracking an arbitrary commit.

Implementation scripts and release download helpers can live in this directory as the wrapper grows.

The composite action stays thin: it collects inputs from GitHub Actions, passes them to the Rust binary, and lets the Rust crates own review logic, scoring, OpenRouter request construction, artifact validation, and summary rendering. The live review path currently runs separate general and adversarial prompts, then aggregates their findings into one artifact and one canonical PR summary.

The action is review-only. It publishes findings and status, but it does not run an autonomous code repair loop inside CI.

Required installation permissions:

```yaml
permissions:
  contents: read
  pull-requests: write
  issues: write
  checks: write
```

Required secret:

```yaml
OPENROUTER_API_KEY
```

The action must update the existing PR summary comment containing `<!-- reviewgate-summary -->` instead of creating duplicate summary comments on every commit.

## Inputs

- `openrouter_api_key`: OpenRouter API key. Required for live review.
- `config`: ReviewGate config path. Defaults to `.reviewgate.yml`.
- `model`: Exact OpenRouter model id. Defaults to ReviewGate's built-in model.
- `min_severity`: Lowest severity published as ReviewGate PR comments. Defaults to `P4`.

Scores below `5` are reported as `needs_changes` in the JSON artifact and PR summary. They publish a neutral ReviewGate check-run conclusion but do not fail the workflow; non-zero exits mean ReviewGate could not complete the review or a required publishing step failed.

## Runtime

The composite action first posts or updates a short `ReviewGate: running` placeholder on pull requests. It then runs the Rust CLI from the action checkout, runs the built-in review angles, writes `.reviewgate/review.json` and `.reviewgate/summary.md` into the repository workspace, appends the summary to the GitHub Actions step summary, replaces the placeholder with one canonical PR summary comment, posts eligible findings as inline PR comments when running on a pull request, and publishes a check-run status for review availability when permissions allow.

When updating an existing summary comment, the action reads the previous hidden state payload and re-renders the summary so cumulative run count, reviewed SHAs, and bounded cost history survive reruns. New review artifacts also include the changed-line count that the concise footer renders as the number of changed lines analyzed for the report.

Inline comments are best-effort and deduped by hidden `<!-- reviewgate-finding:... -->` markers. Stale model-provided line anchors are repaired to matching changed lines when possible, and file-level, PR-level, or unanchored findings are attached to fallback right-side diff lines when needed. If no right-side diff anchor exists or GitHub rejects an inline comment, the full finding remains in `.reviewgate/review.json`; ReviewGate does not create standalone finding comments. Older standalone finding comments with `<!-- reviewgate-finding-comment:... -->` markers are cleaned up on later runs.

Canonical summary publishing is not silent: GitHub API or permission failures emit an Actions error and fail that publish step so maintainers can fix token permissions instead of getting a green run with no PR summary.

## Trigger Guidance

The simplest install runs on PR updates and `workflow_dispatch`. Teams that want tighter cost control can use manual dispatch or the CLI `reviewgate recheck` helper to rerun the latest ReviewGate workflow run for a PR branch.

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
