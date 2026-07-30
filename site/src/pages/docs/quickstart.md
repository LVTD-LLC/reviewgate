---
layout: ../../layouts/DocsLayout.astro
title: "ReviewGate quickstart"
description: "Choose GitHub Actions or the CLI, run a first ReviewGate review, and verify the generated score and artifacts."
heading: "Run your first ReviewGate review"
lede: "Choose the execution mode that matches your environment, produce a review artifact, and verify that the result is fresh and machine-readable."
eyebrow: "GET STARTED / FIRST VERIFIED RESULT"
---

## Decide where the review should run

Use the GitHub Action when you want ReviewGate to run automatically on pull requests and publish to GitHub. Use the CLI when you want a local review, a deterministic smoke test, or artifacts that a coding agent can inspect before pushing.

| Requirement | GitHub Action | Local CLI |
| --- | --- | --- |
| Automatic pull request trigger | Yes | No |
| Canonical PR summary, inline comments, and check run | Yes | No |
| Local JSON and Markdown artifacts | Created inside the workflow | Yes |
| Live model call | Requires `OPENROUTER_API_KEY` | Requires `OPENROUTER_API_KEY` |
| Secret-free deterministic path | Not for a full review job | `fixture-review` or `--mock-artifact` |
| Best fit | Repository-wide installation | Development, CI experimentation, and agent loops |

If you need automatic GitHub publishing, continue with [the complete GitHub Actions installation](/docs/github-actions). The rest of this page proves the local core path without spending model credits.

## Before you begin

You need:

- Git;
- Rustup and Cargo;
- Rust `1.96.0`;
- a clone of the ReviewGate repository;
- `jq` to run the inspection examples.

You do **not** need an OpenRouter key for the fixture or mock paths.

## Clone ReviewGate

```bash
git clone https://github.com/LVTD-LLC/reviewgate.git
cd reviewgate
```

The repository pins Rust in `rust-toolchain.toml`. Confirm the active toolchain:

```bash
rustc --version
cargo --version
```

Expected Rust compiler:

```text
rustc 1.96.0
```

If a tool manager such as mise owns the `cargo` shim, configure Rust `1.96.0` for the checkout before continuing. A `cargo` shim with no selected version cannot build or install the CLI.

## Run the deterministic fixture

This command validates fixture JSON, recomputes score and status, and writes both product artifacts:

```bash
cargo run --locked -p reviewgate-cli -- fixture-review \
  --input fixtures/simple-review.json \
  --json-out .reviewgate/review.json \
  --summary-out .reviewgate/summary.md
```

Inspect the result:

```bash
jq '{
  score,
  status,
  reviewed_sha,
  finding_count: (.findings | length)
}' .reviewgate/review.json
```

The fixture declares `score: 5` but contains a material `P2` finding. ReviewGate recomputes the result from the validated findings, so the generated artifact reports `score: 3` and `status: "needs_changes"`. This proves that the caller cannot force a passing score by supplying contradictory top-level values.

Inspect the human-readable summary:

```bash
sed -n '1,160p' .reviewgate/summary.md
```

Generated files under `.reviewgate/` are local outputs. Do not commit them unless your repository intentionally versions samples.

## Exercise checkout context without a model call

Use `--mock-artifact` to exercise repository context collection and output rendering:

```bash
cargo run --locked -p reviewgate-cli -- review-pr \
  --repo . \
  --mock-artifact fixtures/simple-review.json \
  --json-out .reviewgate/review.json \
  --summary-out .reviewgate/summary.md
```

When `GITHUB_BASE_REF` is unset, local `review-pr` uses `git show HEAD` as its diff source. To review the full branch delta against `main`, fetch the base and set `GITHUB_BASE_REF`:

```bash
git fetch origin main
GITHUB_BASE_REF=main cargo run --locked -p reviewgate-cli -- review-pr \
  --repo . \
  --mock-artifact fixtures/simple-review.json \
  --json-out .reviewgate/review.json \
  --summary-out .reviewgate/summary.md
```

## Run a live local review

A live review calls OpenRouter and can consume credits. Export the key without writing it into a command, config file, shell history entry, or repository:

```bash
read -r -s -p "OpenRouter API key: " OPENROUTER_API_KEY
printf '\n'
export OPENROUTER_API_KEY

cargo run --locked -p reviewgate-cli -- review-pr \
  --repo . \
  --json-out .reviewgate/review.json \
  --summary-out .reviewgate/summary.md
```

ReviewGate runs the built-in `general` and `adversarial` angles unless `.reviewgate.yml` replaces them. The default per-angle timeout is `180` seconds and the default whole-review timeout is `480` seconds.

## Verify the result before acting on it

An agent or script should fail closed if the artifact is missing or malformed. At minimum, verify:

```bash
test -s .reviewgate/review.json
jq -e '
  (.status == "passed" or .status == "needs_changes" or .status == "review_error")
  and (.reviewed_sha | type == "string" and length > 0)
  and (.findings | type == "array")
' .reviewgate/review.json >/dev/null
```

For a local repository, compare `reviewed_sha` with the intended commit:

```bash
reviewed_sha="$(jq -r '.reviewed_sha' .reviewgate/review.json)"
current_sha="$(git rev-parse HEAD)"
test "$reviewed_sha" = "$current_sha"
```

Do not repair findings from a stale artifact. Regenerate the review against the current head first.

## Understand what succeeded

The local command succeeded when:

- `.reviewgate/review.json` exists and validates;
- `.reviewgate/summary.md` exists;
- `reviewed_sha` identifies the commit you intended to review;
- `status` is one of `passed`, `needs_changes`, or `review_error`.

`needs_changes` is a completed review with validated blockers. `review_error` is an inconclusive review and has `score: null`. A process exit of zero does not by itself mean `5/5`.

## Next steps

- [Install the GitHub Action](/docs/github-actions) to publish reviews automatically.
- [Install and use the CLI](/docs/cli) for command-by-command reference.
- [Configure review angles](/docs/configuration) for repository-specific coverage.
- [Implement the external-agent loop](/docs/agent-workflows) to iterate safely toward `5/5`.
