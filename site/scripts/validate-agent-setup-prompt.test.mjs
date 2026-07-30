import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const promptUrl = new URL("../src/content/agent-setup-prompt.txt", import.meta.url);

test("agent setup prompt covers local and GitHub installation", async () => {
  const prompt = await readFile(promptUrl, "utf8");

  assert.match(prompt, /ask me which exact OpenRouter model ID/i);
  assert.match(prompt, /Do not continue until I choose/);
  assert.match(prompt, /openrouter\/free/);
  assert.match(prompt, /deepseek\/deepseek-v4-flash/);
  assert.match(prompt, /provider\/model-name/);
  assert.match(prompt, /cargo install --path crates\/reviewgate-cli --locked/);
  assert.match(prompt, /--model <exact-openrouter-model-id>/);
  assert.match(prompt, /LVTD-LLC\/reviewgate@v0/);
  assert.match(prompt, /OPENROUTER_API_KEY/);
  assert.match(prompt, /pull_request_target/);
  assert.doesNotMatch(prompt, /sk-or-[A-Za-z0-9]/);
});
