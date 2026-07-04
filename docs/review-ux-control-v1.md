# Review UX and Control v1

ReviewGate's GitHub Action reviews pull requests and reports results. It must not run an autonomous repair loop inside CI.

The intended workflow is:

1. ReviewGate reviews the PR diff and context.
2. ReviewGate updates one concise canonical PR summary comment, writes a JSON artifact, and posts eligible findings inline or as standalone PR comments.
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

The status policy is fixed:

- `passed`: the score is `5/5`.
- `needs_changes`: the review completed, but the score is below `5/5`.

Non-zero action exits are reserved for execution failures, such as being unable to collect context, call the model, validate the artifact, write outputs, or publish the required summary.

## Severity Visibility

Users need one control for how much ReviewGate publishes back to the PR:

- `min_severity`: lowest severity published as ReviewGate PR comments.

Defaults should avoid hiding findings:

- Keep the summary concise: verdict, score, compact finding counts, collapsed context sections, and the run/cost/latest-commit footer.
- Post line-scoped findings inline when they can be anchored to a changed line.
- Post file/PR-scoped or unanchored line findings as standalone PR comments.
- Keep all findings in the JSON artifact even when `min_severity` filters lower-severity PR comments.

## Cost Direction

The default canonical summary should show:

- Cumulative PR estimated cost in the compact footer.
- Detailed cost components in the JSON artifact.

ReviewGate has no external database in the action-first architecture, so cumulative state should be stored in the canonical summary's hidden metadata and preserved on update.

The summary stores versioned hidden state with reviewed SHAs, run count, cumulative estimated cost, and bounded cost history. The visible summary remains human-readable; the hidden payload is for robust rerendering on later runs.

## Model Defaults

The action should expose an exact `model` input for users who want stability. If unset, ReviewGate uses its built-in default model so the action surface stays small.

## Security

Default docs must avoid unsafe `pull_request_target` patterns. The recommended workflow should:

- Use least-privilege token permissions.
- Avoid running arbitrary PR code.
- Avoid exposing `OPENROUTER_API_KEY` to untrusted fork PRs.
- Treat model output as untrusted text.
