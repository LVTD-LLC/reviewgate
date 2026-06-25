# Review UX and Control v1

Review Gate's GitHub Action reviews pull requests and reports results. It must not run an autonomous repair loop inside CI.

The intended workflow is:

1. Review Gate reviews the PR diff and context.
2. Review Gate updates one canonical PR summary comment and writes a JSON artifact.
3. A human or external coding agent reads the findings.
4. The human or agent ships fixes.
5. Review Gate is rerun and updates the same summary.

External agent loops can be documented and supported, but they are separate from the action's responsibility.

## Trigger Direction

Default installation should remain low-headache:

- Run on `pull_request` events for `opened`, `synchronize`, `reopened`, and `ready_for_review`.
- Support `workflow_dispatch` for manual reruns.
- Use the `reviewgate recheck` CLI helper to rerun the latest Review Gate workflow run for a PR branch when GitHub CLI auth is available.
- Add PR comment or label-based recheck commands later if users want an in-GitHub control surface.

Running on every push is acceptable as the simplest default while the project is early. It should remain configurable because some repos will prefer explicit reruns to control cost and noise.

## Gate Direction

`fail_under` is still useful, but it should be framed as a CI/check policy, not the essence of the product.

Recommended modes:

- `report_only`: always publish the review and keep the workflow green.
- `check_only`: publish a failing GitHub check below `fail_under` without necessarily failing the job.
- `fail_job`: publish the review and fail the workflow below `fail_under`.

The review summary should always include `target_score`, `fail_under`, and the resulting status so humans and agents can understand the policy.

## Severity Visibility

Users need separate controls for what is visible and what blocks:

- `summary_min_severity`: lowest severity shown in the summary.
- `inline_min_severity`: lowest severity posted as inline PR review comments.
- `blocking_severity_floor` or `fail_under`: policy used to compute check/job status.

Defaults should avoid noise:

- Show P0-P4 findings in the summary by default, with `summary_min_severity` available for quieter installs.
- Post inline comments only for high-confidence P0-P2 line-specific findings.
- Keep all findings in the JSON artifact even when the visible summary filters lower-severity items.

## Cost Direction

The canonical summary should show:

- Current run estimated cost.
- Cumulative PR estimated cost.
- Compact component history by stage/model.

Review Gate has no external database in the action-first architecture, so cumulative state should be stored in the canonical summary's hidden metadata and preserved on update.

The summary now stores versioned hidden state with reviewed SHAs, run count, cumulative estimated cost, and bounded cost history. The visible summary remains human-readable; the hidden payload is for robust rerendering on later runs.

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
