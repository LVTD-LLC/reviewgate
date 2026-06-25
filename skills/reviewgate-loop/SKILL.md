# Review Gate Loop

Use when a user asks an agent to improve a PR until Review Gate reaches a target score.

Current status: draft scaffold. The first implementation should:

1. Read the latest Review Gate JSON artifact or canonical summary comment.
2. Identify findings whose score ceiling is below the configured `fail_under` threshold.
3. Apply focused fixes locally.
4. Push commits.
5. Wait for Review Gate to update the same summary comment.
6. Stop at 5/5, max attempts, or human-judgment findings.

Do not ignore Review Gate findings just because CI is green. The review score is the loop contract.
