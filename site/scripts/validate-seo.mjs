import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const expectedPages = [
  ["index.html", "https://reviewgate.lvtd.dev/"],
  ["docs/index.html", "https://reviewgate.lvtd.dev/docs/"],
  ["docs/quickstart/index.html", "https://reviewgate.lvtd.dev/docs/quickstart/"],
  [
    "docs/github-actions/index.html",
    "https://reviewgate.lvtd.dev/docs/github-actions/",
  ],
  ["docs/cli/index.html", "https://reviewgate.lvtd.dev/docs/cli/"],
  [
    "docs/configuration/index.html",
    "https://reviewgate.lvtd.dev/docs/configuration/",
  ],
  ["docs/features/index.html", "https://reviewgate.lvtd.dev/docs/features/"],
  ["docs/artifacts/index.html", "https://reviewgate.lvtd.dev/docs/artifacts/"],
  [
    "docs/agent-workflows/index.html",
    "https://reviewgate.lvtd.dev/docs/agent-workflows/",
  ],
  ["docs/security/index.html", "https://reviewgate.lvtd.dev/docs/security/"],
  [
    "docs/troubleshooting/index.html",
    "https://reviewgate.lvtd.dev/docs/troubleshooting/",
  ],
  ["blog/index.html", "https://reviewgate.lvtd.dev/blog/"],
  ["sitemap/index.html", "https://reviewgate.lvtd.dev/sitemap/"],
  [
    "blog/how-to-tell-if-code-is-ai-generated/index.html",
    "https://reviewgate.lvtd.dev/blog/how-to-tell-if-code-is-ai-generated/",
  ],
  [
    "blog/best-ai-code-review-tools/index.html",
    "https://reviewgate.lvtd.dev/blog/best-ai-code-review-tools/",
  ],
  [
    "blog/ai-code-review-github/index.html",
    "https://reviewgate.lvtd.dev/blog/ai-code-review-github/",
  ],
  [
    "blog/pr-review-prompts/index.html",
    "https://reviewgate.lvtd.dev/blog/pr-review-prompts/",
  ],
  [
    "blog/pull-request-review-comments/index.html",
    "https://reviewgate.lvtd.dev/blog/pull-request-review-comments/",
  ],
  [
    "blog/ai-code-review-benchmark/index.html",
    "https://reviewgate.lvtd.dev/blog/ai-code-review-benchmark/",
  ],
];

const titles = new Set();
const descriptions = new Set();
const builtPages = new Map();

const pageSources = await Promise.all(
  expectedPages.map(async ([file, expectedCanonical]) => [
    file,
    expectedCanonical,
    await readFile(new URL(`../dist/${file}`, import.meta.url), "utf8"),
  ]),
);

for (const [file, expectedCanonical, html] of pageSources) {
  const title = html.match(/<title>([^<]+)<\/title>/)?.[1];
  const description = html.match(/<meta name="description" content="([^"]+)"/)?.[1];
  const canonical = html.match(/<link rel="canonical" href="([^"]+)"/)?.[1];

  assert(title, `${file} must have a title`);
  assert(description, `${file} must have a meta description`);
  assert.equal(canonical, expectedCanonical, `${file} must have the expected canonical URL`);
  assert(title.length <= 60, `${file} title must be at most 60 characters`);
  assert(description.length <= 155, `${file} description must be at most 155 characters`);
  assert(!titles.has(title), `${file} title must be unique`);
  assert(!descriptions.has(description), `${file} description must be unique`);
  assert.equal(html.match(/<h1(?:\s|>)/g)?.length, 1, `${file} must contain exactly one H1`);

  titles.add(title);
  descriptions.add(description);
  builtPages.set(new URL(expectedCanonical).pathname.replace(/\/$/, "") || "/", html);
}

