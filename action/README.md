# Action Wrapper

GitHub Action metadata lives at the repository root so users can install Review Gate with:

```yaml
- uses: LVTD-LLC/review-gate@v0
```

Implementation scripts and release download helpers can live in this directory as the wrapper grows.

The composite action stays thin: it collects inputs from GitHub Actions, passes them to the Rust binary, and lets the Rust crates own review logic, scoring, OpenRouter request construction, artifact validation, and summary rendering.

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
- `mock_artifact`: Optional artifact path for dry-run workflows.

## Runtime

The composite action runs the Rust CLI from the action checkout, writes `.reviewgate/review.json` and `.reviewgate/summary.md` into the repository workspace, appends the summary to the GitHub Actions step summary, and upserts one canonical PR summary comment when running on a pull request.
