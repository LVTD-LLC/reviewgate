# Action Wrapper

GitHub Action metadata lives at the repository root so users can install Review Gate with:

```yaml
- uses: LVTD-LLC/review-gate@v0
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

The action must update the existing PR summary comment containing `<!-- review-gate-summary -->` instead of creating duplicate summary comments on every commit.

## Inputs

- `openrouter_api_key`: OpenRouter API key. Required for live review.
- `config`: Review Gate config path. Defaults to `.reviewgate.yml`.
- `target_score`: Score required for a fully passing review. Defaults to `5`.
- `fail_under`: Score floor that fails CI. Defaults to `4`.
- `report_only`: Publish results without failing CI. Defaults to `false`.
- `preset`: OpenRouter model preset used when `model` is not pinned. Defaults to `balanced`.
- `model`: Exact OpenRouter model id. Defaults to the selected preset model.
- `mock_artifact`: Optional artifact path for dry-run workflows.
- `summary_min_severity`: Lowest severity shown in the canonical summary. Defaults to `P4`.
- `inline_min_severity`: Lowest severity eligible for future inline comments. Defaults to `P2`.

`fail_under` controls workflow/check behavior. It is not required for teams using Review Gate only as a report; set `report_only: "true"` for that mode.

## Runtime

The composite action runs the Rust CLI from the action checkout, writes `.reviewgate/review.json` and `.reviewgate/summary.md` into the repository workspace, appends the summary to the GitHub Actions step summary, and upserts one canonical PR summary comment when running on a pull request.

When updating an existing summary comment, the action reads the previous hidden state payload and re-renders the summary so cumulative run count, reviewed SHAs, and bounded cost history survive reruns.

## Trigger Guidance

The simplest install runs on PR updates and `workflow_dispatch`. Teams that want tighter cost control can use manual dispatch or the CLI `reviewgate recheck` helper to rerun the latest Review Gate workflow run for a PR branch.
