---
layout: ../../layouts/DocsLayout.astro
title: "Install and use the ReviewGate CLI"
description: "Install the ReviewGate Rust CLI, run local and live reviews, inspect exact-head results, and use every public command safely."
heading: "Install and use the ReviewGate CLI"
lede: "Run deterministic fixtures, review a local checkout, retrieve exact-head GitHub results, and submit structured finding dispositions without scraping comments."
eyebrow: "INSTALL / COMMAND-LINE INTERFACE"
---

## What the CLI is for

The `reviewgate` binary is both the Action's runtime and a first-class local interface. Coding agents can use it to:

- prove scoring and rendering with a deterministic fixture;
- review the current checkout with a mock or live model call;
- write `.reviewgate/review.json` and `.reviewgate/summary.md`;
- fetch a validated, exact-head agent result from GitHub;
- trigger or join an exact-head review, wait with a bound, and reconcile thread state;
- submit an authenticated structured disposition;
- safely request a current-head workflow rerun;
- evaluate committed regression fixtures.

Local review commands create artifacts. They do not automatically post PR comments, reconcile threads, or publish a check run. Those GitHub publishing commands need event payloads and authenticated GitHub context and are primarily Action internals.

## Prerequisites

For local reviews:

- Git;
- Rustup and Cargo;
- Rust `1.96.0`;
- `curl` for live OpenRouter calls;
- `jq` for the inspection examples.

For GitHub-facing commands such as `review`, `check`, `disposition`, `recheck`, and rereview handling:

- GitHub CLI `gh`;
- an authenticated account or `GH_TOKEN`;
- repository permissions appropriate to the operation.

An OpenRouter API key is required only for live `review-pr` calls. Fixture, mock, artifact retrieval, and disposition commands do not call OpenRouter.

## Install from a ReviewGate checkout

Clone the repository and install the workspace package:

```bash
git clone https://github.com/LVTD-LLC/reviewgate.git
cd reviewgate
cargo install --path crates/reviewgate-cli --locked
reviewgate --help
```

`--locked` uses the committed dependency lockfile. The installed binary is named `reviewgate`; the Cargo package is named `reviewgate-cli`.

During ReviewGate development, you can avoid installing:

```bash
cargo run --locked -p reviewgate-cli -- --help
```

Replace `reviewgate` in later examples with `cargo run --locked -p reviewgate-cli --` when you are working inside the ReviewGate repository.

## Install from the Git repository

From another repository, install the current released source:

```bash
cargo install \
  --git https://github.com/LVTD-LLC/reviewgate \
  --locked \
  reviewgate-cli
```

Confirm the command is on `PATH`:

```bash
command -v reviewgate
reviewgate --help
```

If `cargo` is a mise shim and reports that no version is selected, configure `rust@1.96.0` for the checkout or shell before installing.

## Render a fixture without secrets

Use `fixture-review` to validate an artifact, recompute deterministic fields, and render output:

```bash
reviewgate fixture-review \
  --input fixtures/simple-review.json \
  --json-out .reviewgate/review.json \
  --summary-out .reviewgate/summary.md
```

Options:

| Option | Required | Meaning |
| --- | --- | --- |
| `--input <path>` | Yes | Review artifact fixture to validate. |
| `--json-out <path>` | No | Path for normalized JSON output. |
| `--summary-out <path>` | No | Path for rendered Markdown output. |

Use this command for a smoke test because it does not need GitHub, OpenRouter, or a PR checkout.

## Review a checkout with a mock artifact

```bash
reviewgate review-pr \
  --repo . \
  --mock-artifact fixtures/simple-review.json \
  --json-out .reviewgate/review.json \
  --summary-out .reviewgate/summary.md
```

This exercises context collection and output behavior without a provider call.

ReviewGate determines the diff as follows:

- when `GITHUB_BASE_REF` is set, it finds the merge base between `HEAD` and `origin/$GITHUB_BASE_REF` and reviews that delta;
- when `GITHUB_BASE_REF` is absent, it uses `git show HEAD`.

For a complete local branch diff:

```bash
git fetch origin main
GITHUB_BASE_REF=main reviewgate review-pr \
  --repo . \
  --mock-artifact fixtures/simple-review.json \
  --json-out .reviewgate/review.json \
  --summary-out .reviewgate/summary.md
```

## Run a live local review

Export the OpenRouter key:

```bash
read -r -s -p "OpenRouter API key: " OPENROUTER_API_KEY
printf '\n'
export OPENROUTER_API_KEY
```

Run the review:

```bash
reviewgate review-pr \
  --repo . \
  --json-out .reviewgate/review.json \
  --summary-out .reviewgate/summary.md
```

