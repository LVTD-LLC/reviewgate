---
layout: ../../layouts/DocsLayout.astro
title: "Troubleshoot ReviewGate"
description: "Diagnose ReviewGate installation, CLI, config, GitHub permissions, artifacts, reruns, inline findings, and review errors."
heading: "Troubleshoot ReviewGate"
lede: "Start from the observable symptom, verify the current head and execution mode, then apply the smallest recovery that preserves ReviewGate's security boundaries."
eyebrow: "OPERATE / TROUBLESHOOTING"
---

## Use this diagnostic order

Before changing configuration or code:

1. identify whether the failure is local CLI, review, provider, GitHub publishing, rereview, or agent retrieval;
2. confirm the repository, PR number, workflow file, and current head SHA;
3. inspect the Action log or exact CLI error;
4. check `status`, `score`, and `angle_errors` when an artifact exists;
5. verify required permissions and credentials without printing secrets;
6. reproduce with a deterministic fixture or mock path when possible;
7. rerun only after the underlying state changes or the error is explicitly retryable.

Do not weaken the event or permission model to make an error disappear.

## CLI is not installed

Symptom:

```text
reviewgate: command not found
```

Install:

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://raw.githubusercontent.com/LVTD-LLC/reviewgate/main/scripts/install.sh | sh
```

Or on macOS:

```bash
brew install LVTD-LLC/tap/reviewgate
```

Verify:

```bash
command -v reviewgate
reviewgate --help
```

Inside the ReviewGate repository, use the Cargo form without installing:

```bash
cargo run --locked -p reviewgate-cli -- --help
```

## Cargo is a mise shim with no Rust version

Symptom:

```text
mise ERROR No version is set for shim: cargo
```

ReviewGate requires Rust `1.96.0`. Configure it for the checkout:

```bash
mise use rust@1.96.0
```

Or run a one-off command:

```bash
mise exec rust@1.96.0 -- cargo --version
```

Verify:

```bash
rustc --version
cargo --version
```

Avoid changing a global toolchain when only this checkout needs the pinned version.

## Live review requires an OpenRouter key

Symptom:

```text
OPENROUTER_API_KEY is required for live review
```

For local live review:

```bash
read -r -s -p "OpenRouter API key: " OPENROUTER_API_KEY
printf '\n'
export OPENROUTER_API_KEY
reviewgate review-pr --repo .
```

For GitHub Actions, create the repository secret and pass it:

```yaml
with:
  openrouter_api_key: ${{ secrets.OPENROUTER_API_KEY }}
```

For a no-spend diagnostic, use:

```bash
reviewgate review-pr \
  --repo . \
  --mock-artifact fixtures/simple-review.json
```

Do not print the key to prove it exists.

## ReviewGate skips a fork or Dependabot PR

This is expected with the recommended workflow. GitHub does not expose repository secrets to untrusted fork or Dependabot `pull_request` events.

Verify the guard:

```yaml
if: >-
  ${{
    github.event.pull_request.head.repo.full_name == github.repository &&
    github.actor != 'dependabot[bot]'
  }}
```

Do not change the workflow to `pull_request_target`. Review forks through a separately designed, secret-free process if your repository needs that capability; it is not part of the v0 ReviewGate install.

## Merge base cannot be found

Symptom:

```text
failed to find merge-base for origin/<base>
```

The checkout likely lacks history. Use:

```yaml
- uses: actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5
  with:
    ref: ${{ github.event.pull_request.head.sha }}
    fetch-depth: 0
    persist-credentials: false
```

For local review:

```bash
git fetch origin main
GITHUB_BASE_REF=main reviewgate review-pr \
  --repo . \
  --mock-artifact fixtures/simple-review.json
```

Confirm `origin/main` exists before retrying.

## A low score has a green workflow job

This is intentional. Review outcome and process execution are separate:

- `score < 5` means `status: "needs_changes"`;
- the dedicated `ReviewGate` check reports failure;
- the review command can still exit successfully because it produced a valid result;
- required summary/check publication failures exit non-zero;
- `review_error` has `score: null` and a failing check.

Use the check or JSON:

```bash
reviewgate check --pr "$PR_NUMBER" \
  | jq '{status, score, reviewed_sha}'
```

Do not infer pass from the workflow conclusion alone.

## Review status is `review_error`

Inspect typed errors:

```bash
reviewgate check --pr "$PR_NUMBER" \
  | jq '.angle_errors'
