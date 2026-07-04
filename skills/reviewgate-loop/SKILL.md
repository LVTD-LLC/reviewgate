# ReviewGate Loop

Use when a user asks an agent to improve a PR until ReviewGate reaches `5/5`.

ReviewGate's loop contract is:

1. Read the latest ReviewGate JSON artifact at `.reviewgate/review.json` when available.
2. If the JSON artifact is unavailable, read the canonical PR summary comment containing `<!-- reviewgate-summary -->`.
3. Identify findings whose score ceiling is below `5` as score-affecting.
4. Apply focused local fixes for score-affecting findings first, then lower-priority findings if human judgment calls for them.
5. Run the repository's required local checks.
6. Commit and push.
7. Wait for ReviewGate to update the same summary comment.
8. Stop when `status == "passed"` and `score == 5`, when max attempts are reached, or when a finding needs human judgment.

Do not ignore ReviewGate findings just because CI is green. The review score is the loop contract.

Status handling:

- `needs_changes`: the review completed, but the score is below `5/5`.
- `passed`: the score is `5/5`; verify no unresolved review comments remain.
