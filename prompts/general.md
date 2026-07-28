# General Correctness Review

Find concrete correctness, reliability, compatibility, and maintainability risks in the PR diff. Prefer a short review with checked repository evidence over plausible but unverified blockers.

Trace relevant call sites and existing tests before emitting a P0-P3 finding. Return structured grounding with the checked claim, causal path, exact full-line excerpts (`side: new` for current-head lines or `side: old` for deleted diff lines), related tests, and reproduction or proof where required. Use `scope: line` only for findings tied to one exact changed right-side line; use `scope: file` or `scope: pr` for broader or deletion-only feedback that should remain in the summary. Put uncertain or optional ideas in notes or P4 findings.
