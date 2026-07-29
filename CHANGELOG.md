# Changelog

Changes are grouped by calendar date, newest first.

## 2026-07-29

- Verified the SEO foundation deployment in production, submitted the sitemap through the covering Search Console domain property with no errors or warnings, and confirmed PostHog receives the pageview and install-intent event contracts.
- Added crawlable sitemap and robots files, homepage product/organization schema, and privacy-limited PostHog measurement for pageviews plus explicit install/source intent. Updated the production smoke test to match the current homepage contract.
- Kept prior findings valid when their original review angle fails or is no longer successful on the current head by making their angle ownership neutral before outcome validation. Advanced the action runtime pin to `v0.4.2`.
- Preserved newly discovered successful-angle findings and disposition updates when publishing an inconclusive review, so canonical history and the next head cannot lose blockers that are present in the machine artifact. Advanced the action runtime pin to `v0.4.1` for the corrective release.
- Fixed runtime release publication when the publish job has no checkout, made optional queue telemetry non-fatal when GitHub omits timestamps, and taught the agent check skill to report every runtime phase.
- Replaced consumer-side Rust compilation with a version-pinned Linux X64 release binary that is built without publication credentials, attested, verified, and smoke-tested before its draft release becomes public. Added explicit 180-second per-angle and 480-second whole-review budgets, typed timeout outcomes, and queue/startup/model/publish timings in review artifacts, canonical summaries, and the structured agent result.
- Preserved grounded findings and structured disposition updates from successful review angles when another angle fails, so a same-head retry cannot pass by merely omitting a blocker first found during an inconclusive run.
- Fixed the action manifest so GitHub can load the new agent-result outputs, and added real YAML parsing to the site check to catch invalid action metadata before release.
- Added a stable `reviewgate-agent-result/v1` contract and exact-head `reviewgate check` retrieval. Every review outcome now writes action outputs and uploads a bounded, SHA-named `reviewgate-agent-result` artifact with structured findings, thread IDs, angle errors, costs, and disposition history. Retrieval accepts artifacts only from the configured ReviewGate workflow's exact PR/head run.
- Made terminal agent-result publication recover from a missing or malformed internal review artifact by emitting a sanitized, non-retryable `review_error` result for the live PR head.
- Added `reviewgate disposition` for authenticated writers to submit `accepted`, `fixed`, `rejected_with_evidence`, `already_implemented`, `intentional_contract`, or `needs_human` as repository/PR/head-bound JSON. Rereviews revalidate each GitHub author's live write permission and carry immutable comment events into canonical finding history without Markdown scraping or dropping repeated state transitions.
- Rebuilt the public site around a bold control-room design focused on the fixed 5/5 merge signal, with a shared responsive navigation system and dedicated Docs and Blog coming-soon routes that provide useful previews and working fallback links instead of 404s.
- Made rereviews converge on the current PR head: ReviewGate now carries repository/PR-bound finding dispositions in its canonical summary state, reviews only the delta since the last validated SHA, reuses semantic finding identities across wording and line movement, retains still-open findings until an explicit `fixed` resolution deletes every prior current-head evidence location and checks each complete non-empty replacement block, persists grounded disposition updates through summary publication, revalidates their repository evidence at that boundary, and suppresses evidence-rejected or intentional findings unless changed code or contract evidence justifies reopening. Pure deletions remain open for explicit disposition because unrelated added evidence cannot prove the underlying path is gone.
- Made still-open blocker policy monotonic across same-identity rereviews: changed evidence and wording can refresh, but lower severity, confidence, or evidence status cannot silently remove the blocker without an explicit validated disposition.
- Added a stricter `0.95` confidence and per-finding novelty-evidence requirement for blockers first introduced after the initial review. Completed reviews on an unchanged head are now idempotent rereview no-ops.
- Replaced the old hidden summary-state format with a base64url-encoded version 2 contract that cannot be broken by untrusted finding text and is rejected when its repository or pull-request binding does not match. Invalid canonical state now fails closed, and finding count, disposition history, and encoded state size are bounded. The project is pre-user, so old hidden state is deliberately rejected instead of migrated.
- Added deterministic five-pass PR #364 and repeated-permission PR #365 convergence fixtures, plus focused tests for every disposition, relevant-code changes, justified reopening, unchanged-head drift, and cross-PR state rejection.

