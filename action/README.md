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

The action must update the existing PR summary comment containing `<!-- reviewgate-summary -->` instead of creating duplicate summary comments on every commit.

## Inputs

- `openrouter_api_key`: OpenRouter API key. Required for live review.
- `config`: ReviewGate config path. Defaults to `.reviewgate.yml`; falls back to `.shipcheck.yml` during migration when the new default file is absent.
- `target_score`: Score required for a fully passing review. Defaults to `5`.
- `fail_under`: Score floor that fails CI. Defaults to `4`.
- `report_only`: Publish results without failing CI. Defaults to `false`.
- `gate_mode`: Failed-review behavior. `job` fails the workflow; `report` publishes only. `check` is reserved for a future dedicated Check Run publisher.
- `preset`: OpenRouter model preset used when `model` is not pinned. Defaults to `balanced`.
- `model`: Exact OpenRouter model id. Defaults to the selected preset model.
- `mock_artifact`: Optional artifact path for dry-run workflows.
- `summary_min_severity`: Lowest severity shown in the canonical summary. Defaults to `P4`.
- `inline_min_severity`: Lowest severity eligible for future inline comments. Defaults to `P2`.
- `inline_min_confidence`: Minimum model confidence required for inline comments. Defaults to `0.80`.
- `publish_inline_comments`: Whether eligible line-specific findings are posted as PR review comments. Defaults to `true`.

`fail_under` controls gate behavior. It is not required for teams using ReviewGate only as a report; set `gate_mode: report` or the compatibility alias `report_only: "true"` for that mode.

## Runtime

The composite action runs the Rust CLI from the action checkout, writes `.reviewgate/review.json` and `.reviewgate/summary.md` into the repository workspace, appends the summary to the GitHub Actions step summary, upserts one canonical PR summary comment, and posts eligible inline comments when running on a pull request.

When updating an existing summary comment, the action reads the previous hidden state payload and re-renders the summary so cumulative run count, reviewed SHAs, and bounded cost history survive reruns.

Inline comments are best-effort and deduped by hidden `<!-- reviewgate-finding:... -->` markers. If GitHub rejects a line comment because the line is no longer in the diff, the finding remains visible in the canonical summary and publishing continues.

## Trigger Guidance

The simplest install runs on PR updates and `workflow_dispatch`. Teams that want tighter cost control can use manual dispatch or the CLI `reviewgate recheck` helper to rerun the latest ReviewGate workflow run for a PR branch.
