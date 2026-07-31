---
layout: ../../layouts/DocsLayout.astro
title: "ReviewGate features and scoring"
description: "Understand ReviewGate review angles, evidence validation, 0-5 scoring, canonical summaries, inline threads, and exact-head rereviews."
heading: "Understand ReviewGate features and scoring"
lede: "Follow the review lifecycle from bounded pull request context through evidence-grounded findings, deterministic scoring, canonical GitHub output, and current-head convergence."
eyebrow: "CONCEPTS / REVIEW LIFECYCLE"
---

## ReviewGate's product boundary

ReviewGate is a review-only, score-centered gate for agent-written pull requests. It:

- reviews a bounded PR diff and repository context;
- calls user-funded models through OpenRouter;
- validates model output before it can affect the score;
- publishes a visible `0-5` result;
- maintains one canonical PR summary;
- emits structured JSON for humans and external coding agents.

ReviewGate does not repair code, execute PR code, merge the pull request, host a review service, store repository code outside the user's CI environment, or replace the human merge decision.

## Follow a live review from trigger to result

1. A same-repository pull request opens, updates, reopens, or becomes ready for review.
2. The workflow checks out full Git history without persisting checkout credentials.
3. ReviewGate verifies and starts its version-pinned runtime.
4. A short running placeholder is created or updated.
5. The CLI collects the exact reviewed SHA, changed files, diff, PR title/body, instruction files, changed-file contents within bounds, sibling tests, and referenced reusable workflows.
6. The PR title and body are labeled as untrusted scope context, not instructions.
7. Each enabled review angle receives the bounded context and calls OpenRouter.
8. Model output is parsed into strict ReviewGate JSON.
9. Potential blockers pass deterministic policy and repository-evidence validation.
10. If independent blocker verification is enabled and candidates exist, one batched verifier call checks every normalized claim.
11. Successful angle results and typed angle errors are aggregated.
12. ReviewGate recomputes the top-level score and status.
13. JSON and Markdown artifacts are written.
14. Eligible findings are published inline when GitHub has a valid anchor.
15. The canonical summary is updated in place.
16. A dedicated check run reports `passed`, `needs_changes`, or unavailable review.
17. A stable agent result is projected and uploaded for the exact reviewed SHA.

The model proposes findings. ReviewGate owns validation, scoring, status, and publication invariants.

## Understand the bounded review context

ReviewGate starts from the pull request rather than indexing the complete repository. The context can include:

- exact PR head SHA;
- PR title and body;
- changed-file list;
- unified diff;
- number of changed lines;
- common instruction and context files such as `AGENTS.md`, `README.md`, `TECH.md`, `PRODUCT.md`, `STRUCTURE.md`, and `.reviewgate.yml`;
- complete contents of changed text files within explicit file-count and aggregate-byte limits;
- bounded sibling tests;
- referenced local reusable workflows.

Changed-file contents fail closed when the explicit limits are exceeded instead of being silently truncated or omitted. Full-repository indexing is outside the v0 product boundary.

## Use review angles for independent perspectives

The default review contains:

- `general`: broad correctness and regression review;
- `adversarial`: deliberate bug-finding and failure-scenario review.

Repositories can replace the list with inline prompts, prompt files, or repo-local skill instructions. Each angle produces its own model identity, verdict, findings, derived score, status, and cost data.

An angle failure is not converted into `0/5`. It becomes a typed `angle_errors` entry and makes the whole review inconclusive.

