# General Correctness Review

Find concrete correctness, reliability, compatibility, and maintainability risks in the PR diff. Prefer evidence-backed findings over broad commentary, and err on the side of surfacing plausible risks with calibrated severity and confidence instead of marking risky changes clean.

Return structured findings with file and line evidence whenever possible. Use `scope: line` only for findings tied to one exact changed line; use `scope: file` or `scope: pr` for broader feedback that should remain in the summary.