```

Common causes:

- per-angle timeout;
- total review timeout;
- OpenRouter provider or transport failure;
- empty model response;
- malformed model JSON;
- deterministic artifact validation failure.

Retry only entries with `retryable: true`, and cap retries. A non-retryable artifact validation error requires inspecting the producing version, schema, and current-head invariants.

Do not change PR code to chase a provider timeout.

## Summary comment is missing

Check:

- `issues: write` exists on the review job;
- the event is an eligible `pull_request`;
- the Action log reaches `Publish ReviewGate summary`;
- the step is not hidden by a job condition;
- `GITHUB_TOKEN` is not empty;
- no custom wrapper swallows the publishing failure.

ReviewGate treats summary publication as required and should fail visibly.

## Summary comments are duplicated

ReviewGate trusts only bot-authored comments containing:

```html
<!-- reviewgate-summary -->
```

It chooses the best canonical state and deletes bot-authored duplicates. User-authored marker-shaped comments are ignored.

Check:

- the Action runs as `github-actions[bot]`;
- `issues: write` is present;
- only one workflow publishes ReviewGate summaries;
- an old custom integration is not posting its own summary;
- summary publishing is not wrapped in retry logic outside ReviewGate.

## Inline findings are missing

Check:

- `pull-requests: write` exists;
- `min_severity` includes the finding;
- the finding has a right-side diff anchor or another right-side fallback line exists;
- GitHub did not reject the review payload;
- the finding is present in JSON.

List eligible full-artifact findings:

```bash
jq -r '
  .findings[]
  | [.severity, .id, (.file // "PR"), (.line // "-"), .title]
  | @tsv
' .reviewgate/review.json
```

Inline publishing is best-effort. JSON remains complete when no valid GitHub anchor exists.

## The check run is missing

Check:

- `checks: write` exists;
- the Action log reaches `Publish ReviewGate check run`;
- the GitHub event contains or allows ReviewGate to resolve the current head;
- the token is available;
- the job has not been stopped before `always()` publishing steps.

ReviewGate fails the check-publishing step rather than silently leaving an old check as the apparent current gate.

## `gh` authentication fails

Check:

```bash
gh auth status
```

For a non-interactive shell:

```bash
export GH_TOKEN='token-from-a-secure-source'
```

The required scope depends on the command:

- `check` needs access to the repository, PR, workflow runs, and artifacts;
- `disposition` requires authenticated live write permission;
- `recheck` needs permission to enumerate and rerun Actions;
- publishing commands need the Action's documented job permissions.

Do not paste the token into a CLI argument or diagnostic output.

## `reviewgate check` cannot find a valid result

The command fails closed when there is no non-expired agent result from the configured workflow for the exact current PR head.

Verify:

```bash
gh pr view "$PR_NUMBER" --json headRefOid,url
gh workflow list
gh run list --workflow reviewgate.yml --limit 10
```

Common causes:

- the workflow file name is not `reviewgate.yml`;
- the current head has not completed a ReviewGate run;
- the artifact retention window expired;
- the selected workflow is wrong or ambiguous;
- the Action version predates stable agent-result upload;
- the workflow skipped this fork or bot PR.

Use an exact workflow selector:

```bash
reviewgate check \
  --pr "$PR_NUMBER" \
  --workflow .github/workflows/reviewgate.yml
```

The selector may be an exact numeric workflow ID, the workflow file name, the
GitHub workflow path, or an unambiguous display name. Do not use a result from
another workflow or SHA.

## The result is stale after a push

Observe:

```bash
result_sha="$(reviewgate check --pr "$PR_NUMBER" | jq -r .reviewed_sha)"
head_sha="$(gh pr view "$PR_NUMBER" --json headRefOid --jq .headRefOid)"
printf 'result=%s\nhead=%s\n' "$result_sha" "$head_sha"
```

If they differ, trigger or join the exact-head run and wait with a bound:

```bash
reviewgate review \
  --repo . \
  --pr "$PR_NUMBER" \
  --workflow reviewgate.yml \
  --wait \
  --timeout-seconds 600
```

Never treat an old `5/5` as current.

## Rereview command is ignored

The supported command is the entire exact comment:

```text
@reviewgate review
```

It is case- and whitespace-sensitive. Check:

- event is `issue_comment.created`;
- PR is open;
- comment has not been edited into place;
- author association is owner/member/collaborator;
- live permission is `write`, `maintain`, or `admin`;
- `review_workflow` matches the installed workflow file;
- an eligible completed run exists for the exact current head;
- the same comment event was not already processed;
- current head is not already fully reviewed.

An acknowledgement reaction can fail without invalidating an otherwise authorized rereview.

## Rereview reports no eligible run

ReviewGate rejects:

- a run for another PR;
- a run for another repository;
- a stale head SHA;
- a run triggered by a non-PR event;
- an in-progress run;
- a workflow mismatch.

Push or run the normal `pull_request` review job for the current head first. Rereview mode requests a rerun of an eligible review; it does not create the first review from arbitrary issue-comment context.

## Structured disposition is rejected

Check:

- the finding uses `semantic_fingerprint`, not `id`;
- `--status` is one of the supported submitted values;
- evidence is non-empty and at most 4096 bytes;
- the authenticated actor has current repository write permission;
- repository, PR, and reviewed SHA still match;
- the finding exists in canonical state;
- the commit-status attestation can be created;
- replay can verify either the exact status receipt or the actor's current repository-write permission;
- the review job grants `statuses: read`.

If GitHub makes both replay-verification paths unavailable, ReviewGate reports an operational error. Retry after GitHub status and permission APIs recover; do not treat the disposition as applied or resubmit it blindly.

Refresh the result before retrying:

```bash
mkdir -p .reviewgate
reviewgate check --pr "$PR_NUMBER" > .reviewgate/result.json
```

Do not resubmit the same assertion against a changed head without revalidation.

## Configuration errors

### Values appear ignored

Supported repository fields are `min_severity`, `deep`, `verify_blockers`, and
`review_angles`. Select `verifier_model` only through a trusted GitHub Action
input or direct CLI option; repository configuration is pull-request-controlled.
Removed keys are ignored with warnings.

For GitHub Actions, remember that the Action passes its `min_severity` input, default `P4`, as a CLI override. Set the intended value in the workflow.

The Action leaves `verify_blockers` empty by default so repository config is
not masked. Set it to exactly `true` or `false` only when the workflow should
override `.reviewgate.yml`.

### `review_angles must be a YAML list`

Use:

```yaml
review_angles:
  - id: general
    prompt_file: review-prompts/general.md
```

### Block scalar is rejected

This is unsupported:

```yaml
prompt: |
  Long instructions
```

Use `prompt_file`:

```yaml
prompt_file: review-prompts/long-review.md
```

### Angle source is invalid

Each angle needs exactly one of:

- `prompt`
- `prompt_file`
- `skill`

Remove extra sources or add the missing source.

### Path is rejected

Prompt and skill paths must be repository-relative and cannot contain `..`. Move the file inside the repository and reference it directly.

### Duplicate angle ID

Every configured `id` must be unique and contain only ASCII letters, numbers, `_`, or `-`.

Validate with a mock review:

```bash
reviewgate review-pr \
  --repo . \
  --config .reviewgate.yml \
  --mock-artifact fixtures/simple-review.json
```

## Generated `.reviewgate/` files appear in Git

They are local output:

```bash
git status --short .reviewgate
```

Inspect them, but do not commit them by default. Remove only the known generated files you created; do not delete a broad directory without confirming its contents.

Common generated paths:

```text
.reviewgate/review.json
.reviewgate/summary.md
.reviewgate/result.json
```

## `cargo audit` is missing

Install:

```bash
cargo install cargo-audit --locked
```

Run:

```bash
cargo audit
```

This is a ReviewGate repository contribution requirement, not a runtime dependency for consumer Action installations.

## Rust version does not match

Install:

```bash
rustup toolchain install 1.96.0 --component rustfmt --component clippy
```

The ReviewGate repository includes `rust-toolchain.toml`, so Rustup should select it automatically inside the checkout.

## Astro site checks fail

Use Node `24` and reinstall from the lockfile:

```bash
cd site
npm ci
npm run check
npm run build
```

If `astro: command not found`, dependencies are not installed. Run `npm ci`.

## Gather a bounded diagnostic report

When escalating, include:

```bash
reviewgate --help
gh auth status
git rev-parse HEAD
git status --short
```

Also include:

- ReviewGate Action reference;
- workflow file name;
- event type;
- runner OS/architecture;
- result status, reviewed SHA, and sanitized `angle_errors`;
- exact failing step and bounded error;
- whether the PR is same-repository, fork, or Dependabot;
- whether a retry already occurred.

Redact tokens, keys, headers, raw event payloads, private repository content, and model prompts.

## Next steps

- [Compare the reference GitHub workflow](/docs/github-actions).
- [Review accepted configuration fields](/docs/configuration).
- [Inspect artifact invariants](/docs/artifacts).
- [Reapply the external-agent stop conditions](/docs/agent-workflows#evaluate-exact-stop-conditions).