See [Configure custom review angles](/docs/configuration#add-custom-review-angles).

## Optionally verify blockers with an independent call

Repositories can enable a second, independent inference pass for blocker
candidates. This is off by default. It makes no call when the initial review
has no grounded blockers and batches all remaining candidates into at most one
call. The verifier can use the primary model or a separately configured model.

Only normalized claims and checked repository evidence cross this boundary;
the discovery model's persuasive title, prose detail, and repair instruction
do not. Verified candidates retain their normal blocking behavior. Rejected
candidates stay auditable in the full artifact but are not comments or open
agent obligations. A later rejection cannot silently clear an already verified
open obligation: ReviewGate retains the obligation until convergence approves
resolution evidence and stores the disagreement in
`verification.conflicting_decisions`. Inconclusive or malformed verification
fails closed as `review_error`.

See [Independently verify blocker candidates](/docs/configuration#independently-verify-blocker-candidates).

## Separate finding dimensions

Every structured finding separates dimensions that models often conflate:

| Dimension | Purpose |
| --- | --- |
| `classification` | `defect`, `security`, `reliability_risk`, `contract_ambiguity`, or `suggestion`. |
| `severity` | Impact level from `P0` through `P4`. |
| `confidence` | Numeric confidence from `0` through `1`. |
| `evidence_gate_result` | Whether deterministic grounding requirements passed. |
| `blocking_reason` | Auditable reason the finding affects the score, or `null`. |
| `scope` | Whether the finding targets a line, file, or the PR. |
| `grounding` | Checked claim, causal path, exact evidence, tests, and reproduction data. |

Classification is not severity. Confidence is not severity. A suggestion does not become blocking merely because it is phrased strongly.

## Know which findings can block

A finding can affect the score only when all applicable policy checks pass:

- classification is `defect`, `security`, or `reliability_risk`;
- severity is `P0`, `P1`, `P2`, or `P3`;
- confidence is at least `0.85`;
- the evidence gate passes;
- a deterministic `blocking_reason` is assigned.

New blockers introduced after the initial review have a stricter `0.95` confidence requirement and need novelty evidence. This reduces late-review churn from weak new claims.

`contract_ambiguity`, `suggestion`, `P4`, low-confidence, and evidence-rejected findings are advisory. They remain auditable but do not lower the score.

For agent automation, the reliable score-blocking predicate is:

```text
.disposition == "still_open" and .blocking_reason != null
```

Do not infer blocking state from severity alone.

## Understand evidence validation

Before a high-confidence `P0-P3` claim can block, its grounding must include:

- a stable semantic key for the root cause;
- one concise checked claim;
- a causal path from the changed line to the user-visible failure;
- exact repository evidence with path, side, one-based line, excerpt, and reason;
- at least one evidence reference to a changed diff line;
- related tests that exercise the alleged path;
- a reproduction or exceptional proof for `P0-P1`.

Evidence with `side: new` must match the checked-out head. Evidence with `side: old` must match a deleted diff line. Platform-contract contradictions that ReviewGate can check are turned into auditable non-blocking notes rather than unsupported blockers.

The evidence gate is read-only. It does not execute PR code or model-proposed commands.

## Compute the fixed score

The passing target is fixed at `5/5`.

| Validated blocker severity | Maximum score |
| --- | ---: |
| `P0` | `1/5` |
| `P1` | `2/5` |
| `P2` | `3/5` |
| `P3` | `4/5` |
| No validated blocker, including advisory `P4` | `5/5` |

The top-level score is the minimum ceiling across all validated still-open blockers. If no blocker exists, the score is `5`.

Examples:

- one validated `P3` and three advisory `P4` findings → `4/5`;
- one validated `P1` and one validated `P3` → `2/5`;
- two suggestions and one evidence-rejected `P2` → `5/5`;
- reviewer timeout → no numeric score, `review_error`.

A completed review with no score-affecting findings cannot report `0/5`.

## Interpret status correctly

| Status | Score | Meaning |
| --- | --- | --- |
| `passed` | `5` | Completed review with no validated still-open blocker. |
| `needs_changes` | `0` through `4` | Completed review with at least one validated blocker. |
| `review_error` | `null` | Review was inconclusive because an angle, provider, transport, parsing, or artifact validation path failed. |

`review_error` is not a claim about code quality. Retry it when `angle_errors[].retryable` is true; investigate non-retryable validation errors before trusting another result.

## Distinguish workflow, check, and review results

ReviewGate separates infrastructure execution from review outcome:

- a completed low score is a valid review, so the review process itself need not crash;
- the `ReviewGate` check run reports failure for `needs_changes`;
- `review_error` also reports a failing check because no current usable review exists;
- required publishing failures make the Action fail visibly;
- best-effort inline publication can fail while the full finding remains in JSON.

Branch protection should use the `ReviewGate` check or stable JSON result, not assume a green workflow job means `5/5`.

## Keep one canonical summary

The top-level PR summary contains:

```html
<!-- reviewgate-summary -->
```

ReviewGate creates it once and updates it on later runs. It ignores user-authored marker-shaped comments and deletes only bot-authored duplicate canonical candidates.

The comment also contains validated hidden state:

```html
<!-- reviewgate-state ... -->
```

The state tracks repository/PR identity, reviewed SHAs, the latest valid score, run count, cumulative cost, bounded cost history, and canonical finding dispositions. Untrusted text cannot become trusted state merely by resembling the marker.

If a rerun is inconclusive, the visible summary preserves and labels the latest valid score instead of presenting the provider failure as `0/5`.

## Publish findings inline without losing unanchored evidence

For findings at or above `min_severity`, ReviewGate attempts inline publication:

1. use the exact changed right-side line when valid;
2. repair a stale line when changed-line text matches;
3. use another right-side diff line in the same file for file-level or unanchored findings;
4. use a right-side line elsewhere in the PR for PR-level findings;
5. keep the complete finding in JSON when no anchor exists or GitHub rejects the payload.

ReviewGate does not create standalone top-level comments for findings that cannot be anchored.

`min_severity` is a publication filter, not a score threshold. JSON remains the complete contract.

## Track one semantic finding across rereviews

Inline comments include hidden ReviewGate markers and semantic fingerprints. Equivalent findings can keep one thread even when:

- wording changes;
- another angle owns the finding;
- a line moves;
- the best available anchor changes.

After the canonical summary applies current-head dispositions, ReviewGate reconciles only bot-owned threads carrying valid ReviewGate markers:

- still-open and disputed findings keep their thread;
- fixed, rejected-with-evidence, intentional-contract, and superseded findings receive one lifecycle reply and resolve idempotently;
- justified recurrences can reopen with explicit evidence;
- human-authored comments are never deleted.

The stable agent result exposes `thread_status`, `thread_transition`, `thread_outdated`, `thread_id`, and `reopening_evidence`.

## Request an exact maintainer rereview

The supported command is exactly:

```text
@reviewgate review
```

The entire comment must match. ReviewGate verifies:

- `issue_comment.created` on an open PR;
- author association and live write-level repository permission;
- base repository and PR identity;
- exact current `head.sha`;
- configured workflow identity;
- newest completed eligible `pull_request` run;
- duplicate event delivery;
- whether the current head was already fully reviewed.

It never chooses a run by branch name alone. A stale SHA, foreign repository, other PR, non-PR event, or in-progress run is ineligible.

On a new head, the model receives the delta since the latest valid reviewed SHA plus bounded canonical disposition state. An unchanged completed head is an idempotent no-op.

## Converge findings without silent disappearance

ReviewGate does not let a previously still-open blocker disappear merely because a later model omits it or lowers its confidence.

Automatic `fixed` requires current-delta evidence that removes every prior current-head evidence location and checks complete replacement blocks. Pure deletions and findings grounded only in lines deleted before the current head stay open until an explicit disposition can establish the outcome.

A finding marked `rejected_with_evidence` or `intentional_contract` stays suppressed unless changed code or contract evidence justifies reopening it.

External agents can submit supported dispositions with `reviewgate disposition`. ReviewGate binds them to the exact repository, PR, reviewed SHA, semantic fingerprint, authenticated actor, evidence, and writer attestation. During replay, it accepts either the exact writer-only status receipt or a fresh repository-write permission check; if neither verification path is available, the review fails operationally instead of dropping the disposition.

## Report cost and runtime without hosted telemetry

Artifacts can include:

- current-run estimated cost;
- per-component cost entries;
- cost source;
- cumulative cost in canonical summary state;
- queue, startup, model, and publish timings.

ReviewGate uses OpenRouter BYOK. It does not provide hosted accounts, billing, persistent storage, or product telemetry. Provider usage and GitHub Actions logs remain in the user's own provider and repository contexts.

## Next steps

- [Read the stable JSON contracts](/docs/artifacts).
- [Implement a current-head agent loop](/docs/agent-workflows).
- [Review the security model](/docs/security).
