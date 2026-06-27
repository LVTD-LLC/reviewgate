# ReviewGate v0.1.0 Release Checklist

Do not publish to GitHub Marketplace until this checklist is complete.

## Code and CI

- `cargo fmt --all --check` passes.
- `cargo clippy --locked --workspace --all-targets -- -D warnings` passes.
- `cargo test --locked --workspace` passes.
- `cargo audit` passes.
- The ReviewGate dogfood workflow runs on a real PR with `OPENROUTER_API_KEY` configured.
- The action updates one canonical PR summary comment on rerun instead of creating duplicates.
- A failing review exits non-zero by default and report-only mode does not block CI.

## Release Metadata

- `CHANGELOG.md` contains the v0.1.0 changes.
- Cargo package versions are set to `0.1.0`.
- The release tag is immutable after publish.
- The README install snippet pins the release tag.

## Safety

- The action does not use `pull_request_target`.
- Required permissions are documented.
- Secrets are passed only through `OPENROUTER_API_KEY` and are not logged.
- Fork behavior is documented before enabling broad public use.

## Marketplace Gate

- Create a GitHub release first.
- Install ReviewGate in one small external test repository.
- Confirm summary/comment behavior, failure behavior, and artifact output.
- Only then evaluate Marketplace publishing.
