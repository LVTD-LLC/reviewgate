# Repository Structure

Use this file when deciding where new code, docs, prompts, schemas, fixtures, and agent assets belong.

## Directory Map

```text
crates/shipcheck-core/      Review artifact types, scoring, validation, summary rendering
crates/shipcheck-cli/       Local and CI command-line entrypoints
crates/shipcheck-github/    GitHub comment, inline review, and check-run publishing
action/                      GitHub Action documentation and wrapper support
prompts/                     Built-in review stage prompts
schemas/                     JSON artifact schemas
fixtures/                    Golden review fixtures and deterministic examples
skills/shipcheck-loop/      Public agent loop skill draft
.shipcheck/                 Local generated review artifacts; do not commit by default
.github/workflows/           CI and repository automation
```

## Placement Rules

- Put deterministic scoring, validation, and rendering logic in `crates/shipcheck-core`.
- Put command parsing, file IO orchestration, and CI-friendly entrypoints in `crates/shipcheck-cli`.
- Put GitHub API code in `crates/shipcheck-github`.
- Put reusable model prompt text in `prompts/`.
- Put machine-readable artifact contracts in `schemas/`.
- Put small committed sample inputs in `fixtures/`.
- Put public agent-loop instructions under `skills/shipcheck-loop/`.
- Put user-facing install and usage documentation in `README.md` or `action/README.md`.

## Naming Conventions

- Rust crates use the `shipcheck-*` prefix.
- CLI binary name is `shipcheck`.
- Review artifacts should use snake_case JSON fields.
- Finding IDs should be stable and machine-readable when generated.
- The canonical PR summary marker is `<!-- shipcheck-summary -->`.

## Test Placement

- Keep unit tests next to the Rust module they exercise.
- Add fixture files under `fixtures/` only when they are reusable across tests or docs.
- Prefer deterministic tests for scoring, summary rendering, schema compatibility, and GitHub publishing payloads.
- Avoid tests that require live GitHub or OpenRouter credentials by default.
