import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const expectedPages = [
  ["index.html", "https://reviewgate.lvtd.dev/"],
  ["docs/index.html", "https://reviewgate.lvtd.dev/docs/"],
  ["blog/index.html", "https://reviewgate.lvtd.dev/blog/"],
];

const titles = new Set();
const descriptions = new Set();

for (const [file, expectedCanonical] of expectedPages) {
  const html = await readFile(new URL(`../dist/${file}`, import.meta.url), "utf8");
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
}

const homepage = await readFile(new URL("../dist/index.html", import.meta.url), "utf8");
const jsonLdSource = homepage.match(
  /<script type="application\/ld\+json">([\s\S]*?)<\/script>/,
)?.[1];
assert(jsonLdSource, "homepage must contain JSON-LD");

const schemaTypes = JSON.parse(jsonLdSource).map((entry) => entry["@type"]);
assert.deepEqual(schemaTypes, ["SoftwareApplication", "Organization"]);

const robots = await readFile(new URL("../dist/robots.txt", import.meta.url), "utf8");
assert.match(robots, /^User-agent: \*\nAllow: \//);
assert.match(robots, /Sitemap: https:\/\/reviewgate\.lvtd\.dev\/sitemap-index\.xml/);

const sitemapIndex = await readFile(new URL("../dist/sitemap-index.xml", import.meta.url), "utf8");
assert.match(sitemapIndex, /https:\/\/reviewgate\.lvtd\.dev\/sitemap-0\.xml/);

const sitemap = await readFile(new URL("../dist/sitemap-0.xml", import.meta.url), "utf8");
for (const [, canonical] of expectedPages) {
  assert(sitemap.includes(`<loc>${canonical}</loc>`), `sitemap must include ${canonical}`);
}

console.log("validate-seo: ok");
