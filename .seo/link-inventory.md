# ReviewGate — Internal Link Inventory

> Every SEO sprint phase must choose valid links from this inventory and update it when new pages ship.

## Existing pages and anchors

### Homepage and core marketing

| Slug | URL | Title / anchor-text candidate | Used by patterns |
|---|---|---|---|
| `/` | https://reviewgate.lvtd.dev/ | ReviewGate AI code review gate | All |
| `/docs` | https://reviewgate.lvtd.dev/docs/ | ReviewGate documentation | A, B, C, D, E |
| `/docs#install` | https://reviewgate.lvtd.dev/docs/#install | Install ReviewGate as a GitHub Action | A, B, C, D, E |
| `/docs#configure` | https://reviewgate.lvtd.dev/docs/#configure | Configure review angles and severity | A, B, C, D, E |
| `/docs#review-loop` | https://reviewgate.lvtd.dev/docs/#review-loop | Run the ReviewGate repair loop | A, B, C, D, E |
| `/blog` | https://reviewgate.lvtd.dev/blog/ | ReviewGate engineering field notes | E |
| `/blog/how-to-tell-if-code-is-ai-generated` | https://reviewgate.lvtd.dev/blog/how-to-tell-if-code-is-ai-generated/ | How to review code when AI authorship is uncertain | E |
| `/blog/best-ai-code-review-tools` | https://reviewgate.lvtd.dev/blog/best-ai-code-review-tools/ | Best AI code review tools for agent-written PRs | E |
| `/blog/ai-code-review-github` | https://reviewgate.lvtd.dev/blog/ai-code-review-github/ | Merge-safe AI code review on GitHub | E |
| `/blog/pr-review-prompts` | https://reviewgate.lvtd.dev/blog/pr-review-prompts/ | Evidence-bound PR review prompt library | E |
| `/blog/pull-request-review-comments` | https://reviewgate.lvtd.dev/blog/pull-request-review-comments/ | Signal-first pull request review comments | E |
| `/blog/claude-code-review` | https://reviewgate.lvtd.dev/blog/claude-code-review/ | Four-role Claude Code review contract | E |
| `/blog/codex-code-review` | https://reviewgate.lvtd.dev/blog/codex-code-review/ | Codex code review merge-gate workflow | E |
| `/blog/cursor-code-review` | https://reviewgate.lvtd.dev/blog/cursor-code-review/ | Cursor code review and Bugbot merge-gate workflow | E |
| `/blog/ai-code-review-benchmark` | https://reviewgate.lvtd.dev/blog/ai-code-review-benchmark/ | AI code review benchmark and merge-gate scorecard | E |
| `/blog/windsurf-code-review` | https://reviewgate.lvtd.dev/blog/windsurf-code-review/ | Windsurf code review and review-epoch merge-gate workflow | E |
| `/blog/devin-code-review` | https://reviewgate.lvtd.dev/blog/devin-code-review/ | Devin code review and four-authority merge contract | E |
| `/blog/amazon-mandating-ai-code-review` | https://reviewgate.lvtd.dev/blog/amazon-mandating-ai-code-review/ | Amazon AI code review mandate correction and TRACE risk routing | E |

There is currently no `/pricing`, `/about`, `/features/*`, or `/tools/*` route. Do not create links to those paths until the relevant phase ships them.

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

### AI-generated code field note

- “how to review AI-generated code”
- “review code when AI authorship is uncertain”
- “provenance-blind pull request review”
- “evidence gate for agent-written code”
- “signs of AI-generated code”

### Comparison content

- “best AI code review tools”
- “AI code review tool comparison”
- “agent-written PR review tools”
- “how to choose AI code review tools”

### GitHub AI review field note

- “AI code review on GitHub”
- “merge-safe AI review workflow”
- “five bindings for AI code review”
- “exact-head review check”
- “repository-owned AI reviewer”

### PR review prompt library

- “PR review prompt library”
- “evidence-bound PR review prompts”
- “AI code review prompt templates”
- “prompts for agent-written pull requests”
- “specialized code review prompts”

### Pull request review comments guide

- “pull request review comment contract”
- “signal-first review comments”
- “write actionable PR comments”
- “line, file, and PR review scope”
- “review finding disposition”

### Claude Code review field note

- “Claude Code review workflow”
- “four-role Claude Code review contract”
- “review Claude-authored pull requests”
- “separate AI reviewer and merge gate”
- “current-head Claude review”

### Codex code review field note

- “Codex code review workflow”
- “turn Codex findings into a merge gate”
- “Codex AGENTS.md review rules”
- “current-head Codex review”
- “Codex review output contract”

### Cursor code review field note

- “Cursor code review workflow”
- “turn Bugbot findings into a merge gate”
- “scoped BUGBOT.md review rules”
- “current-head Cursor review”
- “Bugbot finding disposition contract”

### AI code review benchmark field note

- “AI code review benchmark”
- “code review benchmark scorecard”
- “evaluate AI code review tools”
- “measure review recall and false blockers”
- “reproducible reviewer evaluation”

### Windsurf code review field note

- “Windsurf code review workflow”
- “Windsurf review epoch contract”
- “Quick Review and Devin Review merge gate”
- “current-head Windsurf review”
- “rerun review after Autofix”

### Devin code review field note

- “Devin code review workflow”
- “four-authority Devin review contract”
- “separate Devin review and merge authority”
- “current-head Devin review gate”
- “rerun review after Devin Auto-Fix”

### Amazon AI code review mandate field note

- “Amazon AI code review mandate correction”
- “TRACE risk routing for AI-assisted code”
- “AI code review approval contract”
- “blast-radius review matrix”
- “exact-head AI code sign-off”
