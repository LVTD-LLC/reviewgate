# ReviewGate Loop

Use when a user asks an agent to improve a PR until ReviewGate reaches a target score.

ReviewGate's loop contract is:

1. Read the latest ReviewGate JSON artifact at `.reviewgate/review.json` when available.
2. If the JSON artifact is unavailable, read the canonical PR summary comment containing `<!-- reviewgate-summary -->`.
3. Identify findings whose score ceiling is below the configured `fail_under` threshold as blocking.
4. Apply focused local fixes for blocking findings first, then non-blocking findings if the target score requires them.
5. Run the repository's required local checks.
6. Commit and push.
7. Wait for ReviewGate to update the same summary comment.
8. Stop when `status == "passed"` and `score >= target_score`, when max attempts are reached, or when a finding needs human judgment.

Do not ignore ReviewGate findings just because CI is green. The review score is the loop contract.

Status handling:

- `failed`: the gate is closed; fix blocking findings before merge.
- `needs_changes`: the hard floor passed, but the target score is not met.
- `passed`: the target score is met; verify no unresolved review comments remain.
