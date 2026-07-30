# ReviewGate v0.1.0 Release Checklist

Do not publish to GitHub Marketplace until this checklist is complete.

## Code and CI

- Merge every feature and release-workflow change intended for the release before creating the runtime tag; verify the tag commit contains those changes.
- `cargo fmt --all --check` passes.
- `cargo clippy --locked --workspace --all-targets -- -D warnings` passes.
- `cargo test --locked --workspace` passes.
- `cargo audit` passes.
- The ReviewGate dogfood workflow runs on a real PR with `OPENROUTER_API_KEY` configured.
- The action updates one canonical PR summary comment on rerun instead of creating duplicates.
- After moving the `v0` major tag, a fresh consumer workflow run follows `docs/v0-smoke.md` and proves `LVTD-LLC/reviewgate@v0` resolves to the new release SHA.
- A low-score review leaves the workflow green, while review execution failures exit non-zero.

## Release Metadata

- `CHANGELOG.md` contains the v0.1.0 changes.
- Cargo package versions are set to `0.1.0`.
- The release tag is immutable after publish.
- The release runtime workflow uploads and attests Linux X64 and macOS Apple Silicon/Intel archives plus SHA-256 checksum files.
- Publish and verify the new runtime release before advancing `REVIEWGATE_RUNTIME_VERSION` in `action.yml`; update that pin in a follow-up change so `main` never references a missing release.
- A clean `ubuntu-latest` consumer run verifies the attestation, performs no source compilation, and records startup at or below 15 seconds.
- The standalone installer downloads and checksum-verifies the new release on each supported platform.
- `Formula/reviewgate.rb` in `LVTD-LLC/homebrew-tap` points to the new release URL and checksum.
- Clean `brew install LVTD-LLC/tap/reviewgate` and `reviewgate upgrade` smoke tests pass.

## Safety

- The action does not use `pull_request_target`.
- Required permissions are documented.
- Secrets are passed only through `OPENROUTER_API_KEY` and are not logged.
- Fork behavior is documented before enabling broad public use.

## Marketplace Gate

- Push the exact release tag and let `release-runtime.yml` publish the draft only after the runtime is built, attested, verified, and executed.
- Install ReviewGate in one small external test repository.
- Install the standalone CLI with the README one-liner and exercise an upgrade.
- Confirm concise summary, inline comment, review-execution failure behavior, and artifact output.
- Only then evaluate Marketplace publishing.
