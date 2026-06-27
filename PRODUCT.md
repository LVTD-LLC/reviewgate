# Product Context

ReviewGate is a free, fully open-source credibility project. The initial goal is adoption and trust among AI builders, open-source maintainers, solo builders, and small teams, not paid conversion.

## Core User Story

1. A maintainer installs the GitHub Action.
2. They add `OPENROUTER_API_KEY` to GitHub Actions secrets.
3. They optionally add `.reviewgate.yml`.
4. A PR opens or updates.
5. ReviewGate posts one top-level summary comment with a clear `0-5` score.
6. ReviewGate emits structured JSON for humans or external agent loops.
7. ReviewGate posts inline comments only for specific, high-confidence issues.
8. A human or external agent fixes findings and pushes again until the review reaches the target score, ideally `5/5`.

## Durable Product Constraints

- Free and fully open source in v0.
- Standalone public tool, not a private wrapper around internal review skills.
- GitHub Actions-first distribution.
- OpenRouter/BYOK model access.
- Every review must produce a score and summary.
- The score must be visually obvious in the PR summary.
- The summary must be canonical: create it once, then update the same comment on later runs.
- Machine-readable JSON is part of the product contract, not a debug artifact.
- Full-repo indexing is not required for v1; start with diff, changed files, nearby context, and instruction files.
- Default model presets should optimize for price-to-value.

## Positioning

Do not position ReviewGate as a generic open-source PR reviewer. The sharper wedge is score-gated review for agent-written PRs: score, rubric, canonical summary, check-run status, machine-readable findings, and agent-loop compatibility.

## Non-Goals For v0

- Hosted GitHub App.
- Billing, subscriptions, or usage metering.
- Org-wide dashboards or cross-repo analytics.
- Full-codebase indexing service.
- Broad agent workflows such as issue triage or CI remediation.
- Storing repository code or model outputs outside the user's CI environment.

## Success Criteria

- A user can install the action with one workflow file and one OpenRouter secret.
- PRs get a stable summary comment that updates in place.
- Humans can quickly understand merge readiness from the `0-5` score.
- Agents can consume JSON artifacts and loop on blocking findings outside the GitHub Action.
- The tool feels reliable enough for public open-source maintainers to try without a hosted account.