## 2026-07-28

- Added a deterministic finding policy that separates classification, severity, confidence, evidence status, and blocking disposition; only high-confidence validated defects, security findings, and reliability risks now lower the score or fail the ReviewGate check, while contract ambiguities and suggestions remain advisory. Failure to publish that check now fails the action instead of silently leaving an older result in place.
- Added a read-only evidence gate and version 3 artifact schema for blocking findings: P0-P3 claims now need exact current-head or deleted-line citations and a causal path, P0-P1 need reproduction-grade evidence, related tests are inspected, checked platform-contract contradictions are suppressed, and prompts direct self-retracting or uncertain claims to auditable non-blocking notes. Changed-file context is complete within explicit bounds and fails closed instead of silently omitting or truncating content. Version 1 and 2 schemas remain immutable.
- Enforced deterministic score invariants before GitHub publication: completed scores and statuses must match structured findings, per-angle scores must match their referenced findings, and stale or contradictory artifacts now publish a sanitized `review_error` for the current PR head instead of impossible score/check combinations.
- Separated reviewer failures from code-quality outcomes: incomplete reviews now emit typed, retryable `angle_errors`, `status: review_error`, and `score: null`; summaries preserve the latest valid score and check runs fail consistently without inventing a `0/5`. The new artifact shape uses the version 2 public schema while the immutable version 1 schema remains available for v0.1.x consumers.
- Corrected the documented rereview permission contract to grant `pull-requests: write`, which GitHub requires for the bot-owned command reservation comment on PR conversations.
- Added first-class maintainer-requested rereviews through the exact `@reviewgate review` command and composite-action `rereview` mode.
- Replaced branch-name/single-result recheck discovery with paginated exact-PR, exact-workflow, and current-head run selection.
- Hardened rereview authorization with a live effective repository-permission check, stable machine-readable failure results, and subprocess coverage for duplicate, failure, and success behavior.
- Preserved `recheck --workflow` compatibility for exact numeric IDs, workflow file names/paths, and display names by resolving non-numeric selectors to an unambiguous workflow ID.
- Documented the single-workflow installation, least-privilege event-specific jobs, duplicate-delivery handling, fork boundary, and bounded failure behavior.

## 2026-07-22

- Initialized the Astro marketing site's SEO sprint with measured keyword and SERP research, a technical audit, brand context, internal-link inventory, backlink targets, and a phase-by-phase roadmap.

## 2026-07-08

- Redesigned the Astro marketing site around ReviewGate's score-centered purpose, GitHub Actions installation flow, supported features, artifacts, and configuration surface.

## 2026-07-06

- Expanded the root README with full local setup, architecture, configuration, command, testing, deployment, security, and troubleshooting documentation.

## 2026-07-05

- Switched changelog entries from release buckets to calendar-date sections.
- Added configurable review angles backed by inline prompts, prompt files, or repo-local `SKILL.md` instructions.
- Temporarily disabled the dogfood ReviewGate workflow on pull request events, leaving manual dispatch available for explicit runs.
- Added ReviewGate-specific workflow prompt guardrails so the reviewer does not flag the documented `@v0` install tag, required summary/check permissions, or speculative workflow-concurrency concerns as findings without concrete evidence.
- Added an explicit composite-action validation error for missing `OPENROUTER_API_KEY` and refreshed the documented install workflow with a job timeout and safer manual-run concurrency fallback.
- Clarified Action install examples to recommend the moving `v0` tag for early releases instead of agent-suggested latest commit pins.
- Added the Astro marketing site for `reviewgate.lvtd.dev` with CI coverage and token-based CapRover deployment through GitHub Actions.
- Added PR title and description to live review prompts as bounded untrusted scope context so findings stay focused on code issues introduced by the PR.
- Added CI validation for public agent skill frontmatter, fenced Markdown, and shell snippets.
- Added authentication, polling, and push-concurrency guidance to ReviewGate agent skills.
- Documented `skills` CLI installation for ReviewGate agent skills and added comment-before-resolve guidance for agent repair loops.
- Added public `check-reviewgate` and expanded `reviewgate-loop` agent skills for inspecting ReviewGate output and iterating PRs toward `5/5`.

