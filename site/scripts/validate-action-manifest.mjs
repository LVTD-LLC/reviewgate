import { readFileSync } from "node:fs";
import { parseDocument } from "yaml";

export function isMapping(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

const manifestUrl = new URL("../../action.yml", import.meta.url);
const document = parseDocument(readFileSync(manifestUrl, "utf8"), {
  prettyErrors: true,
  strict: true,
});

if (document.errors.length > 0) {
  throw new Error(
    `Invalid action.yml:\n${document.errors.map((error) => error.message).join("\n")}`,
  );
}

const manifest = document.toJS();
if (
  !isMapping(manifest) ||
  typeof manifest.name !== "string" ||
  !isMapping(manifest.runs)
) {
  throw new Error("action.yml must define a named action with a runs mapping");
}
