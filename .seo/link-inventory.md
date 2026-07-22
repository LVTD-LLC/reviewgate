# ReviewGate — Internal Link Inventory

> Every SEO sprint phase must choose valid links from this inventory and update it when new pages ship.

## Existing pages and anchors

### Homepage and core marketing

| Slug | URL | Title / anchor-text candidate | Used by patterns |
|---|---|---|---|
| `/` | https://reviewgate.lvtd.dev/ | ReviewGate AI code review gate | All |
| `/#purpose` | https://reviewgate.lvtd.dev/#purpose | Merge-readiness signal for agent PRs | A, B, C, D, E |
| `/#install` | https://reviewgate.lvtd.dev/#install | Install ReviewGate as a GitHub Action | A, B, C, D, E |
| `/#configure` | https://reviewgate.lvtd.dev/#configure | Configure review angles and severity | A, B, C, D, E |

There is currently no `/pricing`, `/about`, `/features/*`, `/tools/*`, or blog route. Do not create links to those paths until the relevant phase ships them.

### Documentation and source (external, supporting links)

| URL | Title | Notes |
|---|---|---|
| https://github.com/LVTD-LLC/reviewgate | ReviewGate source and README | Primary installation and product contract. |
| https://github.com/LVTD-LLC/reviewgate/blob/main/docs/external-agent-workflow.md | External agent workflow | Supports repair-loop claims. |
| https://github.com/LVTD-LLC/reviewgate/blob/main/docs/evaluation.md | Evaluation guide | Supports testing and credibility claims. |

## SEO-sprint-generated pages

### `/alternatives/[slug]`

| Slug | Ships in phase | URL | Inbound links from | Outbound links to |
|---|---|---|---|---|
| `coderabbit` | 3 | `/alternatives/coderabbit` | Phase 2 spine + later siblings | Homepage, install, configure, sibling alternatives |
| `greptile` | 5 | `/alternatives/greptile` | Phase 2 spine + CodeRabbit alternative | Homepage, install, configure, sibling alternatives |
| `qodo` | 8 | `/alternatives/qodo` | Phase 2 spine + comparison pages | Homepage, install, configure, sibling alternatives |

### `/for/[slug]`

| Slug | Ships in phase | URL | Inbound links from | Outbound links to |
|---|---|---|---|---|
| `open-source-maintainers` | 7 | `/for/open-source-maintainers` | Homepage + Phase 2 spine | Install, configure, alternatives, playbooks |
| `agent-written-pull-requests` | 9 | `/for/agent-written-pull-requests` | Homepage + Phase 2 spine | Install, configure, alternatives, playbooks |

### `/compare/[slug]`

| Slug | Ships in phase | URL | Inbound links from | Outbound links to |
|---|---|---|---|---|
| `coderabbit-vs-greptile` | 4 | `/compare/coderabbit-vs-greptile` | Phase 2 spine + both alternative pages | Both alternatives, maintainer use case, install |
| `coderabbit-vs-qodo` | 10 | `/compare/coderabbit-vs-qodo` | Phase 2 spine + Qodo alternative | Both alternatives, maintainer use case, install |

### `/playbooks/[slug]`

| Slug | Ships in phase | URL | Inbound links from | Outbound links to |
|---|---|---|---|---|
| `code-review-checklist` | 6 | `/playbooks/code-review-checklist` | Homepage + Phase 2 spine | Install, configure, alternatives, use cases |
| `code-review-agent` | 11 | `/playbooks/code-review-agent` | Homepage + Phase 2 spine | Install, configure, alternatives, use cases |
| `ai-code-review` | 12 | `/playbooks/ai-code-review` | Homepage + Phase 2 spine | Install, configure, alternatives, use cases |

## Anchor-text variations

### Homepage

- “open-source AI code review gate”
- “ReviewGate for agent-written pull requests”
- “score an agent PR before merge”
- “GitHub Actions-first AI review”

### Install section

- “install the ReviewGate GitHub Action”
- “add AI PR review to GitHub Actions”
- “set up the review gate in CI”
- “copy the ReviewGate workflow”

### Configuration section

- “configure review angles”
- “set the inline severity floor”
- “tune ReviewGate for your repository”
- “ReviewGate configuration options”
