# Changelog

## Unreleased

- Kept docs, agent workflow guidance, CI commands, and summary rendering aligned with Rust 1.96, Rust 2024, locked dependency use, and dynamic fail-under thresholds.
- Aligned the review artifact status computation and CLI exit behavior with the configured fail-under threshold, and pinned CI setup to auditable toolchain inputs.
- Added repo steering files for coding agents, product constraints, technical context, and repository structure.
- Addressed PR review feedback by surfacing agent instructions in summaries, making severity scoring explicit, and removing an unused YAML dependency.
- Created the initial Review Gate Rust workspace, CLI, GitHub Action scaffold, schemas, prompts, and deterministic fixture milestone.
