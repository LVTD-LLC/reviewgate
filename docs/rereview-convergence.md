# Rereview convergence

ReviewGate treats a rereview as a state transition over one pull request, not as an unrelated fresh opinion.

## Identity and state

The canonical summary carries a base64url-encoded version 2 state record. ReviewGate accepts that state only from the bot-owned canonical comment and only when its repository and pull-request binding matches the current event. Every tracked finding has:

- an explicit finding ID for the current review;
- a semantic fingerprint derived from classification, file, and `grounding.semantic_key`;
- a SHA-256 code fingerprint derived from its checked causal path and repository evidence;
- a current disposition and a bounded chronological history of disposition records with actor, evidence summary, reviewed SHA, and code fingerprint.

The state format is intentionally a clean break. Invalid or older canonical state fails the review closed instead of silently starting a new convergence history. State is capped at 128 tracked findings, eight disposition records per finding, and 32 KiB before encoding so GitHub comment publication cannot grow without bound.

## Dispositions

Each tracked finding is exactly one of:

| Disposition | Meaning |
| --- | --- |
| `still_open` | The validated issue remains actionable. |
| `fixed` | A repair agent or maintainer explicitly verified the fix against the new head and recorded evidence. |
| `rejected_with_evidence` | Repository or platform evidence disproved the claim. |
| `intentional_contract` | The behavior is an explicit product or repository contract. |
| `disputed` | A human decision is still required. |
| `superseded` | A newer finding or contract replaced this identity. |

Still-open findings remain present even if a later model pass omits or rewrites them. Reviewer silence and unrelated same-file edits are never fix evidence. The reviewer must either emit the equivalent finding again or emit the same semantic identity with `resolution_disposition: fixed` and a non-empty `resolution_evidence_summary`. ReviewGate records an automatic fixed transition only when the delta deletes every prior current-head evidence location and the resolution checks every added line in each non-empty contiguous replacement block. Pure deletions remain open for explicit disposition because a separate added line cannot prove that the underlying path is gone. Grounded disposition updates and their resolution candidates travel in the review artifact; summary publication revalidates those candidates against the repository and delta before applying the exact same convergence result. Findings grounded only in previously deleted lines remain open for an explicit disposition; partial evidence, a partial block, or a different reviewer-authored fingerprint is insufficient.

Fixed, rejected, intentional, disputed, and superseded identities remain suppressed unless the relevant code or external contract changed, the code fingerprint changed, and the new finding supplies specific `reopening_evidence`.

## Late findings

After the first validated review, a new blocker must have confidence of at least `0.95` and include `novelty_evidence` explaining why the issue did not exist or could not be detected at the prior reviewed SHA. Otherwise ReviewGate suppresses it with an audit note. Advisory findings do not use the blocking novelty threshold.

The review prompt receives only validated prior disposition data and the Git delta since the latest completed `last_valid_reviewed_sha`. An inconclusive `review_error` does not advance that convergence base. Prior state, repository text, and model output remain untrusted context.

## Idempotence

An unchanged head reuses the prior validated finding set and ignores reviewer drift. The maintainer `@reviewgate review` command is a no-op when the canonical state already contains a completed review for that exact head. A prior `review_error` can still be retried because it is not a completed code-quality outcome.
