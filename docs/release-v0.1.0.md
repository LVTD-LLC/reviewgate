# v0.1.0 Release Readiness

Release only after these checks pass:

- CI `rust` is green on `main`.
- ReviewGate dogfood workflow posts one canonical summary and updates it on rerun.
- Inline comments are best-effort and do not fail the workflow when a finding cannot map to a diff line.
- Low-score reviews report `needs_changes` without failing the workflow.
- `reviewgate recheck` works with GitHub CLI auth.
- `eval-fixtures` has been run against at least 20 historical PR artifacts.
- A smoke-test repository can install a pinned tag and receive a review without Marketplace publishing.

Marketplace publishing should stay deferred until a tagged release has been smoke-tested in a second repository.
