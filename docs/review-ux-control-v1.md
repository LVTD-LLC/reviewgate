# Review UX and Control v1

ReviewGate's GitHub Action reviews pull requests and reports results. It must not run an autonomous repair loop inside CI.

The intended workflow is:

1. ReviewGate reviews the PR diff and context.
2. ReviewGate updates one concise canonical PR summary comment, writes a JSON artifact, and posts eligible line-specific findings inline.
3. A human or external coding agent reads the findings.
4. The human or agent ships fixes.
5. ReviewGate is rerun and updates the same summary.

External agent loops can be documented and supported, but they are separate from the action's responsibility.

## Trigger Direction

Default installation should remain low-headache:

- Run on `pull_request` events for `opened`, `synchronize`, `reopened`, and `ready_for_review`.
- Support `workflow_dispatch` for manual reruns.
- Use the `reviewgate recheck` CLI helper to rerun the latest ReviewGate workflow run for a PR branch when GitHub CLI auth is available.
- Add PR comment or label-based recheck commands later if users want an in-GitHub control surface.

Running on every push is acceptable as the simplest default while the project is early. It should remain configurable because some repos will prefer explicit reruns to control cost and noise.

## Status Direction

ReviewGate's action should remain review-only. It reports score quality and publishes findings, but a low score should not fail the GitHub Actions job.

`target_score` is the status policy:

- `passed`: the score meets or exceeds `target_score`.
- `needs_changes`: the review completed, but the score is below `target_score`.

Non-zero action exits are reserved for execution failures, such as being unable to collect context, call the model, validate the artifact, write outputs, or publish the required summary.

## Severity Visibility

Users need separate controls for what is visible and what should be fixed before the target score is expected:

- `summary_min_severity`: lowest severity shown in the summary.
- `inline_min_severity`: lowest severity posted as inline PR review comments.
- `target_score`: policy used to compute review status and target-blocking finding counts.

Defaults should avoid noise:

- Keep the summary concise by default: verdict, score, status, one-line cost, compact finding counts, and short fallback entries only for findings that are not eligible for inline comments.
- Use `summary_style: detailed` when a repo wants the old full summary with cost details, metrics, findings, notes, and agent instructions.
- Post inline comments only for high-confidence P0-P2 findings with `scope: line`.
- Keep all findings in the JSON artifact even when the visible summary filters lower-severity items.

## Cost Direction

The default canonical summary should show:

- Cumulative PR estimated cost as a single line, for example `Cost: $0.08 (3 runs)`.
- Detailed cost components only when `summary_style: detailed` is enabled.

ReviewGate has no external database in the action-first architecture, so cumulative state should be stored in the canonical summary's hidden metadata and preserved on update.

The summary stores versioned hidden state with reviewed SHAs, run count, cumulative estimated cost, and bounded cost history. The visible summary remains human-readable; the hidden payload is for robust rerendering on later runs.

## Model Defaults

Model config should use preset aliases so defaults can improve over time:

- `cheap`: `qwen/qwen3-coder`
- `balanced`: `deepseek/deepseek-v4-flash`
- `strong`: `anthropic/claude-sonnet-4`

Users can pin exact OpenRouter model IDs when they want stability.

## Security

Default docs must avoid unsafe `pull_request_target` patterns. The recommended workflow should:

- Use least-privilege token permissions.
- Avoid running arbitrary PR code.
- Avoid exposing `OPENROUTER_API_KEY` to untrusted fork PRs.
- Treat model output as untrusted text.
