# Action Wrapper

GitHub Action metadata lives at the repository root so users can install ReviewGate with:

```yaml
- uses: LVTD-LLC/reviewgate@v0
```

Implementation scripts and release download helpers can live in this directory as the wrapper grows.

The composite action stays thin: it collects inputs from GitHub Actions, passes them to the Rust binary, and lets the Rust crates own review logic, scoring, OpenRouter request construction, artifact validation, and summary rendering.

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

The action must update the existing PR summary comment containing `<!-- reviewgate-summary -->` instead of creating duplicate summary comments on repeated comment-triggered runs.

## Inputs

- `openrouter_api_key`: OpenRouter API key. Required for live review.
- `config`: ReviewGate config path. Defaults to `.reviewgate.yml`.
- `target_score`: Score required for a fully passing review. Defaults to `5`.
- `preset`: OpenRouter model preset used when `model` is not pinned. Defaults to `balanced`.
- `model`: Exact OpenRouter model id. Defaults to the selected preset model.
- `mock_artifact`: Optional artifact path for dry-run workflows.
- `summary_min_severity`: Lowest severity shown in the canonical summary. Defaults to `P4`.
- `summary_style`: Summary detail level. `concise` is the default PR UX; `detailed` includes full cost, metrics, findings, notes, and agent instructions. Defaults to `concise`.
- `inline_min_severity`: Lowest severity eligible for inline comments. Defaults to `P2`.
- `inline_min_confidence`: Minimum model confidence required for inline comments. Defaults to `0.80`.
- `publish_inline_comments`: Whether eligible line-specific findings are posted as PR review comments. Defaults to `true`.

Scores below `target_score` are reported as `needs_changes` in the JSON artifact and PR summary. They publish a neutral ReviewGate check-run conclusion but do not fail the workflow; non-zero exits mean ReviewGate could not complete the review or a required publishing step failed.

## Runtime

The composite action first posts or updates a short `ReviewGate: running` placeholder on pull requests. It then runs the Rust CLI from the action checkout, writes `.reviewgate/review.json` and `.reviewgate/summary.md` into the repository workspace, appends the summary to the GitHub Actions step summary, replaces the placeholder with one canonical PR summary comment, posts eligible inline comments when running on a pull request event or PR comment event, and publishes a check-run status for review availability when permissions allow.

When updating an existing summary comment, the action reads the previous hidden state payload and re-renders the summary so cumulative run count, reviewed SHAs, and bounded cost history survive reruns.

Inline comments are best-effort and deduped by hidden `<!-- reviewgate-finding:... -->` markers. Stale model-provided line anchors are repaired to matching changed lines when possible. Findings with no publishable changed line stay as compact fallback entries in the concise summary. If GitHub rejects a line comment, the workflow emits a warning and the full finding remains in `.reviewgate/review.json`; use `summary_style: detailed` when you want all finding detail in the summary comment.

Canonical summary publishing is not silent: GitHub API or permission failures emit an Actions error and fail that publish step so maintainers can fix token permissions instead of getting a green run with no PR summary.

## Trigger Guidance

The default install is comment-triggered. Comment `/reviewgate` on a pull request to run ReviewGate; the workflow should not run on every commit.

For public repositories, guard the ReviewGate job so it only runs for PR comments from maintainer-style authors:

```yaml
on:
  issue_comment:
    types: [created]

jobs:
  reviewgate:
    if: >-
      ${{
        github.event.issue.pull_request != null &&
        (
          github.event.comment.body == '/reviewgate' ||
          startsWith(github.event.comment.body, '/reviewgate ')
        ) &&
        (
          github.event.comment.author_association == 'OWNER' ||
          github.event.comment.author_association == 'MEMBER' ||
          github.event.comment.author_association == 'COLLABORATOR'
        )
      }}
```

For comment-triggered workflows, resolve the PR with `gh api`, skip cross-repository PRs before passing `OPENROUTER_API_KEY` to ReviewGate, check out the PR head SHA, fetch the base branch, and pass `REVIEWGATE_PR_NUMBER`, `REVIEWGATE_PR_HEAD_SHA`, and `REVIEWGATE_BASE_REF` to the action. Keep untrusted fork review workflows off `pull_request_target`; do not switch to it to get secret access.
