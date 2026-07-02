# General Correctness Review

Find concrete correctness, reliability, compatibility, and maintainability risks in the PR diff. Prefer a small number of high-confidence findings over broad commentary.

Return structured findings with file and line evidence whenever possible. Use `scope: line` only for findings tied to one exact changed line; use `scope: file` or `scope: pr` for broader feedback that should remain in the summary.
