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
| `coderabbit` | 3 | `/alternatives/coderabbit` | Homepage + resources index, added atomically in Phase 3; later siblings | Homepage, install, configure, sibling alternatives |
| `greptile` | 5 | `/alternatives/greptile` | Resources index + CodeRabbit alternative, added atomically in Phase 5 | Homepage, install, configure, sibling alternatives |
| `qodo` | 8 | `/alternatives/qodo` | Resources index + comparison hub, added atomically in Phase 8 | Homepage, install, configure, sibling alternatives |

### `/for/[slug]`

| Slug | Ships in phase | URL | Inbound links from | Outbound links to |
|---|---|---|---|---|
| `open-source-maintainers` | 7 | `/for/open-source-maintainers` | Homepage + resources index, added atomically in Phase 7 | Install, configure, alternatives, playbooks |
| `agent-written-pull-requests` | 9 | `/for/agent-written-pull-requests` | Homepage + resources index, added atomically in Phase 9 | Install, configure, alternatives, playbooks |

### `/compare/[slug]`

| Slug | Ships in phase | URL | Inbound links from | Outbound links to |
|---|---|---|---|---|
| `coderabbit-vs-greptile` | 4 | `/compare/coderabbit-vs-greptile` | Resources index + CodeRabbit alternative, added atomically in Phase 4 | Available alternatives, maintainer use case once live, install |
| `coderabbit-vs-qodo` | 10 | `/compare/coderabbit-vs-qodo` | Resources index + Qodo alternative, added atomically in Phase 10 | Both alternatives, maintainer use case, install |

### `/playbooks/[slug]`

| Slug | Ships in phase | URL | Inbound links from | Outbound links to |
|---|---|---|---|---|
| `code-review-checklist` | 6 | `/playbooks/code-review-checklist` | Homepage + resources index, added atomically in Phase 6 | Install, configure, alternatives, live use cases |
| `code-review-agent` | 11 | `/playbooks/code-review-agent` | Homepage + resources index, added atomically in Phase 11 | Install, configure, alternatives, use cases |
| `ai-code-review` | 12 | `/playbooks/ai-code-review` | Homepage + resources index, added atomically in Phase 12 | Install, configure, alternatives, use cases |

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
