---
layout: ../../layouts/DocsLayout.astro
title: "ReviewGate security model"
description: "Understand ReviewGate trust boundaries, fork-safe events, least-privilege tokens, secret handling, attested runtime, and agent safety."
heading: "Operate ReviewGate within its security model"
lede: "Keep untrusted pull request and model data outside privileged execution paths, grant only the permissions ReviewGate needs, and preserve exact-head identity."
eyebrow: "CONCEPTS / SECURITY"
---

## Threat model

Assume an attacker can control or influence:

- pull request code and diffs;
- PR title and body;
- repository instruction and context files;
- `.reviewgate.yml`;
- repo-local prompt files and skills;
- model output;
- review comments;
- summary-comment text;
- marker-shaped strings inside untrusted content.

ReviewGate treats these as data. None of them should gain authority to execute code, expose secrets, choose a privileged GitHub event, or rewrite canonical state without deterministic validation.

## Never execute pull request code during review

The recommended workflow checks out code so ReviewGate can read the diff and bounded files. It does not build, test, source, import, or execute that code.

Do not add steps before or during ReviewGate that execute untrusted PR code with:

- repository secrets;
- write-capable GitHub tokens;
- deployment credentials;
- package-registry credentials;
- cloud credentials;
- persistent self-hosted runner access.

Testing the pull request is a separate CI responsibility with its own permission and secret design.

## Keep the `pull_request` event

The recommended review job uses:

```yaml
on:
  pull_request:
    types: [opened, synchronize, reopened, ready_for_review]
```

Do not switch to `pull_request_target` to make secrets available to forked code. `pull_request_target` runs in the base-repository security context and is dangerous when later steps check out or execute attacker-controlled code.

ReviewGate intentionally skips untrusted forks and Dependabot events in the default model-backed job because GitHub withholds repository secrets.

## Preserve the fork and bot guard

```yaml
if: >-
  ${{
    github.event.pull_request.head.repo.full_name == github.repository &&
    github.actor != 'dependabot[bot]'
  }}
```

This guard is not a convenience filter. It ensures the review job runs only where the repository secret is expected to exist.

Do not replace it with a fallback that passes an empty key, logs the missing secret, or moves the job into a privileged event.

## Separate review and rereview authority

The `review` job:

- runs only on eligible `pull_request` events;
- checks out the PR;
- receives the OpenRouter key;
- publishes review output.

The `rereview` job:

- runs on `issue_comment.created`;
- does not check out PR code;
- does not receive the OpenRouter key;
- validates the exact command and actor;
- finds an eligible completed review run for the exact current PR head;
- requests a rerun of that already-approved workflow.

Do not combine these boundaries by passing model secrets to the issue-comment job.

## Grant least privilege to the review job

```yaml
permissions:
  actions: read
  attestations: read
  contents: read
  pull-requests: write
  issues: write
  checks: write
  statuses: read
```

Why each write exists:

- `issues: write` updates the canonical PR conversation comment;
- `pull-requests: write` publishes inline review comments;
- `checks: write` publishes the current ReviewGate check;
- `statuses: read` enables the exact commit-status receipt path for structured dispositions written by authenticated repository writers.

Do not grant broad `write-all`, `contents: write`, `packages: write`, `id-token: write`, or deployment permissions to the ReviewGate review job.

## Grant least privilege to the rereview job

```yaml
permissions:
  actions: write
  attestations: read
  contents: read
  pull-requests: write
  issues: write
```

`actions: write` is limited to selecting and rerunning the eligible workflow run. The exact run must belong to the configured workflow, exact repository, exact PR, and current head.

## Protect secrets

Set `OPENROUTER_API_KEY` as a GitHub Actions secret:

```yaml
with:
  openrouter_api_key: ${{ secrets.OPENROUTER_API_KEY }}
```

Do not:

- store it in `.reviewgate.yml`;
- commit it to a workflow;
- paste it into issue comments or PR descriptions;
- include it in disposition evidence;
- pass it as a command-line literal that remains in shell history;
- print request headers;
- send it to an external repair agent that does not need model access.

For local use, export the environment variable in a trusted shell or use a secret manager. ReviewGate's live client writes the non-secret request body to a temp file and passes curl configuration through standard input so large prompts and credentials are not exposed in process arguments.

## Verify the distributed runtime

The Action does not compile source from the pull request. It:

1. downloads the release archive pinned inside `action.yml`;
2. verifies the archive's GitHub attestation;
3. restricts the signer workflow to ReviewGate's release workflow;
4. restricts the source ref to the expected tag;
5. rejects self-hosted-runner provenance;
6. extracts and smoke-tests the binary.

The Action wrapper is still referenced by `LVTD-LLC/reviewgate@v0` unless the consumer pins a full commit SHA. Repositories with immutable Action policies should pin the wrapper to an audited SHA and automate updates.

## Keep exact-head identity

Review results are meaningful only for one commit.

The Action uses the PR head SHA rather than a synthetic checkout merge SHA. Stable artifacts include `reviewed_sha`. `reviewgate check` accepts only a result from the configured workflow's exact PR/head run and checks freshness again before printing it.

