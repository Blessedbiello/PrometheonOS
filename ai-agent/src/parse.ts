/** Extract and validate an `LlmDecision` from a model's text response. */
import { llmDecisionSchema, type LlmDecisionParsed } from "./schema.js";

/**
 * Parse model output into a validated `LlmDecision`. Tolerates markdown code fences and prose
 * around the JSON object. Throws if no valid object is found or it fails the schema.
 */
export function parseLlmDecision(text: string): LlmDecisionParsed {
  const json = extractJsonObject(text);
  if (json === null) {
    throw new Error("no JSON object found in model output");
  }
  return llmDecisionSchema.parse(JSON.parse(json));
}

/** Return the first balanced top-level `{...}` substring, stripping ``` fences first. */
function extractJsonObject(text: string): string | null {
  const stripped = text.replace(/```(?:json)?/gi, "").trim();
  const start = stripped.indexOf("{");
  if (start === -1) return null;
  let depth = 0;
  let inString = false;
  let escaped = false;
  for (let i = start; i < stripped.length; i++) {
    const ch = stripped[i];
    if (inString) {
      if (escaped) escaped = false;
      else if (ch === "\\") escaped = true;
      else if (ch === '"') inString = false;
      continue;
    }
    if (ch === '"') inString = true;
    else if (ch === "{") depth++;
    else if (ch === "}") {
      depth--;
      if (depth === 0) return stripped.slice(start, i + 1);
    }
  }
  return null;
}
