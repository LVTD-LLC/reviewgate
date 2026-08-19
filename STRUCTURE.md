# Repository Structure

Use this file when deciding where new code, docs, prompts, schemas, fixtures, and agent assets belong.

## Directory Map

```text
crates/reviewgate-core/      Review artifact types, scoring, validation, summary rendering
crates/reviewgate-cli/       Local and CI command-line entrypoints
crates/reviewgate-github/    GitHub comment, inline review, and check-run publishing
action/                      GitHub Action documentation and wrapper support
site/                        Astro static site for reviewgate.lvtd.dev
deployment/                  Site container image and nginx config for CapRover
prompts/                     Built-in review stage prompts
schemas/                     JSON artifact schemas
fixtures/                    Golden review fixtures and deterministic examples
scripts/                     Repository validation and maintenance scripts
skills/check-reviewgate/     Public agent PR inspection skill
skills/reviewgate-loop/      Public agent loop skill
.reviewgate/                 Local generated review artifacts; do not commit by default
.github/workflows/           CI and repository automation
```

## Placement Rules

- Put deterministic scoring, validation, and rendering logic in `crates/reviewgate-core`.
- Put command parsing, file IO orchestration, and CI-friendly entrypoints in `crates/reviewgate-cli`.
- Put GitHub API code in `crates/reviewgate-github`.
- Put public website pages, components, and styles in `site/`.
- Put production website container/deployment support in `deployment/`.
- Put reusable model prompt text in `prompts/`.
- Put machine-readable artifact contracts in `schemas/`.
- Put small committed sample inputs in `fixtures/`.
- Put repository validation and maintenance scripts in `scripts/`.
- Put public agent skill instructions under `skills/check-reviewgate/` and `skills/reviewgate-loop/`.
- Keep `README.md` focused on a short public overview, installation, and common configuration path.
- Put comprehensive contributor and maintainer reference material in `AGENTS.md`, with task-focused public detail in `site/src/pages/docs/` and Action-specific detail in `action/README.md`.

## Naming Conventions

- Rust crates use the `reviewgate-*` prefix.
- CLI binary name is `reviewgate`.
- Review artifacts should use snake_case JSON fields.
- Finding IDs should be stable and machine-readable when generated.
- The canonical PR summary marker is `<!-- reviewgate-summary -->`.

## Test Placement

- Keep unit tests next to the Rust module they exercise.
- Add fixture files under `fixtures/` only when they are reusable across tests or docs.
- Prefer deterministic tests for scoring, summary rendering, schema compatibility, and GitHub publishing payloads.
- Avoid tests that require live GitHub or OpenRouter credentials by default.