for (const [sourcePath, html] of builtPages) {
  const internalDocLinks = [...html.matchAll(/href="(\/docs(?:\/[^"#?]*)?(?:#[^"]+)?)"/g)];

  for (const [, href] of internalDocLinks) {
    const [rawPath, fragment] = href.split("#");
    const targetPath = rawPath.replace(/\/$/, "") || "/";
    const targetHtml = builtPages.get(targetPath);

    assert(targetHtml, `${sourcePath} links to missing documentation page ${rawPath}`);
    if (fragment) {
      assert(
        targetHtml.includes(`id="${fragment}"`),
        `${sourcePath} links to missing documentation section ${href}`,
      );
    }
  }
}

const homepage = builtPages.get("/");
assert(homepage, "homepage must be present in built pages");
const jsonLdSource = homepage.match(
  /<script type="application\/ld\+json">([\s\S]*?)<\/script>/,
)?.[1];
assert(jsonLdSource, "homepage must contain JSON-LD");

const schemaTypes = JSON.parse(jsonLdSource).map((entry) => entry["@type"]);
assert.deepEqual(schemaTypes, ["SoftwareApplication", "Organization"]);

const article = builtPages.get("/blog/how-to-tell-if-code-is-ai-generated");
assert(article, "article must be present in built pages");
const articleJsonLdSource = article.match(
  /<script type="application\/ld\+json">([\s\S]*?)<\/script>/,
)?.[1];
assert(articleJsonLdSource, "article must contain JSON-LD");
const articleSchemaTypes = JSON.parse(articleJsonLdSource).map((entry) => entry["@type"]);
assert.deepEqual(articleSchemaTypes, ["Article", "HowTo", "FAQPage", "BreadcrumbList"]);

const githubReviewArticle = builtPages.get("/blog/ai-code-review-github");
assert(githubReviewArticle, "GitHub review article must be present in built pages");
const githubReviewJsonLdSource = githubReviewArticle.match(
  /<script type="application\/ld\+json">([\s\S]*?)<\/script>/,
)?.[1];
assert(githubReviewJsonLdSource, "GitHub review article must contain JSON-LD");
const githubReviewSchemaTypes = JSON.parse(githubReviewJsonLdSource).map(
  (entry) => entry["@type"],
);
assert.deepEqual(githubReviewSchemaTypes, ["BlogPosting", "HowTo", "BreadcrumbList"]);

const reviewCommentsArticle = builtPages.get("/blog/pull-request-review-comments");
assert(reviewCommentsArticle, "review comments article must be present in built pages");
const reviewCommentsJsonLdSource = reviewCommentsArticle.match(
  /<script type="application\/ld\+json">([\s\S]*?)<\/script>/,
)?.[1];
assert(reviewCommentsJsonLdSource, "review comments article must contain JSON-LD");
const reviewCommentsSchemaTypes = JSON.parse(reviewCommentsJsonLdSource).map(
  (entry) => entry["@type"],
);
assert.deepEqual(reviewCommentsSchemaTypes, [
  "BlogPosting",
  "HowTo",
  "FAQPage",
  "BreadcrumbList",
]);

const benchmarkArticle = builtPages.get("/blog/ai-code-review-benchmark");
assert(benchmarkArticle, "benchmark article must be present in built pages");
const benchmarkJsonLdSource = benchmarkArticle.match(
  /<script type="application\/ld\+json">([\s\S]*?)<\/script>/,
)?.[1];
assert(benchmarkJsonLdSource, "benchmark article must contain JSON-LD");
const benchmarkSchemaTypes = JSON.parse(benchmarkJsonLdSource).map(
  (entry) => entry["@type"],
);
assert.deepEqual(benchmarkSchemaTypes, ["BlogPosting", "FAQPage", "BreadcrumbList"]);

const robots = await readFile(new URL("../dist/robots.txt", import.meta.url), "utf8");
assert.match(robots, /^User-agent: \*\nAllow: \//);
assert.match(robots, /Sitemap: https:\/\/reviewgate\.lvtd\.dev\/sitemap-index\.xml/);

const sitemapIndex = await readFile(new URL("../dist/sitemap-index.xml", import.meta.url), "utf8");
assert.match(sitemapIndex, /https:\/\/reviewgate\.lvtd\.dev\/sitemap-0\.xml/);

const sitemap = await readFile(new URL("../dist/sitemap-0.xml", import.meta.url), "utf8");
for (const [, canonical] of expectedPages) {
  assert(sitemap.includes(`<loc>${canonical}</loc>`), `sitemap must include ${canonical}`);
}

const sitemapPage = builtPages.get("/sitemap");
assert(sitemapPage, "HTML sitemap must be present in built pages");
for (const canonical of sitemap.matchAll(/<loc>(https:\/\/reviewgate\.lvtd\.dev\/[^<]*)<\/loc>/g)) {
  const route = new URL(canonical[1]).pathname;
  assert(sitemapPage.includes(`href="${route}"`), `HTML sitemap must link to ${route}`);
}

console.log("validate-seo: ok");
