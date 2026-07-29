# ReviewGate SEO Sprint — Roadmap

> **Canonical document.** This is the single source of truth for ReviewGate's multi-phase organic-search sprint. The scope is the Astro marketing site under `site/`.

## How to use this document

1. Read this roadmap, `.seo/brand.md`, `.seo/link-inventory.md`, and `.seo/config.json`.
2. Take the lowest-numbered pending phase unless Rasul selects a different phase.
3. Re-query commercial-intent terms when the cached research is older than 90 days or competitor pricing/features may have changed.
4. Execute one phase per PR, run the listed verification, and update the tracker plus link inventory in the same PR.
5. Every LVTD PR must pass CI and the Greptile 5/5 gate with zero unresolved actionable comments before merge.

## Phase Status Tracker

| # | Phase | Pattern | Status | PR |
|---|---|---|---|---|
| 0 | Technical foundations | Setup | in_progress | [#55](https://github.com/LVTD-LLC/reviewgate/pull/55); post-deploy verification pending |
| 1 | Retarget the homepage for “AI code review tool” | Homepage boost | pending | – |
| 2 | Build the internal-link spine and reusable Astro SEO layouts | Internal links | pending | – |
| 3 | CodeRabbit alternatives | A — alternatives | pending | – |
| 4 | CodeRabbit vs Greptile | D — compare | pending | – |
| 5 | Greptile alternatives | A — alternatives | pending | – |
| 6 | Code review checklist | E — playbook | pending | – |
| 7 | AI code review for open-source maintainers | C — audience | pending | – |
| 8 | Qodo alternatives | A — alternatives | pending | – |
| 9 | Review agent-written pull requests | B — use case | pending | – |
| 10 | CodeRabbit vs Qodo | D — compare | pending | – |
| 11 | Code review agent guide | E — playbook | pending | – |
| 12 | AI code review guide | E — playbook | pending | – |
| 13 | Directory submissions | Off-page | pending | – |
| 14 | Listicle outreach | Off-page | pending | – |

Statuses: `pending` → `in_progress` → `completed`. Add the PR number after merge. Use `skipped` only with a one-line reason.

## Reference Data

### Site facts

- **Domain:** https://reviewgate.lvtd.dev
- **Keyword source:** DataForSEO, United States / English, measured 2026-07-22
- **Connected sources:** GSC domain property, DataForSEO, ReviewGate PostHog project, Exa, Jina Reader, live web search
- **Degraded sources:** Plausible query failed authorization
- **Authority baseline:** DataForSEO returned no domain-rank/backlink record and zero ranked keywords for the new subdomain
- **Stack:** Astro 7, TypeScript 6, npm, static output
- **Marketing root:** `site/src/pages`
- **Layout:** `site/src/layouts/BaseLayout.astro`
- **Accent / fonts:** Blue `oklch(52% 0.19 252)`; system sans-serif

### Tool evidence snapshot

| Source | Status | Credential/config evidence | API/tool evidence | Used for | Saved config | Reason |
|---|---|---|---|---|---|---|
| GSC | connected | Infisical `/services/google-search-console` | `sc-domain:lvtd.dev` queried; 0 ReviewGate rows and no submitted sitemap | property/indexing baseline | `sc-domain:lvtd.dev` | The domain property covers the ReviewGate subdomain. |
| Ahrefs | missing | Loaded tools, environment, TOOLS.md, repo, recursive Infisical name scan | Not attempted | none | `ahrefs_project_id: null` | No credential or MCP connection found. |
| DataForSEO | connected | Infisical `/services/dataforseo` | Keyword, rank, backlink, and live SERP requests succeeded | demand, KD, CPC, SERPs, authority | US / English | Primary measured market-data source. |
| Plausible | attempted_failed | Runtime key + TOOLS.md API host | ReviewGate v2 query returned 401 | conversion weighting | site ID recorded | Key lacks access or site does not exist. |
| PostHog | connected | Runtime personal API key + TOOLS.md | ReviewGate project 534132 provisioned with anonymized IPs and no session replay | pageviews and install/source intent | project ID + hosts | Production build key is stored as a GitHub Actions secret. |
| Exa / web search | connected | Runtime Exa key + live search tool | Eight relevant category sources plus current SERPs | competitor and source discovery | no ID needed | Category and outreach surfaces confirmed. |
| Jina / Firecrawl / WebFetch | connected | Runtime Jina and Firecrawl keys | Jina returned 200 for five relevant product/source pages | page extraction | no ID needed | Current competitor and product pages were inspectable. |

### Existing programmatic surface

| Route pattern | Status | Notes |
|---|---|---|
| `/` | live | Single Astro homepage with anchors for purpose, install, and configuration. |
| `/alternatives/*` | absent | No collection, route, layout, or entries. |
| `/for/*` | absent | No collection, route, layout, or entries. |
| `/compare/*` | absent | No collection, route, layout, or entries. |
| `/playbooks/*` | absent | No collection, route, layout, or entries. |

### Critical files

| File | What lives there |
|---|---|
| `site/astro.config.mjs` | Domain and future sitemap integration. |
| `site/src/pages/index.astro` | Entire current marketing page and homepage copy. |
| `site/src/layouts/BaseLayout.astro` | Title, description, canonical, Open Graph, JSON-LD, and analytics bootstrap. |
| `site/src/styles/global.css` | Visual tokens and page styles. |
| `deployment/nginx.conf` | Static hosting and route behavior. |
| `.seo/keyword-research.json` | Measured keyword, competitor, SERP, and extraction cache. |
| `.seo/link-inventory.md` | Valid internal link targets and planned pages. |

### Conventions and quality bars

- Slugs are lowercase and hyphenated.
- Alternative pages include at least three honest tradeoffs where the competitor wins.
- The first alternatives-page H2 uses “Best [Brand] alternatives in 2026.”
- Minimum body lengths: alternatives 600 words, use case/audience 800, comparison 700, playbook 2,500.
- Every generated page gets `BreadcrumbList`; FAQ pages get `FAQPage`; product/use-case pages get `SoftwareApplication`; playbooks get `Article`.
- Every new page must have at least two inbound links before merge.
- Do not invent a `/pricing` route: ReviewGate is free and open source. Comparison CTAs should point to `/`, `/#install`, and the GitHub repository.
- Use Astro components/content collections without client-side JavaScript for static marketing copy.

## Keyword Research Appendix

### Owned search and analytics baseline

- DataForSEO found **0 ranked keywords** for `reviewgate.lvtd.dev` and no backlink-summary row. Treat authority as effectively new/unknown.
- The connected GSC service account can query the `sc-domain:lvtd.dev` property. It returned no ReviewGate query/page rows for the latest 90-day window and no submitted ReviewGate sitemap.
- Plausible conversion data is unavailable because the site/key query returned 401.
- PostHog project 534132 now exists for ReviewGate. It will start collecting anonymous pageviews plus explicit install/source intent after Phase 0 deploys.
- There are therefore no striking-distance or conversion-weighted opportunities yet. Phase ordering uses measured market demand, SERP shape, product fit, and implementation dependencies.

### Primary commercial opportunity

| Target | Volume | KD | CPC | Intent | Decision |
|---|---:|---:|---:|---|---|
| `ai code review tool` | 590 | 8 | $42.95 | commercial | Retarget homepage first; the mixed SERP includes product homepages at ranks 4–5. |
| `code review agent` | 210 | 9 | $26.01 | commercial | Build a deep guide after the internal-link spine and early commercial pages. |
| `code review automation` | 170 | 24 | $21.99 | commercial | Defer until the domain earns authority. |
| `ai code review` | 1,300 | 32 | $63.85 | informational | High-value head term, but too competitive for the new subdomain today. |

### Alternatives candidates

| Target | Volume | KD | CPC | Confidence | Priority note |
|---|---:|---:|---:|---|---|
| `coderabbit alternatives` | 140 | 0 | $31.54 | measured | Best first programmatic page; strong commercial SERP and product-led results. |
| `greptile alternatives` | 30 | — | $49.14 | measured | Low volume but unusually high commercial value. |
| `qodo alternative` | 10 | — | $56.92 | measured | Small demand; publish after CodeRabbit/Greptile establish the pattern. |
| `sourcery alternative` | 10 | — | — | measured | Backlog, not an initial sprint priority. |

### Use-case and audience candidates

| Target | Volume | KD | Confidence | Priority note |
|---|---:|---:|---|---|
| `automated pull request review` | 40 | — | measured | Fold into the agent-written PR use-case page. |
| `AI code review for open-source maintainers` | — | — | estimated | Strategic persona match; validate the exact query family before drafting. |
| `code review for agent-written pull requests` | — | — | estimated | Core positioning and emerging demand; strategic even before keyword tools catch up. |

### Comparison candidates

| Target | Volume | KD | CPC | Priority note |
|---|---:|---:|---:|---|
| `coderabbit vs greptile` | 110 | 0 | $36.26 | Highest-intent early comparison; current SERP lacks a neutral category owner. |
| `coderabbit vs qodo` | 20 | 0 | — | Smaller but commercial; ship after the Qodo alternatives page. |

### Playbook candidates

| Target | Volume | KD | CPC | Priority note |
|---|---:|---:|---:|---|
| `code review checklist` | 170 | 0 | $9.13 | Easy informational win and a natural internal-link hub. |
| `code review agent` | 210 | 9 | $26.01 | Strong product-adjacent guide with a mixed, beatable SERP. |
| `ai code review` | 1,300 | 32 | $63.85 | Publish only after supporting pages and links exist. |

### SERP reality

- `ai code review tool` mixes Medium, Reddit, CodeRabbit, Greptile, LogRocket, DeepSource, and Augment. The result type supports a focused product homepage, but ReviewGate must clearly explain its GitHub Actions/BYOK wedge.
- `coderabbit alternative` is led by product-led alternative pages plus Reddit and roundups. A transparent page with deployment, cost, privacy, context, and workflow tradeoffs matches intent.
- `coderabbit vs greptile` mixes Reddit, Greptile's own comparison, editorial comparisons, YouTube, and SourceForge. Neutral, source-linked comparison content has room.
- `code review agent` mixes GitHub repositories, engineering guides, vendor pages, docs, and community discussion. A practical build-vs-buy and evaluation guide fits better than a thin landing page.

### Conversion-weighted opportunities

No conversion source is usable for ReviewGate yet. Use CPC only as a weak commercial proxy until Phase 0 adds measurement. The highest CPC candidates are `ai code review` ($63.85), `qodo alternative` ($56.92), `greptile alternatives` ($49.14), `ai code review tool` ($42.95), and `coderabbit vs greptile` ($36.26).

### Striking-distance opportunities

None: DataForSEO reports zero ranked keywords, and the `sc-domain:lvtd.dev` property returned zero ReviewGate query/page rows.

### Out of scope

- Broad software code-review terms unrelated to AI or pull requests.
- Hosted-dashboard, auto-fix, or enterprise-compliance claims that contradict ReviewGate's v0 product constraints.
- “Best” claims without a reproducible evaluation basis.
- More than three alternative pages before the internal-link spine exists.

## Phases

### Phase 0 — Technical foundations

**Why:** Production currently returns 404 for both the sitemap and `/robots.txt`, the 160-character homepage description exceeds the sprint heuristic, and the homepage has no JSON-LD. The July 29 redesign added unique `/docs` and `/blog` metadata, so the old duplicate/missing-page concerns no longer apply.

**Scope:**

1. Add `@astrojs/sitemap`, configure it in `site/astro.config.mjs`, and verify the generated sitemap index covers `/`, `/docs/`, and `/blog/`.
2. Add `site/public/robots.txt` allowing crawl and referencing `https://reviewgate.lvtd.dev/sitemap-index.xml`.
3. Shorten the homepage description to 147 characters without dropping the action, score, audience, or review-contract positioning.
4. Add reusable JSON-LD support to `BaseLayout.astro`; emit `SoftwareApplication` and `Organization` on the homepage.
5. Use the existing `sc-domain:lvtd.dev` GSC property and submit the sitemap after deployment.
6. Provision a privacy-limited PostHog project with anonymized IPs and no session replay; measure anonymous pageviews plus explicit install/source intent events.
7. Keep production measurement credentials outside the repository by injecting the public project key from GitHub Actions at image build time.

**Files:** `site/package.json`, `site/package-lock.json`, `site/astro.config.mjs`, `site/public/robots.txt`, `site/src/layouts/BaseLayout.astro`, `site/src/scripts/analytics.ts`, page/CTA components, `deployment/Dockerfile`, `.github/workflows/deploy.yml`, `.seo/config.json`, `.seo/link-inventory.md`, `docs/seo-sprint.md`, `CHANGELOG.md`.

**Verification:** Astro check/build; generated sitemap index and robots return valid files; one H1 per page; unique title/description/canonical; JSON-LD parses; dependency audit is clean; production smoke check after deploy; GSC sitemap submission recorded; PostHog receives production pageview and intent events.

**Post-deployment pending:** verify production, submit `sitemap-index.xml` through `sc-domain:lvtd.dev`, and confirm PostHog receives the expected anonymous events before changing the tracker status to `completed`.

### Phase 1 — Retarget the homepage for “AI code review tool”

**Why:** 590 monthly searches, KD 8, and $42.95 CPC make this the strongest measured commercial opportunity.

**Scope:** Keep the sharper agent-PR positioning while making “open-source AI code review tool” explicit in the title, H1 support copy, first paragraph, feature language, and relevant anchors. Add a concise “who it is for / who it is not for” section and source-linked trust proof. Do not turn the page into a generic reviewer pitch.

**Verification:** Target phrase appears naturally; title remains concise; one H1; product constraints remain accurate; no unsupported comparative claims; build succeeds.

### Phase 2 — Internal-link spine and reusable Astro SEO layouts

**Why:** The site currently has one route. Programmatic pages would be islands and cannot meet inbound-link or feature/tool-link quality bars.

**Scope:** Add a crawlable resources/index surface linked from global navigation and the homepage; define Astro content collections and shared layouts for alternatives, comparisons, use cases, and playbooks; add breadcrumb/FAQ/schema helpers; prepare reusable index-card and contextual-link components for later phases. Do **not** render an anchor or sitemap entry for a route until that route ships. Each page phase must add its own destination plus at least two inbound links atomically in the same PR.

**Verification:** Shared layouts and route families build without emitting placeholder pages; the resource index contains no link whose destination is absent from `dist/`; every next-phase page has two documented inbound locations that its own phase will activate atomically; schema helpers have fixture coverage or deterministic build assertions.

### Phase 3 — `/alternatives/coderabbit`

Target `coderabbit alternatives` (volume 140, KD 0, CPC $31.54). Compare hosted convenience and mature UX honestly against ReviewGate's open-source, GitHub Actions-first, BYOK, score-centered model. Include at least three cases where CodeRabbit is the better fit. Re-extract current official pricing/features before writing.

### Phase 4 — `/compare/coderabbit-vs-greptile`

Target `coderabbit vs greptile` (volume 110, KD 0, CPC $36.26). Make the page a neutral three-way decision guide: CodeRabbit vs Greptile, with ReviewGate as the open-source/BYOK option. Cite official sources and avoid declaring an overall winner.

### Phase 5 — `/alternatives/greptile`

Target `greptile alternatives` (volume 30, CPC $49.14). Center deployment model, repository context, score semantics, data handling, and agent-loop artifacts. Admit where Greptile's hosted whole-codebase context is stronger.

### Phase 6 — `/playbooks/code-review-checklist`

Target `code review checklist` (volume 170, KD 0). Publish a 2,500+ word, evidence-backed checklist for agent-written pull requests, with a reusable downloadable/checkable format and natural links to setup, configuration, comparison, and use-case pages.

### Phase 7 — `/for/open-source-maintainers`

Target the strategic audience cluster around free/open-source AI code review. Validate exact query variants before drafting. Emphasize inspectability, contributor-fork safety, BYOK costs, least privilege, and no hosted storage.

### Phase 8 — `/alternatives/qodo`

Target `qodo alternative` (volume 10, CPC $56.92). Distinguish Qodo's broader platform from the open-source PR-Agent lineage, and compare both fairly with ReviewGate's intentionally narrow score-gate contract.

### Phase 9 — `/for/agent-written-pull-requests`

Target `automated pull request review` (volume 40) plus emerging “agent-written PR” language. Show the full maintainer workflow from agent PR to score, findings, repair loop, and human merge decision.

### Phase 10 — `/compare/coderabbit-vs-qodo`

Target `coderabbit vs qodo` (volume 20, KD 0). Provide a sourced comparison and a third-path section for teams that prefer open-source CI-owned review.

### Phase 11 — `/playbooks/code-review-agent`

Target `code review agent` (volume 210, KD 9, CPC $26.01). Cover architecture, threat model, context boundaries, output contracts, evaluation, noise control, and build-vs-buy decisions. Use ReviewGate as a concrete open-source implementation, not as the only answer.

### Phase 12 — `/playbooks/ai-code-review`

Target `ai code review` (volume 1,300, KD 32, CPC $63.85) only after the lower-KD cluster has shipped and earned links. Recheck domain authority and SERP composition before starting; defer again if winnability remains poor.

### Phase 13 — Directory submissions

Use `.seo/backlink-targets.json`. Prioritize Product Hunt, AlternativeTo, SaaSHub, and relevant GitHub Marketplace/category surfaces. Do not describe ReviewGate as SaaS or imply a hosted free tier.

### Phase 14 — Listicle outreach

Approach current AI-code-review roundups only after the homepage, evaluation proof, and at least two comparison/alternative pages are live. Lead with the inspectable open-source/BYOK deployment model and offer reproducible setup/evaluation evidence rather than generic inclusion requests.

## Off-page checklist

- [ ] Product Hunt launch page
- [ ] AlternativeTo listing and competitor relationships
- [ ] SaaSHub listing
- [ ] GitHub Marketplace listing verification/optimization
- [ ] G2 product profile if an open-source action fits its taxonomy
- [ ] Outreach to the current LogRocket, DeepSource, and Augment roundups in `.seo/backlink-targets.json`
- [ ] Publish a reproducible public evaluation that comparison writers can cite

## Next action

Review this roadmap, then run the sprint again to execute **Phase 0 — Technical foundations**. The next two phases after that are the homepage retarget and internal-link spine.
