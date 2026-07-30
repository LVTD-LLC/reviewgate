---
layout: ../../layouts/DocsLayout.astro
title: "Install ReviewGate with GitHub Actions"
description: "Add the ReviewGate workflow, least-privilege permissions, OpenRouter secret, reruns, and Action inputs to a repository."
heading: "Install ReviewGate with GitHub Actions"
lede: "Add one fork-safe workflow and one OpenRouter secret to publish a canonical score, inline findings, a check run, and a structured agent result."
eyebrow: "INSTALL / GITHUB ACTIONS"
---

## What the installation creates

For each eligible pull request, the review job:

1. downloads the version-pinned Linux X64 ReviewGate runtime;
2. verifies its GitHub build-provenance attestation;
3. collects the current PR diff and bounded repository context;
4. calls OpenRouter for each enabled review angle;
5. writes `.reviewgate/review.json` and `.reviewgate/summary.md`;
6. publishes inline findings when GitHub has a valid diff anchor;
7. creates or updates one canonical PR summary;
8. publishes the `ReviewGate` check run;
9. uploads `reviewgate-agent-result-<reviewed_sha>-attempt-<run_attempt>` for external agents.

The separate rereview job handles the exact maintainer command `@reviewgate review`. It does not check out PR code and never receives the OpenRouter secret.

## Before you begin

You need:

- repository admin access or permission to add Actions secrets and workflows;
- an OpenRouter API key;
- GitHub Actions enabled;
- pull requests originating from the same repository for the default review path.

The recommended workflow intentionally skips fork and Dependabot PRs because GitHub withholds repository secrets from those `pull_request` events.

## Add the OpenRouter secret

Create a repository Actions secret named exactly:

```text
OPENROUTER_API_KEY
```

With GitHub CLI, run this from the target repository and paste the key when prompted:

```bash
gh secret set OPENROUTER_API_KEY
```

Do not put the key in `.reviewgate.yml`, workflow source, action inputs as a literal, logs, or an agent prompt. The workflow passes the secret through `${{ secrets.OPENROUTER_API_KEY }}`.

## Add the workflow

Save this file as `.github/workflows/reviewgate.yml`:

```yaml
name: ReviewGate

on:
  pull_request:
    types: [opened, synchronize, reopened, ready_for_review]
  issue_comment:
    types: [created]

jobs:
  review:
    if: >-
      ${{
        github.event.pull_request.head.repo.full_name == github.repository &&
        github.actor != 'dependabot[bot]'
      }}
    runs-on: ubuntu-latest
    timeout-minutes: 20
    permissions:
      actions: read
      attestations: read
      contents: read
      pull-requests: write
      issues: write
      checks: write
      statuses: read
    concurrency:
      group: reviewgate-${{ github.workflow }}-${{ github.event.pull_request.number }}
      cancel-in-progress: true
    steps:
      - uses: actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5
        with:
          fetch-depth: 0
          persist-credentials: false

      - uses: LVTD-LLC/reviewgate@v0
        with:
          openrouter_api_key: ${{ secrets.OPENROUTER_API_KEY }}
          min_severity: P4

  rereview:
    if: >-
      ${{
        github.event_name == 'issue_comment' &&
        github.event.action == 'created' &&
        github.event.issue.pull_request &&
        github.event.issue.state == 'open' &&
        github.event.comment.body == '@reviewgate review' &&
        contains(fromJSON('["OWNER","MEMBER","COLLABORATOR"]'), github.event.comment.author_association)
      }}
    runs-on: ubuntu-latest
    timeout-minutes: 5
    permissions:
      actions: write
      attestations: read
      contents: read
      pull-requests: write
      issues: write
    concurrency:
      group: reviewgate-rereview-${{ github.event.comment.id }}
      cancel-in-progress: false
    steps:
      - uses: LVTD-LLC/reviewgate@v0
        with:
          mode: rereview
          review_workflow: reviewgate.yml
```

If you choose another workflow file name, set `review_workflow` in the rereview job to that file name.

## Why the workflow uses these guards

The review job condition checks two boundaries:

- `github.event.pull_request.head.repo.full_name == github.repository` allows only same-repository pull requests, where the repository secret is available.
- `github.actor != 'dependabot[bot]'` avoids invoking the model path when Dependabot does not receive the secret.

Do not replace `pull_request` with `pull_request_target` to expose secrets to fork code. ReviewGate reviews untrusted pull request content and must not move that content into a privileged execution context.

`fetch-depth: 0` gives ReviewGate enough history to find the merge base with the PR base branch. `persist-credentials: false` prevents the checkout step from leaving its token in Git configuration.

## Grant the review job permissions

| Permission | Why it is required |
| --- | --- |
| `actions: read` | Read workflow timing and artifact context. |
| `attestations: read` | Verify signed build provenance for the release archive. |
| `contents: read` | Check out and inspect the current PR diff and context. |
| `issues: write` | Create or update the canonical PR summary comment. PR conversation comments use the issues API. |
| `pull-requests: write` | Publish inline review comments. |
| `checks: write` | Publish the dedicated `ReviewGate` check run. |
| `statuses: read` | Verify the exact writer-only commit-status receipt for a structured disposition. During replay, ReviewGate can independently fall back to a fresh repository-write permission check when workflow-token status filtering hides that receipt. |

Do not hide summary or check-run failures with `continue-on-error`. The canonical summary and current check are product-critical outputs. Inline finding publication is best-effort; the complete finding set remains in JSON when an inline anchor is unavailable.

## Grant the rereview job permissions

The rereview job has a separate boundary:

| Permission | Why it is required |
| --- | --- |
| `actions: write` | Enumerate eligible runs and request a rerun. |
| `attestations: read` | Verify the ReviewGate runtime. |
| `contents: read` | Read repository metadata used during exact-run selection. |
| `pull-requests: write` | Verify the open PR/current head and reserve the command with a bot-owned PR comment. |
| `issues: write` | Add the acknowledgement reaction and bounded status feedback. |

The job never needs `OPENROUTER_API_KEY`. It reruns a previously approved `pull_request` workflow run for the exact current PR head.

## Configure Action inputs

| Input | Required | Default | Effect |
| --- | --- | --- | --- |
| `mode` | No | `review` | `review` runs model-backed review; `rereview` handles the exact maintainer command. |
| `openrouter_api_key` | In `review` mode | None | OpenRouter credential. Use the repository secret expression. |
| `review_workflow` | No | `reviewgate.yml` | Workflow file selected by rereview mode. |
| `config` | No | `.reviewgate.yml` | Repository-relative configuration path. |
| `model` | No | Built-in balanced model | Exact OpenRouter model ID. An empty value keeps the default. |
| `min_severity` | No | `P4` | Lowest severity published as an inline comment. |
| `angle_timeout_seconds` | No | `180` | Maximum runtime for one model review angle. |
| `total_timeout_seconds` | No | `480` | Maximum combined model runtime for the whole review. |

The built-in balanced model is `deepseek/deepseek-v4-flash`. The Action accepts an exact `model` override; it does not expose the CLI's `cheap`, `balanced`, or `strong` preset names as an Action input.

Action input values override `.reviewgate.yml` where the wrapper passes them explicitly. See [Configuration precedence](/docs/configuration#understand-configuration-precedence).

## Consume Action outputs

The composite Action exposes:

| Output | Meaning |
| --- | --- |
| `schema_version` | Stable agent-result version, currently `reviewgate-agent-result/v1`. |
| `status` | `passed`, `needs_changes`, or `review_error`. |
| `score` | Integer `0` through `5`; empty for `review_error`. |
| `reviewed_sha` | Exact pull request head reviewed. |
| `result_path` | Path to `.reviewgate/result.json` in the runner workspace. |

Example follow-up step:

```yaml
      - id: reviewgate
        uses: LVTD-LLC/reviewgate@v0
        with:
          openrouter_api_key: ${{ secrets.OPENROUTER_API_KEY }}

      - name: Print machine result location
        shell: bash
        env:
          REVIEWGATE_STATUS: ${{ steps.reviewgate.outputs.status }}
          REVIEWGATE_SCORE: ${{ steps.reviewgate.outputs.score }}
          REVIEWGATE_RESULT_PATH: ${{ steps.reviewgate.outputs.result_path }}
        run: |
          printf 'status=%s score=%s result=%s\n' \
            "$REVIEWGATE_STATUS" \
            "$REVIEWGATE_SCORE" \
            "$REVIEWGATE_RESULT_PATH"
```

Give the `uses` step an `id` before referencing its outputs.

## Choose a version reference

`LVTD-LLC/reviewgate@v0` is the recommended moving channel while ReviewGate is in v0. If your repository requires immutable third-party Action references, pin ReviewGate to an audited full commit SHA and schedule dependency updates.

The wrapper itself downloads a version-pinned ReviewGate Linux X64 archive and verifies its GitHub attestation before running it. Normal Action startup does not install Rust or compile ReviewGate source.

## Runner requirements

The supported Action runner is GitHub-hosted `ubuntu-latest` on Linux X64. The runtime path expects:

- Git;
- `curl`;
- GitHub CLI `gh` with attestation support;
- `tar`;
- GNU `date`.

Self-hosted runners, ARM runners, macOS, and Windows are not supported by the v0 Action runtime.

## Verify the installation

Open or update a same-repository pull request. A complete run should produce:

- one bot-authored PR comment containing `<!-- reviewgate-summary -->`;
- a `ReviewGate` check run for the current head;
- zero or more inline finding comments;
- an Actions artifact named `reviewgate-agent-result-<reviewed_sha>-attempt-<run_attempt>`;
- Action outputs for status, score, reviewed SHA, schema version, and result path.

Confirm the run reviewed the current PR head:

```bash
pr_number=123
head_sha="$(gh pr view "$pr_number" --json headRefOid --jq .headRefOid)"
reviewgate check --pr "$pr_number" \
  | jq -e --arg head "$head_sha" '.reviewed_sha == $head'
```

Replace `123` with the pull request number. A stale result must not be used to decide whether the current code passes.

## Configure maintainer-requested rereviews

With the `issue_comment` job installed, a maintainer can post exactly:

```text
@reviewgate review
```

Matching is case-sensitive and whitespace-sensitive. The whole comment must match. ReviewGate verifies the actor's current repository permission, the open PR, the exact current head SHA, and the eligible completed workflow run before requesting a rerun.

Commands for a stale SHA, another PR, another repository, an in-progress run, or an unauthorized actor are not rerun. Duplicate delivery of the same comment event is suppressed. A new comment creates a new request.

## Add branch protection

If you want ReviewGate to participate in merge protection, add the `ReviewGate` check to the branch's required status checks. Understand the result semantics first:

- `passed` produces a successful check;
- `needs_changes` produces a failing check;
- `review_error` produces a failing check because the review is unavailable;
- the overall workflow can still complete successfully for a completed `needs_changes` review.

Use the check result or the JSON status, not only the workflow job conclusion, as the gate signal.

## Next steps

- [Configure review angles and severity](/docs/configuration).
- [Understand scoring, evidence validation, and rereviews](/docs/features).
- [Give an external agent a safe repair loop](/docs/agent-workflows).
- [Troubleshoot skipped events and publishing failures](/docs/troubleshooting).
