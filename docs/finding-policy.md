# Finding policy

ReviewGate treats classification, severity, confidence, evidence, and blocking disposition as separate values. Model output proposes the first three; deterministic Rust policy owns the final disposition.

## Classification

| Classification | Meaning | Can block? |
| --- | --- | --- |
| `defect` | Incorrect product or code behavior | Yes |
| `security` | Exploitable or trust-boundary failure | Yes |
| `reliability_risk` | Concrete failure, availability, or operational risk | Yes |
| `contract_ambiguity` | The applicable contract is unclear or contradictory | No |
| `suggestion` | Optional improvement or preference | No |

## Blocking matrix

A finding blocks only when every condition is true:

1. Classification is `defect`, `security`, or `reliability_risk`.
2. Severity is `P0`, `P1`, `P2`, or `P3`.
3. Confidence is at least `0.85`.
4. The repository evidence gate returns `passed`.

`P4`, `contract_ambiguity`, and `suggestion` findings are advisory. A potentially blocking finding below the confidence threshold becomes an auditable note. A potentially blocking finding that fails repository-evidence validation is suppressed and recorded in notes. P0-P1 evidence still requires a reproduction or exceptionally strong proof.

ReviewGate recalibrates `blocking_reason` from the checked fields; model-provided prose or severity cannot directly set the GitHub check outcome.

## Structured fields

Each serialized finding includes:

- `classification`
- `severity`
- `confidence`
- `evidence_gate_result`
- `blocking_reason`

`blocking_reason` is `validated_defect`, `validated_security`, or `validated_reliability_risk` only for a blocker. It is `null` for advisory findings.

## Outcome projection

| Artifact status | Score | ReviewGate check |
| --- | --- | --- |
| `passed` | `5` | `success` |
| `needs_changes` | `0..4`, derived from validated blockers | `failure` |
| `review_error` | `null` | `failure`, labeled inconclusive |

The workflow job can still complete successfully after publishing a `needs_changes` result. Repositories that make the ReviewGate check required can use its failing conclusion as the merge gate.
