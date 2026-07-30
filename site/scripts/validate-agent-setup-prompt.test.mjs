import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const promptUrl = new URL("../src/content/agent-setup-prompt.txt", import.meta.url);
const homepageUrl = new URL("../src/pages/index.astro", import.meta.url);

test("agent setup prompt gives agents a concise route to current guidance", async () => {
  const prompt = await readFile(promptUrl, "utf8");

  assert(prompt.length < 1_000, "setup prompt should stay under 1,000 characters");
  assert.match(prompt, /https:\/\/reviewgate\.lvtd\.dev\/docs/);
  assert.match(prompt, /https:\/\/reviewgate\.lvtd\.dev\/llms\.txt/);
  assert.match(prompt, /https:\/\/github\.com\/LVTD-LLC\/reviewgate$/m);
  assert.match(
    prompt,
    /https:\/\/github\.com\/LVTD-LLC\/reviewgate\/blob\/main\/README\.md/,
  );
  assert.match(prompt, /ask me which exact OpenRouter model ID/i);
  assert.match(prompt, /deepseek\/deepseek-v4-flash/);
  assert.match(prompt, /OPENROUTER_API_KEY/);
  assert.doesNotMatch(prompt, /cargo install/);
  assert.doesNotMatch(prompt, /\.github\/workflows\/reviewgate\.yml/);
  assert.doesNotMatch(prompt, /sk-or-[A-Za-z0-9]/);
});

test("homepage exposes one setup-prompt copy CTA in the hero", async () => {
  const homepage = await readFile(homepageUrl, "utf8");

  assert.match(
    homepage,
    /<div class="signal-actions">[\s\S]*?<button[\s\S]*?data-copy-agent-prompt[\s\S]*?Copy setup prompt[\s\S]*?<\/button>/,
  );
  assert.match(homepage, /data-agent-setup-prompt=\{encodedAgentSetupPrompt\}/);
  assert.doesNotMatch(homepage, /<section class="agent-setup"/);
  assert.doesNotMatch(homepage, /Install ReviewGate <span/);
});
