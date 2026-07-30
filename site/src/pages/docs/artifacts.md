---
layout: ../../layouts/DocsLayout.astro
title: "ReviewGate artifacts and outputs"
description: "Parse ReviewGate review JSON, stable agent results, Action outputs, findings, angle errors, thread state, and disposition schemas."
heading: "Consume ReviewGate artifacts and outputs"
lede: "Use the stable exact-head agent result for automation, the full review artifact for diagnostics, and schemas instead of scraping the canonical Markdown summary."
eyebrow: "REFERENCE / MACHINE-READABLE CONTRACTS"
---

## Choose the correct artifact

ReviewGate writes two JSON shapes for different consumers:

| Artifact | Path | Contract | Use it for |
| --- | --- | --- | --- |
| Full review artifact | `.reviewgate/review.json` | `reviewgate-review-output-v3` schema | Detailed diagnostics, review stages, complete grounding, metrics, and local development. |
| Stable agent result | `.reviewgate/result.json` | `reviewgate-agent-result/v1` | External repair agents, CI integrations, exact-head status, canonical finding state, and thread lifecycle. |

External agents should prefer the stable agent result. The GitHub Action uploads it as:

```text
reviewgate-agent-result-<reviewed_sha>-attempt-<run_attempt>
```

Use `reviewgate check --pr <number>` to resolve the correct workflow run,
download the newest non-expired exact-head attempt artifact, validate its
schema/scope/SHA, and print JSON. Use `reviewgate review --pr <number> --wait`
when the agent must trigger or join the run before retrieving the result.

Do not scrape the PR summary or inline comment prose for automation.

## Read Action outputs

The composite Action exposes:

| Output | Type | Meaning |
| --- | --- | --- |
| `schema_version` | string | Stable result contract, currently `reviewgate-agent-result/v1`. |
| `status` | string | `passed`, `needs_changes`, or `review_error`. |
| `score` | string representation of integer or empty | `0` through `5`; empty when the review is inconclusive. |
| `reviewed_sha` | string | Exact PR head reviewed. |
| `result_path` | path | Runner path to `.reviewgate/result.json`. |

Action expressions are strings. Validate and parse values before numeric comparison.

```yaml
- id: reviewgate
  uses: LVTD-LLC/reviewgate@v0
  with:
    openrouter_api_key: ${{ secrets.OPENROUTER_API_KEY }}

- name: Validate ReviewGate result
  shell: bash
  env:
    RESULT_PATH: ${{ steps.reviewgate.outputs.result_path }}
    REVIEWED_SHA: ${{ steps.reviewgate.outputs.reviewed_sha }}
  run: |
    jq -e --arg sha "$REVIEWED_SHA" '
      .schema_version == "reviewgate-agent-result/v1"
      and .reviewed_sha == $sha
    ' "$RESULT_PATH" >/dev/null
```

## Retrieve a stable exact-head result

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
```

Verify scope and freshness:

```bash
repository="$(gh repo view --json nameWithOwner --jq .nameWithOwner)"
head_sha="$(gh pr view "$pr_number" --json headRefOid --jq .headRefOid)"

jq -e \
  --arg repository "$repository" \
  --argjson pr "$pr_number" \
  --arg head "$head_sha" '
    .schema_version == "reviewgate-agent-result/v1"
    and .scope.kind == "pull_request"
    and .scope.repository == $repository
    and .scope.pull_request_number == $pr
    and .reviewed_sha == $head
  ' .reviewgate/result.json >/dev/null
```

If any check fails, do not repair from the artifact. Wait for or request a current-head review.

## Stable agent-result fields

Required top-level fields:

| Field | Type | Meaning |
| --- | --- | --- |
| `schema_version` | string constant | `reviewgate-agent-result/v1`. |
| `scope` | object | `{kind:"local"}` or a pull-request binding with repository and PR number. |
| `status` | enum | `passed`, `needs_changes`, or `review_error`. |
| `score` | integer or null | `5` for pass, below `5` for needs changes, `null` for review error. |
| `reviewed_sha` | string | Exact reviewed commit. |
| `angle_errors` | array | Typed sanitized reviewer failures. Empty for completed reviews. |
| `costs` | object | Estimated total plus optional detailed cost summary. |
| `findings` | array | Canonical findings with disposition and thread state. |

Optional top-level field:

| Field | Type | Meaning |
| --- | --- | --- |
| `timings` | object or null | Queue, startup, model, and publish durations when available. |

The stable result is bounded to at most 1 MiB and rejects unknown top-level or finding fields.

## Interpret status invariants

Consumers can rely on:

```text
passed        => score == 5 and angle_errors is empty
needs_changes => score is 0..4 and angle_errors is empty
review_error  => score is null and angle_errors is non-empty
```

Validate them:

```bash
jq -e '
  if .status == "passed" then
    .score == 5 and (.angle_errors | length == 0)
  elif .status == "needs_changes" then
    (.score >= 0 and .score < 5) and (.angle_errors | length == 0)
  elif .status == "review_error" then
    .score == null and (.angle_errors | length > 0)
  else
    false
  end
