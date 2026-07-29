import { readFileSync } from "node:fs";
import { parseDocument } from "yaml";

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
  manifest === null ||
  typeof manifest !== "object" ||
  typeof manifest.name !== "string" ||
  typeof manifest.runs !== "object"
) {
  throw new Error("action.yml must define a named action with a runs mapping");
}
