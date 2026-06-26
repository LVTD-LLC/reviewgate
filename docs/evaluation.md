# Shipcheck Evaluation

Shipcheck can evaluate committed review artifacts without publishing comments:

```bash
cargo run --locked -p shipcheck-cli -- eval-fixtures --dir fixtures
```

The command prints JSON with fixture count, average score, finding counts, blocking counts, and estimated cost totals. This is the dry-run surface for historical PR validation: generate artifacts for candidate PRs, store them outside the live PR workflow, then compare score stability, useful finding rate, false positives, latency, and cost.

For v0.1.0 readiness, use at least 20 historical PR artifacts before Marketplace publishing.