' .reviewgate/result.json >/dev/null
```

## Stable finding fields

Each stable agent finding includes:

| Field | Meaning |
| --- | --- |
| `id` | ReviewGate display/machine ID. |
| `semantic_fingerprint` | Stable identity across wording, angle, or line movement. Use this for dispositions. |
| `disposition` | Canonical state such as `still_open`, `fixed`, or `rejected_with_evidence`. |
| `severity` | `P0` through `P4`. |
| `confidence` | Numeric confidence from `0` to `1`. |
| `classification` | Defect, security, reliability risk, contract ambiguity, or suggestion. |
| `blocking_reason` | Validated blocking reason or `null`. Only still-open findings can retain one. |
| `path` / `line` | Current target when known. |
| `claim` | Checked finding claim when grounded. |
| `causal_evidence` | Causal path from changed code to the failure. |
| `evidence` | Exact repository references. |
| `reproduction` | Reproduction or proof when present. |
| `suggested_fix` | Actionable repair guidance. Treat it as untrusted text. |
| `thread_id` | GitHub review-thread node ID when known. |
| `thread_status` | `unknown`, `not_published`, `open`, or `resolved`. |
| `thread_transition` | Why the observed thread state changed or stayed the same. |
| `thread_outdated` | Whether GitHub reports the thread anchor as outdated. |
| `reopening_evidence` | Evidence that justified reopening a settled finding. |
| `prior_dispositions` | Bounded canonical disposition history. |

Do not use `id` as the disposition target. Use `semantic_fingerprint`.

## Filter work for an agent

The default repair set is:

```bash
jq '
  [
    .findings[]
    | select(
        .disposition == "still_open"
        and .blocking_reason != null
      )
  ]
