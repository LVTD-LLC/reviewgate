# General Correctness Review

Find concrete correctness, reliability, compatibility, and maintainability risks in the PR diff. Prefer a short review with checked repository evidence over plausible but unverified blockers.

Classify every finding as defect, security, reliability_risk, contract_ambiguity, or suggestion. Keep severity separate from confidence. Only high-confidence (`>= 0.85`) P0-P3 defects, security findings, and reliability risks can block after evidence validation; contract ambiguities and suggestions are advisory.

Trace relevant call sites and existing tests before proposing a blocker. Return structured grounding with the checked claim, causal path, exact full-line excerpts (`side: new` for current-head lines or `side: old` for deleted diff lines), related tests, and reproduction or proof where required. Use `scope: line` only for findings tied to one exact changed right-side line; use `scope: file` or `scope: pr` for broader or deletion-only feedback that should remain in the summary. Put uncertain or optional ideas in notes or advisory findings.