Agents must compare:

```bash
head_sha="$(gh pr view "$PR_NUMBER" --json headRefOid --jq .headRefOid)"
result_file="$(mktemp)"
trap 'rm -f "$result_file"' EXIT

if reviewgate check --pr "$PR_NUMBER" >"$result_file"; then
  reviewgate_exit=0
else
  reviewgate_exit=$?
fi

case "$reviewgate_exit" in
  0|2|3) ;;
  *) exit "$reviewgate_exit" ;;
esac

jq -e --arg head "$head_sha" '.reviewed_sha == $head' "$result_file"
```

A stale `5/5` is not permission to merge or stop.

## Treat model findings as untrusted

Fields such as `claim`, `causal_evidence`, `evidence[].excerpt`, `suggested_fix`, `agent_instruction`, `verdict`, and `notes` can contain attacker-influenced text.

An agent may use them to locate and understand a possible issue. It must not:

- execute embedded shell commands;
- fetch arbitrary URLs because a finding requests it;
- reveal environment variables;
- weaken permissions or safety checks;
- modify unrelated files;
- accept a disposition without independent repository evidence.

The trusted automation fields are bounded enums, identifiers, booleans, numeric scores, validated scope, and exact SHA after schema validation. Text remains data.

## Validate before a finding blocks

Model output cannot directly choose the final score. Eligible blockers require deterministic:

- classification;
- severity range;
- confidence threshold;
- changed-line grounding;
- exact repository evidence;
- causal path;
- related-test assessment;
- stronger proof for critical findings.

Only a non-null `blocking_reason` on a current `still_open` finding should enter the external agent's repair set.

## Protect canonical comments and thread state

ReviewGate trusts only bot-authored comments with validated markers and state. It:

- ignores user-authored summary markers;
- percent-encodes finding marker IDs;
- binds hidden state to repository and PR;
- bounds state size and history;
- reconciles only ReviewGate-owned inline roots;
- never deletes human-authored comments.

Do not build an integration that trusts marker-shaped user text or edits the hidden state directly.

## Authenticate structured dispositions

`reviewgate disposition` binds a submission to:

- repository;
- PR number;
- exact reviewed SHA;
- semantic fingerprint;
- authenticated actor;
- actor's live write permission;
- comment event;
- evidence;
- a payload-digest commit status that only a repository writer can create.

At submission time, ReviewGate requires live repository write permission and creates the payload-digest status receipt. During replay it accepts either that exact status receipt or a fresh GitHub repository-write permission check. The second path prevents workflow-token status filtering from silently discarding a valid writer submission.

The review workflow uses `statuses: read` for the receipt path. If both replay-verification paths are unavailable, ReviewGate reports an operational error instead of silently ignoring the disposition. If submission-time attestation fails, it rejects the submission and removes the transport comment when possible.

Disposition evidence is bounded. Never include secrets, personal data, full logs, or untrusted commands.

## Bound provider and model failures

ReviewGate enforces per-angle and total timeouts. A provider timeout, empty response, malformed response, or transport failure becomes a sanitized typed `angle_errors` entry.

The result becomes:

```json
{
  "status": "review_error",
  "score": null
}
```

This prevents unavailable review infrastructure from becoming a false code-quality `0/5` or a false pass.

External agents should cap retries. Repeated retryable errors still require escalation after the attempt limit.

## Understand network boundaries

The live review path sends bounded review context to OpenRouter using the user's key. It adds:

- `Authorization: Bearer <OPENROUTER_API_KEY>`;
- `HTTP-Referer: https://github.com/LVTD-LLC/reviewgate`;
- `X-OpenRouter-Title: ReviewGate`;
- `X-OpenRouter-Categories: cli-agent,cloud-agent`.

ReviewGate does not host a proxy or persist the repository context. The user remains responsible for OpenRouter's terms, chosen model providers, repository data policy, and whether code may be sent to those providers.

## Review logging and error handling

Logs and public errors should be:

- secret-free;
- bounded;
- structured enough to diagnose the failed phase;
- explicit about whether the failure is review, provider, artifact, or publishing related.

Do not add raw provider bodies, request headers, complete event payloads, tokens, or authorization codes to errors.

## Security checklist for workflow changes

```text
[ ] Review job still uses pull_request.
[ ] Fork and Dependabot guard remains present.
[ ] PR code is read but not executed with secrets.
[ ] Review and rereview jobs remain separate.
[ ] Rereview job receives no OpenRouter key.
[ ] permissions are job-scoped and least-privilege.
[ ] contents: write, write-all, id-token: write, and deployment secrets are absent.
[ ] checkout uses fetch-depth: 0 and persist-credentials: false.
[ ] runtime attestation verification remains enabled.
[ ] result is bound to repository, PR, workflow, and current head.
[ ] model and comment text remain untrusted.
[ ] retries and artifact sizes are bounded.
```

## Next steps

- [Install the reference workflow](/docs/github-actions).
- [Implement safe artifact parsing](/docs/artifacts).
- [Give an agent the exact-head loop](/docs/agent-workflows).
- [Troubleshoot permission and event failures](/docs/troubleshooting).