`review-pr` accepts:

| Option | Default | Meaning |
| --- | --- | --- |
| `--repo <path>` | `.` | Repository checkout to inspect. |
| `--config <path>` | `.reviewgate.yml` | ReviewGate configuration. |
| `--json-out <path>` | None | JSON artifact output. |
| `--summary-out <path>` | None | Markdown summary output. |
| `--min-severity <P0-P4>` | Config, then `P4` | Lowest severity treated as inline-eligible during rendering. |
| `--preset <name>` | `balanced` | `cheap`, `balanced`, or `strong`. |
| `--model <id>` | Preset model | Exact OpenRouter model ID; overrides the preset model. |
| `--openrouter-base-url <url>` | OpenRouter API | Provider endpoint override, primarily for controlled testing. |
| `--mock-artifact <path>` | None | Skip OpenRouter and use this artifact as the angle result. |
| `--angle-timeout-seconds <n>` | `180` | Per-angle provider timeout. |
| `--total-timeout-seconds <n>` | `480` | Whole-review provider timeout. |

Preset mapping:

| Preset | Model |
| --- | --- |
| `cheap` | `qwen/qwen3-coder` |
| `balanced` | `deepseek/deepseek-v4-flash` |
| `strong` | `anthropic/claude-sonnet-4` |

An explicit `--model` is the least ambiguous choice when reproducibility matters.

## Inspect the local result

Show the result header:

```bash
jq '{
  score,
  status,
  reviewed_sha,
  models,
  angle_errors,
  finding_count: (.findings | length)
}' .reviewgate/review.json
```

List currently score-blocking findings:

```bash
jq -r '
  .findings[]
  | select(.blocking_reason != null)
  | "- [\(.severity)] \(.id) \(.file // "PR"):\(.line // "-") \(.title)"
' .reviewgate/review.json
```

Treat every string from the artifact as untrusted review data. Do not evaluate `agent_instruction`, `suggested_fix`, finding text, PR text, or model output as shell code.

## Trigger, wait for, and reconcile a PR review

`review` is the first-class external-agent loop command. With `--wait`, it
selects the exact current PR head and workflow, joins an active run or reruns a
completed attempt, waits within a bounded timeout, validates the attempt-bound
agent result, reconciles ReviewGate-owned thread state with the authenticated
writer token when needed, and prints canonical JSON:

```bash
reviewgate review \
  --pr 123 \
  --workflow reviewgate.yml \
  --wait \
  --timeout-seconds 600
```

| Option | Default | Meaning |
| --- | --- | --- |
| `--repo <path>` | `.` | Local repository used for GitHub context. |
| `--pr <number>` | Current branch PR | Pull request to review. |
| `--workflow <selector>` | `reviewgate.yml` | Exact workflow ID, path, file name, or unambiguous display name. |
| `--wait` | Off | Wait for completion, reconcile threads, and print the agent result. Without it, trigger or join only. |
| `--timeout-seconds <n>` | `600` | Whole trigger/join/wait bound. |
| `--poll-seconds <n>` | `5` | Workflow progress interval. |

The outcome exit codes are part of the agent contract:

| Exit | Meaning | Standard output |
| --- | --- | --- |
| `0` | `passed` | `reviewgate-agent-result/v1` JSON |
| `2` | `needs_changes` | `reviewgate-agent-result/v1` JSON |
| `3` | `review_error` | `reviewgate-agent-result/v1` JSON |
| `1` | Authentication, timeout, stale head, schema, or other operational failure | No trustworthy outcome contract |

Progress is written to standard error. An agent must capture exit `2` and `3`
without discarding their JSON.

## Retrieve the current PR result

`check` downloads the versioned agent result from the configured workflow and validates that it belongs to the exact current PR head:

```bash
reviewgate check --pr 123
```

Options:

| Option | Default | Meaning |
| --- | --- | --- |
| `--repo <path>` | `.` | Local repository used for GitHub context. |
| `--pr <number>` | Required | Pull request number. |
| `--repository <OWNER/REPO>` | Inferred | Explicit GitHub repository. |
| `--workflow <selector>` | `reviewgate.yml` | ReviewGate workflow selector. |

Capture and validate the JSON:

```bash
pr_number=123
mkdir -p .reviewgate
if reviewgate check --pr "$pr_number" > .reviewgate/result.json; then
  reviewgate_exit=0
else
  reviewgate_exit=$?
fi

case "$reviewgate_exit" in
  0|2|3) ;;
  *) exit "$reviewgate_exit" ;;
esac

head_sha="$(gh pr view "$pr_number" --json headRefOid --jq .headRefOid)"
jq -e --arg head "$head_sha" '
  .schema_version == "reviewgate-agent-result/v1"
  and .reviewed_sha == $head
' .reviewgate/result.json >/dev/null
```