## 2026-07-04

- Added a built-in adversarial review angle to the live PR flow, with per-angle scores in the canonical summary, `angle_results` in the JSON artifact, and angle-labeled finding comments.
- Hardened multi-angle aggregation so failed angles are reflected as `0/5` angle results, generated finding IDs stay unique and bounded, and each angle resolves its own cost metadata.
- Simplified ReviewGate configuration to a single `min_severity` control, removed public target-score, preset, summary-style, inline-confidence, and inline-publish action inputs/schema, and fixed the passing target at `5/5`.
- Changed finding publishing so all findings at or above `min_severity` are posted as inline PR comments when possible; file-level, PR-level, unanchored, or stale-line findings are anchored to fallback right-side diff lines instead of standalone PR comments, and older standalone finding comments are cleaned up.
- Clarified concise summary inline-comment wording so inline candidates describe findings publishable under `min_severity`, with detail left to inline comments and JSON.
- Filter direct line anchors for inline PR comments to added right-side diff lines.
- Added `scope: line|file|pr` to review findings and updated the schema/prompts so scope describes the finding target while publishing remains inline-first.
- Changed the default canonical PR summary to concise output with a compact verdict, one-line cumulative cost, compact finding counts, and finding detail left to PR comments and JSON.
- Fixed concise summaries to keep publishable finding counts visible when inline PR comments cannot be published.
- Expanded the public agent-loop contract for JSON artifacts, canonical summaries, status handling, and stop conditions.

## 2026-07-03

- Left-aligned the concise summary confidence score, added changed-line analysis counts to review metrics and the summary footer, and tightened review prompts around deploy-time data-sync risks.

## 2026-07-02

- Pinned the dogfood ReviewGate workflow to the main-branch action implementation so PRs cannot affect their own review score by changing ReviewGate code under review.
- Changed completed `needs_changes` ReviewGate check runs to use a neutral conclusion while keeping passed reviews green and unavailable reviews failing.
- Tightened model prompt/schema guidance so concrete defects named in verdict prose must also be emitted as structured findings.
- Repaired inline PR comment anchors to matching changed lines before falling back when model-provided line numbers are stale or imprecise.
- Updated the canonical PR summary layout with a Review Gate Summary title, centered confidence score, collapsed Important Files Changed and Mermaid Flowchart sections, and a compact run/cost/latest-commit footer.
- Updated the dogfood ReviewGate workflow to remove the obsolete score-floor input and grant `checks: write` for check-run publishing.
- Moved GitHub summary, start-signal, inline-comment, and check-run publishing from Bash/JQ in the composite action into Rust CLI commands.
- Added a dedicated GitHub Check Run publisher that reports review availability without turning low scores into workflow failures.
- Fixed PR reviewed SHA handling to prefer the pull request head SHA over the checkout merge SHA in GitHub Actions.
- Fixed check-run publishing so the step executes under `always()` and can emit a failure check when the review artifact is unavailable.
- Tightened canonical summary comment selection to ignore user-authored ReviewGate markers and delete only bot-authored duplicate summary comments.
- Added workflow concurrency guidance to reduce duplicate ReviewGate runs on rapid PR updates.
- Updated `anyhow` to 1.0.103 to avoid the RustSec advisory affecting 1.0.102.
- Removed the separate score floor and related action/CLI controls; low-score reviews now report `needs_changes` without failing the workflow.
- Added migration warnings for removed score-floor config keys and backwards-compatible deserialization for legacy failed-status artifacts.
- Removed the concise PR summary metadata row that repeated status, target score, fail-under, and reviewed SHA.
- Defaulted missing inline-comment publish output to unavailable so eligible findings stay visible if the best-effort inline step fails before reporting status.
- Removed the separate action enforcement step; summary publishing failures still fail the publish step directly.
- Added advisory review status semantics for score reporting.
- Kept docs, agent workflow guidance, CI commands, and summary rendering aligned with Rust 1.96, Rust 2024, locked dependency use, and dynamic target-score thresholds.
- Aligned the review artifact status computation and CLI behavior with the configured target-score threshold, and pinned CI setup to auditable toolchain inputs.

