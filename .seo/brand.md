# ReviewGate — Brand Context for SEO

> Read every time. This file is read by every phase of the SEO sprint.

## Product

- **Name:** ReviewGate
- **One-liner:** Open-source AI review gates for agent-written pull requests.
- **What we do:** ReviewGate runs as a GitHub Action in the user's CI, reviews an agent-written pull request through OpenRouter, posts one canonical PR summary with a visible 0–5 score, publishes structured findings, and emits JSON that humans or external agents can use for a repair loop.
- **Pricing structure:** Free and fully open source. Users bring their own OpenRouter API key and pay their model provider directly.
- **Free tier?** Yes — the product itself is free; model usage is BYOK.

## Audience

- **Primary persona:** Open-source maintainers and small engineering teams accepting pull requests from coding agents.
- **Secondary personas:** Solo builders, AI-native software teams, platform engineers, and maintainers experimenting with agent repair loops.
- **Industries we target:** Developer tools and software engineering teams across industries.
- **Company size we target:** Solo maintainers through small and midsize engineering teams.
- **Jobs to be done:**
  1. Decide whether an agent-written pull request is ready for human review or merge.
  2. Catch concrete correctness and security risks without adding noisy review threads.
  3. Give humans and repair agents a stable, machine-readable list of what to fix next.

## Competitors

| Brand | Slug | URL | Tier | Notes |
|---|---|---|---|---|
| CodeRabbit | `coderabbit` | https://www.coderabbit.ai/ | head | Category leader with polished hosted PR-review ergonomics. |
| Greptile | `greptile` | https://www.greptile.com/ | head | Hosted AI code review emphasizing whole-codebase context. |
| Qodo | `qodo` | https://www.qodo.ai/products/qodo-merge/ | head | Broader code-quality platform; PR-Agent is its open-source predecessor. |
| PR-Agent | `pr-agent` | https://github.com/qodo-ai/pr-agent | mid | Original open-source PR reviewer and a closer deployment-model comparison. |
| Sourcery | `sourcery` | https://sourcery.ai/ | mid | Automated review and coding-assistance product. |
| Reviewpad | `reviewpad` | https://reviewpad.com/ | niche | Policy-oriented pull request automation and review workflows. |

## Brand voice

- **Voice tags:** Technical, specific, trust-first, sober, concise, open-source-native.
- **Person/perspective:** “We” for product decisions; “you” for setup and outcomes.
- **Forbidden words/phrases:** Revolutionary, seamless, magical, guaranteed, perfect, replace human reviewers.
- **Reference brands for tone:** The existing ReviewGate README and product page: explicit constraints, concrete behavior, no hype.

## Anti-positioning

1. No hosted GitHub App or ReviewGate account.
2. No billing, subscriptions, or usage metering.
3. No persistent storage of repository code or model output outside the user's CI environment.
4. No full-repository indexing service in v0.
5. No automatic code repair or mutation of pull requests.
6. No claim to replace the human merge decision.
7. No org-wide dashboard or cross-repository analytics.

## Concrete differentiators

1. A fixed, visually obvious 0–5 merge-readiness score with a deterministic 5/5 passing target.
2. One canonical PR summary comment that updates in place instead of creating duplicate review threads.
3. Structured `.reviewgate/review.json` output designed for external agent repair loops.
4. GitHub Actions-first, BYOK, review-only operation that can be inspected before it reviews anything.

## Visual brand

- **Accent color:** `oklch(52% 0.19 252)`
- **Accent dark:** `oklch(38% 0.18 252)`
- **Ink color:** `oklch(18.2% 0.035 252)`
- **Surface color:** `oklch(100% 0 0)`
- **Hero font family:** System sans-serif stack
- **Body font family:** System sans-serif stack
- **Icon set:** Custom favicon and text-first UI; no external icon library.

## Links to existing surfaces

- Homepage: https://reviewgate.lvtd.dev/
- Install section: https://reviewgate.lvtd.dev/#install
- Configuration section: https://reviewgate.lvtd.dev/#configure
- Purpose section: https://reviewgate.lvtd.dev/#purpose
- Source and full documentation: https://github.com/LVTD-LLC/reviewgate
- Pricing: Not applicable; ReviewGate is free and open source.
- Blog: None yet.
