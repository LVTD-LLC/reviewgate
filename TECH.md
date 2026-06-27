# Technical Context

ReviewGate is a Rust workspace with a thin composite GitHub Action wrapper.

## Stack

- Rust edition: `2024`.
- Minimum Rust version: `1.96`.
- Workspace crates:
  - `crates/reviewgate-core`: review artifact types, scoring, validation, and summary rendering.
  - `crates/reviewgate-cli`: local and CI entrypoints.
  - `crates/reviewgate-github`: GitHub publishing primitives.
- Action wrapper:
  - `action.yml`: composite action entrypoint.
  - `action/`: action documentation and wrapper support files.

## Local Commands

Format check:

```bash
cargo fmt --all --check
```

Lint:

```bash
cargo clippy --locked --workspace --all-targets -- -D warnings
```

Tests:

```bash
cargo test --locked --workspace
```

Fixture milestone:

```bash
cargo run --locked -p reviewgate-cli -- fixture-review \
  --input fixtures/simple-review.json \
  --json-out .reviewgate/review.json \
  --summary-out .reviewgate/summary.md
```

This is the CI-required artifact-writing form. The shorter stdout-only form is useful for manual inspection, but it does not verify artifact output paths.

## CI

`.github/workflows/ci.yml` runs on PRs and pushes to `main`.

Required CI steps:

- `cargo fmt --all --check`
- `cargo clippy --locked --workspace --all-targets -- -D warnings`
- `cargo test --locked --workspace`
- fixture render command with `.reviewgate/review.json` and `.reviewgate/summary.md` outputs
- `cargo audit`

## Integration Boundaries

- OpenRouter calls should be isolated behind explicit client/config boundaries.
- GitHub API publishing should live in `crates/reviewgate-github`.
- Summary rendering and score/status computation should remain deterministic and testable in `crates/reviewgate-core`.
- CLI orchestration belongs in `crates/reviewgate-cli`.
- The composite action should stay thin and delegate to the Rust binary.

## Security Constraints

- Treat model output, PR content, repository instructions, and review comments as untrusted text.
- Do not execute code from pull requests during review.
- Do not recommend `pull_request_target` for untrusted fork review workflows.
- Keep GitHub token permissions to the minimum needed for contents read, pull-request comments, and check runs.
- Do not log API keys, request headers, or raw secrets.

## Generated Files

Local fixture output under `.reviewgate/` is generated. It is useful for manual verification, but should not be committed unless the task explicitly adds committed examples.