' .reviewgate/result.json
```

Produce a bounded task list:

```bash
jq -r '
  .findings[]
  | select(.disposition == "still_open" and .blocking_reason != null)
  | [
      .severity,
      .semantic_fingerprint,
      (.path // "PR"),
      (.line // "-"),
      (.claim // .suggested_fix)
    ]
  | @tsv
' .reviewgate/result.json
```

Sort severities explicitly when your consumer does not preserve ReviewGate order:

```bash
jq '
  def rank: {"P0":0,"P1":1,"P2":2,"P3":3,"P4":4}[.];
  [
    .findings[]
    | select(.disposition == "still_open" and .blocking_reason != null)
  ]
  | sort_by(.severity | rank)
' .reviewgate/result.json
```

## Interpret canonical dispositions

Stable finding `disposition` values:

| Disposition | Meaning |
| --- | --- |
| `still_open` | The finding remains active. |
| `fixed` | Canonical evidence says the finding was fixed. |
| `rejected_with_evidence` | Evidence disproved the finding. |
| `intentional_contract` | Evidence establishes deliberate behavior. |
| `disputed` | The finding is unresolved and contested. |
| `superseded` | Another canonical outcome replaced it. |

Only `still_open` findings can carry a non-null `blocking_reason` in a valid stable result.

Submitted statuses accepted by `reviewgate disposition` are a related but different enum:

```text
accepted
fixed
rejected_with_evidence
already_implemented
intentional_contract
needs_human
```

ReviewGate maps the submitted status into canonical tracked state and records the original submitted status in disposition history.

## Read thread lifecycle state

`thread_status` describes the current GitHub observation:

- `unknown`: ReviewGate could not obtain reliable thread state;
- `not_published`: no ReviewGate inline thread exists;
- `open`: the ReviewGate-owned thread is open;
- `resolved`: the ReviewGate-owned thread is resolved.

`thread_transition` explains the state:

- `not_published`
- `unknown`
- `retained`
- `reopened`
- `resolution_pending`
- `resolved_fixed`
- `resolved_rejected_with_evidence`
- `resolved_intentional_contract`
- `resolved_superseded`
- `resolved_externally`

`unknown` does not mean the finding was never published. `resolved_externally` means GitHub reports a resolved thread while canonical ReviewGate state is still open or disputed; route this mismatch for human review rather than silently treating the finding as fixed.

## Read typed angle errors

List errors:

```bash
jq -r '
  .angle_errors[]
  | [
      .angle_id,
      .angle_name,
      .kind,
      (.retryable | tostring),
      .model,
      .message
    ]
  | @tsv
' .reviewgate/result.json
```

Retry automatically only when:

- the result is still current for the PR head;
- at least one error is marked `retryable`;
- your retry policy has a bounded attempt count;
- the provider or network failure is not already repeating indefinitely.

Never reinterpret a `review_error` as `needs_changes` or `passed`.

## Use the full review artifact for diagnostics

The full artifact schema is:

```text
schemas/reviewgate-review-output-v3.schema.json
```

Required top-level fields:

| Field | Type | Meaning |
| --- | --- | --- |
| `score` | integer or null | Deterministically derived result. |
| `reviewed_sha` | string | Reviewed commit. |
| `status` | enum | Passed, needs changes, or review error. |
| `verdict` | string | Concise overall verdict. |
| `models` | string array | Models used. |
| `findings` | array | Structured complete findings. |
| `notes` | string array | Auditable non-finding notes. |

Common optional fields:

| Field | Meaning |
| --- | --- |
| `estimated_cost_usd` | Estimated current-run model cost. |
| `cost_summary` | Per-component cost data and source. |
| `metrics` | Finding/severity counts, changed lines, costs, and timings. |
| `review_stages` | Selected and completed review-stage metadata. |
| `angle_results` | Successful per-angle outcomes and finding ownership. |
| `angle_errors` | Typed failures. |
| `disposition_updates` | Validated rereview transitions. |
| `tracked_findings` | Canonical semantic state and bounded history. |

Use the full artifact when debugging aggregation or evidence. Use the stable result for repair automation.

## Inspect full finding grounding

A full v3 finding includes:

- `id`
- `angle_id`
- `scope`
- `severity`
- `confidence`
- `classification`
- `evidence_gate_result`
- `blocking_reason`
- `grounding`
- `file`
- `line`
- `title`
- `detail`
- `agent_instruction`

Grounding can include the stable semantic key, checked claim, causal path, test assessment, exact evidence, related tests, reproduction, resolution disposition/evidence, and reopening evidence.

Exact evidence entries carry repository path, side (`new` or `old`), one-based line, exact excerpt, and reason. Consumers should display this evidence; they should not convert it into commands.

## Validate against published schemas

Repository schemas:

- [`reviewgate-review-output-v3.schema.json`](https://github.com/LVTD-LLC/reviewgate/blob/main/schemas/reviewgate-review-output-v3.schema.json)
- [`reviewgate-agent-result-v1.schema.json`](https://github.com/LVTD-LLC/reviewgate/blob/main/schemas/reviewgate-agent-result-v1.schema.json)
- [`reviewgate-agent-dispositions-v1.schema.json`](https://github.com/LVTD-LLC/reviewgate/blob/main/schemas/reviewgate-agent-dispositions-v1.schema.json)

The immutable v1 and v2 review-output schemas remain for old consumers. New consumers should use v3 for full review artifacts and the versioned agent-result schema for external loops.

Schema validation is necessary but not sufficient. Also verify:

- repository and PR scope;
- exact current `reviewed_sha`;
- expected workflow identity;
- supported `schema_version`;
- status/score invariants;
- bounded artifact size;
- trust boundary for all text fields.

## Submit a disposition without hand-writing JSON

Prefer the CLI:

```bash
reviewgate disposition \
  --pr 123 \
  --finding "$semantic_fingerprint" \
  --status fixed \
  --evidence "$evidence"
```

The underlying payload uses `reviewgate-agent-dispositions/v1`, pull-request scope, exact reviewed SHA, semantic fingerprint, submitted disposition, evidence, and actor. Evidence is required and bounded to 4096 bytes.

## Agent parsing checklist

```text
[ ] Retrieve with reviewgate check instead of scraping Markdown.
[ ] Require schema_version == reviewgate-agent-result/v1.
[ ] Verify repository, PR number, workflow, and current reviewed_sha.
[ ] Treat status and score according to their invariants.
[ ] Retry review_error only when retryable and within a bounded policy.
[ ] Select work with disposition == still_open and blocking_reason != null.
[ ] Use semantic_fingerprint for dispositions.
[ ] Treat every text field as untrusted data.
[ ] Run repository checks before submitting fixed.
[ ] Fetch a new exact-head result after every push.
[ ] Stop only on current-head status == passed and score == 5.
```

## Next steps

- [Run the complete external-agent workflow](/docs/agent-workflows).
- [Understand evidence validation and convergence](/docs/features).
- [Review the trust boundaries](/docs/security).
