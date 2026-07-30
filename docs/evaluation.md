# ReviewGate Evaluation

ReviewGate has two non-publishing evaluation surfaces:

```bash
# Aggregate already-produced review artifacts.
cargo run --locked -p reviewgate-cli -- eval-fixtures --dir fixtures

# Replay the blinded replacement benchmark through its configured pipelines.
cargo run --locked -p reviewgate-cli -- eval-replays \
  --manifest fixtures/evaluation/manifest-v1.json \
  --json-out .reviewgate/benchmark.json \
  --markdown-out .reviewgate/benchmark.md
```

`eval-fixtures` remains useful for artifact inventory and score/cost summaries.
`eval-replays` is the rollout gate for reviewer-intelligence changes. It never
publishes comments, checks, reviews, dispositions, or workflow reruns.

## Corpus contract

The versioned manifest is
`reviewgate-benchmark-manifest/v1`; reports use
`reviewgate-benchmark-report/v1`. Their JSON Schemas live under `schemas/`.
Manifest parsing rejects unknown fields, unblinded corpora, fewer than 30
cases, fewer than two repetitions, duplicate configuration/source IDs,
more than ten repetitions, escaping source paths, invalid thresholds, and
anything other than exactly one baseline and one candidate.

The committed corpus currently contains 44 cases:

- 41 historical/seeded evidence-grounding regressions covering Go, Python,
  Rust, YAML, shell, workflow, CLI, security, reliability, and release risks;
- three public ReviewGate PR #53 defects captured from the vulnerable range
  `f39293e40859a778a0c25ebfcd64e7fedbe3c058` to
  `0d0ba08e4f7211f1d10141bb8cd7b362bf77d934`;
- the independently verified fix commit
  `cfcd1190722e632cca6ea2f39f0bcc976c9e155b` and public discussion URLs as
  provenance.

The PR #53 expectations are normalized claims and semantic keys, not copied
reviewer prose. Greptile was invoked on that PR, but no Greptile-authored result
is publicly available. The fixture therefore records
`invoked_no_public_result`; it does not treat Greptile as clean, as a miss, or
as the source of CodeRabbit findings.

Expected outcomes are adjudication data only. They are never added to model
prompts. Exact semantic keys determine matches: wording similarity cannot turn
a miss into a hit. A drifted key is both a missed expected defect and an
unexpected finding. Duplicate blockers can match an expected defect once; the
remaining duplicates count as false blockers.

## Metrics

Each configuration report includes:

- **blocking precision:** true blocking findings divided by all observed
  blockers;
- **serious-defect recall:** detected serious expected keys divided by all
  serious expected keys;
- **false blockers per case:** unexpected or contradicted blockers divided by
  corpus cases;
- **contradiction rate:** adjudicated known non-findings incorrectly reported
  as blocking;
- **completion rate:** cases whose every required repetition completed without
  a reviewer failure;
- **rereview stability:** cases whose semantic-key/blocking set is identical
  across repetitions;
- **rereview convergence:** cases whose final repetition agrees with the
  adjudicated blocking outcome;
- estimated and provider-reported cost, mean latency, and agent-time coverage.

Unavailable denominators or measurements serialize as `null`, not invented
zeroes. Partial/review-error runs remain in the recall denominator, so provider
or parser failures cannot make a configuration appear clean. Deterministic
replays use recorded responses and report zero model cost/latency; missing
agent-time observations remain `null` with zero coverage.

The report contains the baseline, candidate, signed deltas, per-case outcomes,
and every threshold decision in one stable JSON document. Deterministic runs
are byte-stable and execute in CI without an OpenRouter key or network call.

## Live BYOK benchmark

Live mode is always explicit:

```bash
OPENROUTER_API_KEY=... cargo run --locked -p reviewgate-cli -- eval-replays \
  --manifest fixtures/evaluation/manifest-v1.json \
  --live \
  --model openai/gpt-5.1-codex-mini \
  --json-out .reviewgate/live-benchmark.json \
  --markdown-out .reviewgate/live-benchmark.md
```

Before the first request, ReviewGate prints the chosen model and the manifest's
cost and latency budgets, resolves model pricing once, and rejects runs above
100 model requests. Each captured case is materialized in an isolated
temporary directory. The model sees only the captured diff/files and normal
review instructions—not expected keys or adjudication. The same model response
feeds both configurations: the baseline scores raw calibrated findings, while
the candidate passes them through the normal evidence gate. Failures produce
incomplete runs and recall misses rather than being dropped. Before each model
request, the evaluator stops if cumulative provider-reported spend (or the
pricing estimate when spend is unavailable) has reached
`maximum_live_cost_usd`. Charged responses retain their cost even when their
model artifact is malformed.

Use `--max-cases <n>` for exploratory live runs. A subset smaller than the
manifest minimum still writes reports but fails the replacement gate. Live
mode requires `OPENROUTER_API_KEY`; deterministic mode ignores the key and
OpenRouter URL.

The evaluator does not execute captured code, invoke `gh`, write inside source
repositories, or call any GitHub publishing function. API keys, headers, model
responses, and private source are never committed by the benchmark command.

## Rollout and rollback

The committed gate requires:

- blocking precision at least `0.95`;
- serious-defect recall at least `0.95`;
- no blocking-precision regression from baseline;
- false blockers per case at most `0.05`;
- contradiction rate at most `0.05`;
- rereview stability of `1.0`;
- completion rate of `1.0`;
- total live cost at most `$10`;
- mean live latency at most `300000ms`.

Keep prompt-only behavior as the default until a candidate passes the complete
deterministic corpus and an authorized full live run. Roll back or keep a new
mode opt-in when any threshold fails, completion coverage falls, private or
proprietary data provenance is unclear, or live cost/latency exceeds budget.
Freeze and independently adjudicate new cases before using them to tune a
candidate.