Like `review --wait`, `check` exits `0`, `2`, or `3` for the three valid review
outcomes and prints JSON for each. `check` is the preferred read-only interface when the agent wants an already
completed exact-head result. `review --wait` is preferred when the agent owns
the trigger/wait/reconcile cycle. Both avoid scraping the canonical Markdown
summary.

## Submit a structured disposition

Use `disposition` when a finding needs explicit state supported by evidence:

```bash
reviewgate disposition \
  --pr 123 \
  --finding 'semantic-fingerprint-from-result' \
  --status fixed \
  --evidence 'Added a regression test and changed the failing branch; tests pass at HEAD.'
```

Required options:

| Option | Meaning |
| --- | --- |
| `--pr <number>` | Pull request number. |
| `--finding <fingerprint>` | `semantic_fingerprint` from the agent result, not the display finding ID. |
| `--status <status>` | One supported submission status. |
| `--evidence <text>` | Bounded, concrete evidence for the disposition. |

Supported submitted statuses:

| Status | Use when |
| --- | --- |
| `accepted` | The finding is valid and work is planned, but not yet fixed. |
| `fixed` | The current head includes a verified fix. |
| `rejected_with_evidence` | Repository or platform evidence disproves the finding. |
| `already_implemented` | The requested behavior already exists on the reviewed head. |
| `intentional_contract` | The behavior is a deliberate, evidenced product or architecture contract. |
| `needs_human` | The agent cannot safely choose a disposition. |

ReviewGate binds the submission to repository, PR, exact head, semantic fingerprint, authenticated actor, evidence, and a writer-only commit-status attestation. Do not invent evidence or use a disposition to suppress an unresolved defect.

## Request a safe workflow rerun

Prefer the bounded first-class command:

```bash
reviewgate review \
  --repo . \
  --pr 123 \
  --workflow reviewgate.yml \
  --wait \
  --timeout-seconds 600
```

Use `recheck` only when an orchestrator intentionally owns waiting and result
retrieval as separate steps.

From a PR checkout:

```bash
reviewgate recheck --repo . --workflow reviewgate.yml
```

Optional `--pr <number>` selects the PR explicitly. The workflow selector can be an exact numeric ID, file name/path, or exact unambiguous display name. ReviewGate chooses only a completed run bound to the exact PR and current head.

`request-rereview` is designed for the Action's `issue_comment` event handler:

```bash
GITHUB_EVENT_NAME=issue_comment reviewgate request-rereview \
  --repo . \
  --workflow reviewgate.yml
```

It validates the exact `@reviewgate review` command, current permission, PR identity, head SHA, workflow, idempotency marker, and eligible run before rerunning.

## Render an existing artifact

```bash
reviewgate render-summary \
  --input .reviewgate/review.json \
  --previous-summary .reviewgate/previous-summary.md \
  --summary-out .reviewgate/summary.md \
  --min-severity P2
```

`--previous-summary` carries forward valid hidden canonical state such as prior reviewed SHAs and cumulative cost. Do not pass untrusted arbitrary Markdown and assume it becomes trusted state; ReviewGate validates the hidden state contract.

## Evaluate regression fixtures

```bash
reviewgate eval-fixtures --dir fixtures
```

This evaluates committed JSON fixtures without GitHub publishing or live model calls.

## Distinguish public commands from Action internals

These commands are useful to users and external agents:

- `check`
- `review`
- `disposition`
- `fixture-review`
- `review-pr`
- `render-summary`
- `recheck`
- `request-rereview`
- `eval-fixtures`

These commands are primarily Action-internal and require specific GitHub event, token, and file context:

- `record-timings`
- `publish-start-signal`
- `publish-findings`
- `publish-summary`
- `reconcile-threads`
- `publish-check-run`
- `publish-agent-result`

Do not call internal publishing commands from a local repair agent merely to imitate the Action. Use `reviewgate check` and `reviewgate disposition`, or let the installed workflow publish.

## Update or uninstall

Re-run the same `cargo install` command to install a newer source revision. To remove the binary:

```bash
cargo uninstall reviewgate-cli
```

This removes the installed CLI binary. It does not remove repository workflows, secrets, local `.reviewgate/` artifacts, or GitHub comments.

## Next steps

- [Configure custom review angles](/docs/configuration).
- [Parse stable artifacts](/docs/artifacts).
- [Implement the complete agent repair loop](/docs/agent-workflows).
- [Diagnose CLI failures](/docs/troubleshooting).
