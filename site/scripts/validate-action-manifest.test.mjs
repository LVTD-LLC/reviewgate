import assert from "node:assert/strict";
import test from "node:test";

import { isMapping } from "./validate-action-manifest.mjs";

test("accepts only YAML mappings", () => {
  assert.equal(isMapping({ using: "composite", steps: [] }), true);
  assert.equal(isMapping(null), false);
  assert.equal(isMapping([]), false);
  assert.equal(isMapping("composite"), false);
});
