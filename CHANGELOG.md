# Changelog

## Unreleased

- Added configurable summary/inline severity floors, a `reviewgate recheck` helper, and hidden summary state for bounded cumulative PR cost/run history.
- Added Review UX and Control v1 dogfood notes and design guidance for review-only semantics, trigger/recheck choices, model defaults, cost display, severity visibility, and secure workflow behavior.
- Added `P0` severity support, structured cost summary metadata, and cost rendering in canonical summaries.
- Updated OpenRouter preset defaults to prefer price-to-value models: `qwen/qwen3-coder` for cheap and `deepseek/deepseek-v4-flash` for balanced.
- Added the first live `review-pr` CLI path for PR diff/context collection, OpenRouter artifact parsing, summary/artifact output, configurable exit semantics, and mock-artifact dry runs.
- Wired the GitHub Action to run the live CLI path, publish a step summary, and upsert one canonical PR summary comment.
- Added a dogfood Review Gate workflow and v0.1.0 release-readiness checklist.
- Hardened curl-based OpenRouter calls so secrets and large request bodies are not passed through process arguments.
- Added OpenRouter BYOK model-client boundary types with redacted secret handling, explicit model presets, and mocked transport tests.
- Added GitHub canonical summary upsert planning with create/update/no-op behavior and mocked publisher tests.
- Expanded the public agent-loop contract for JSON artifacts, canonical summary fallback, status handling, and stop conditions.
- Added Rust-side review artifact validation, summary status output, lockfile audit/provenance documentation, and cleaned Review Gate context file references.
- Kept docs, agent workflow guidance, CI commands, and summary rendering aligned with Rust 1.96, Rust 2024, locked dependency use, and dynamic fail-under thresholds.
- Aligned the review artifact status computation and CLI exit behavior with the configured fail-under threshold, and pinned CI setup to auditable toolchain inputs.
- Added repo steering files for coding agents, product constraints, technical context, and repository structure.
- Addressed PR review feedback by surfacing agent instructions in summaries, making severity scoring explicit, and removing an unused YAML dependency.
- Created the initial Review Gate Rust workspace, CLI, GitHub Action scaffold, schemas, prompts, and deterministic fixture milestone.