## 2026-06-29

- Documented and dogfooded a fork-safe ReviewGate workflow guard so required checks do not fail when GitHub withholds `OPENROUTER_API_KEY` from forked or Dependabot PR events.
- Fixed action summary rendering to fall back to concise mode when `summary_style` is explicitly passed empty.
- Added `summary_style: concise|detailed` and `inline_min_confidence` support in config/action/CLI summary rendering, with detailed mode preserving full cost, metrics, findings, notes, and agent-instruction sections.
- Added a `ReviewGate: running` PR placeholder comment that is replaced by the final canonical summary when review completes.
- Made canonical summary publishing failures visible by failing the publish step with an Actions error instead of hiding them behind `continue-on-error`.
- Added stable OpenRouter attribution headers to chat and model-pricing requests without exposing user secrets.
- Added fixture-backed golden coverage for concise summary output and inline comment payloads.
- Added a fresh `v0` consumer smoke-test procedure for validating moved major tags on new workflow runs.

## 2026-06-28

- Fixed GitHub Action summary and inline comment publishing on current GitHub CLI versions by avoiding the unsupported `gh api --paginate --jq` option combination.

## 2026-06-27

- Removed pre-user migration compatibility for old Shipcheck config and marker names.
- Renamed the product, action, CLI, Rust crates, config/artifact paths, workflow, schema, docs, and public agent-loop skill from Shipcheck to ReviewGate.

## 2026-06-26

- Added blue Marketplace branding with a shield icon for the GitHub Action listing.
- Renamed the GitHub Action display name to `ReviewGate` for Marketplace uniqueness.
- Encoded inline finding marker payloads so every schema-valid finding ID dedupes safely across reruns.
- Added best-effort inline PR comment publishing with stable finding markers, dedupe, configurable severity/confidence floors, and unmappable-line fallback.
- Added review-stage metadata, stage selection, and offline fixture evaluation support.
- Added external-agent workflow, evaluation, and v0.1.0 release readiness docs.
- Added usage-derived OpenRouter cost estimation with live model pricing lookup and fallback pricing.
- Added review metrics to artifacts, schemas, and canonical summaries.
- Added bounded model-output JSON repair for prose-wrapped review artifacts.

## 2026-06-25

- Added configurable summary/inline severity floors, a `reviewgate recheck` helper, and hidden summary state for bounded cumulative PR cost/run history.
- Added Review UX and Control v1 dogfood notes and design guidance for review-only semantics, trigger/recheck choices, model defaults, cost display, severity visibility, and secure workflow behavior.
- Added `P0` severity support, structured cost summary metadata, and cost rendering in canonical summaries.
- Updated OpenRouter preset defaults to prefer price-to-value models: `qwen/qwen3-coder` for cheap and `deepseek/deepseek-v4-flash` for balanced.
- Added the first live `review-pr` CLI path for PR diff/context collection, OpenRouter artifact parsing, summary/artifact output, configurable exit semantics, and mock-artifact dry runs.
- Wired the GitHub Action to run the live CLI path, publish a step summary, and upsert one canonical PR summary comment.
- Added a dogfood ReviewGate workflow and v0.1.0 release-readiness checklist.
- Hardened curl-based OpenRouter calls so secrets and large request bodies are not passed through process arguments.
- Added OpenRouter BYOK model-client boundary types with redacted secret handling, explicit model presets, and mocked transport tests.
- Added GitHub canonical summary upsert planning with create/update/no-op behavior and mocked publisher tests.
- Added Rust-side review artifact validation, summary status output, lockfile audit/provenance documentation, and cleaned ReviewGate context file references.
- Added repo steering files for coding agents, product constraints, technical context, and repository structure.
- Addressed PR review feedback by surfacing agent instructions in summaries, making severity scoring explicit, and removing an unused YAML dependency.
- Created the initial ReviewGate Rust workspace, CLI, GitHub Action scaffold, schemas, prompts, and deterministic fixture milestone.
