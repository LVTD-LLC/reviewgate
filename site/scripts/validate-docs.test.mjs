import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { parse } from "yaml";

const siteRoot = new URL("../", import.meta.url);
const repoRoot = new URL("../../", import.meta.url);

const docSlugs = [
  "index",
  "quickstart",
  "github-actions",
  "cli",
  "configuration",
  "features",
  "artifacts",
  "agent-workflows",
  "security",
  "troubleshooting",
];

const docSources = new Map();

async function readDoc(slug) {
  if (!docSources.has(slug)) {
    docSources.set(
      slug,
      readFile(new URL(`src/pages/docs/${slug}.md`, siteRoot), "utf8"),
    );
  }

  return docSources.get(slug);
}

test("publishes the complete agent-first documentation set", async () => {
  const docs = await Promise.all(docSlugs.map(readDoc));

  for (const [index, source] of docs.entries()) {
    assert.match(source, /^---\nlayout: /, `${docSlugs[index]} must use the docs layout`);
    assert.match(source, /\ntitle: "/, `${docSlugs[index]} must declare a title`);
    assert.match(source, /\ndescription: "/, `${docSlugs[index]} must declare a description`);
    assert.match(source, /\nheading: "/, `${docSlugs[index]} must declare a heading`);
    assert.match(source, /\nlede: "/, `${docSlugs[index]} must declare a lede`);
  }
});

test("documents every public Action input", async () => {
  const manifest = parse(await readFile(new URL("action.yml", repoRoot), "utf8"));
  const docs = [
    await readDoc("github-actions"),
    await readDoc("configuration"),
  ].join("\n");

  for (const input of Object.keys(manifest.inputs)) {
    assert(
      docs.includes(`\`${input}\``),
      `Action input ${input} must appear in the Action or configuration docs`,
    );
  }
});

test("keeps the reference workflow parseable and fork-safe", async () => {
  const source = await readDoc("github-actions");
  const workflowSource = source.match(/```yaml\n([\s\S]*?)\n```/)?.[1];
  assert(workflowSource, "GitHub Actions docs must contain a YAML workflow");

  const workflow = parse(workflowSource);
  assert.deepEqual(workflow.on.pull_request.types, [
    "opened",
    "synchronize",
    "reopened",
    "ready_for_review",
  ]);
  assert.deepEqual(workflow.on.issue_comment.types, ["created"]);
  assert.match(workflow.jobs.review.if, /head\.repo\.full_name == github\.repository/);
  assert.match(workflow.jobs.review.if, /dependabot\[bot\]/);
  assert.deepEqual(workflow.jobs.review.permissions, {
    actions: "read",
    attestations: "read",
    contents: "read",
    "pull-requests": "write",
    issues: "write",
    checks: "write",
    statuses: "read",
  });
  assert.deepEqual(workflow.jobs.rereview.permissions, {
    actions: "write",
    attestations: "read",
    contents: "read",
    "pull-requests": "write",
    issues: "write",
  });
  assert.equal(
    workflow.jobs.rereview.steps[0].with.openrouter_api_key,
    undefined,
    "rereview mode must not receive the model key",
  );

  const serialized = JSON.stringify(workflow);
  assert(!serialized.includes("pull_request_target"));
  assert(serialized.includes("fetch-depth"));
  assert(serialized.includes("persist-credentials"));
});

test("documents the public CLI and exact agent stop contract", async () => {
  const cli = await readDoc("cli");
  const workflow = await readDoc("agent-workflows");
  const troubleshooting = await readDoc("troubleshooting");
  const commands = [
    "review",
    "check",
    "disposition",
    "fixture-review",
    "review-pr",
    "render-summary",
    "recheck",
    "request-rereview",
    "eval-fixtures",
  ];

  for (const command of commands) {
    assert(cli.includes(`\`${command}\``), `CLI docs must include ${command}`);
  }

  for (const contract of [
    'schema_version == "reviewgate-agent-result/v1"',
    '.disposition == "still_open"',
    ".blocking_reason != null",
    '.status == "passed"',
    ".score == 5",
    ".reviewed_sha == $head",
  ]) {
    assert(workflow.includes(contract), `agent workflow must include ${contract}`);
  }

  assert.match(
    workflow,
    /\.schema_version == "reviewgate-agent-result\/v1"[\s\S]*?and \.status == "passed"[\s\S]*?and \.score == 5[\s\S]*?and \.reviewed_sha == \$head[\s\S]*?select\(\.disposition == "still_open" and \.blocking_reason != null\)[\s\S]*?\| length == 0/,
    "agent workflow must document the complete exact-head stop predicate",
  );
  assert.match(
    troubleshooting,
    /--workflow \.github\/workflows\/reviewgate\.yml/,
    "troubleshooting must use a valid workflow path selector",
  );
  assert.match(
    cli,
    /reviewgate review[\s\S]*?--wait[\s\S]*?--timeout-seconds 600/,
    "CLI docs must include the bounded first-class review loop",
  );
  assert.match(
    cli,
    /\| `2` \| `needs_changes`[\s\S]*?\| `3` \| `review_error`/,
    "CLI docs must preserve structured review outcome exit codes",
  );
});

test("keeps credential and artifact examples safe to paste", async () => {
  const docs = await Promise.all(docSlugs.map(readDoc));

  for (const [index, source] of docs.entries()) {
    assert.doesNotMatch(
      source,
      /export OPENROUTER_API_KEY=['"]/,
      `${docSlugs[index]} must not put a literal API key assignment in shell history`,
    );
  }

  for (const slug of ["cli", "artifacts", "troubleshooting"]) {
    const source = await readDoc(slug);
    const shellBlocks = [...source.matchAll(/```bash\n([\s\S]*?)\n```/g)].map(
      ([, block]) => block,
    );
    const unsafeRedirect = shellBlocks.find(
      (block) =>
        /> ?\.reviewgate\/result\.json/.test(block) &&
        !block.includes("mkdir -p .reviewgate"),
    );
    assert.equal(
      unsafeRedirect,
      undefined,
      `${slug} must create .reviewgate in every block that redirects a result into it`,
    );
  }
});

test("llms.txt routes agents to every documentation page", async () => {
  const llms = await readFile(new URL("public/llms.txt", siteRoot), "utf8");

  for (const slug of docSlugs) {
    const path = slug === "index" ? "/docs/" : `/docs/${slug}/`;
    assert(llms.includes(path), `llms.txt must include ${path}`);
  }

  assert.match(llms, /untrusted data/);
  assert.match(llms, /Stop only when/);
});
