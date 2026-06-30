// Copy the committed proof-replay artifacts into the app (proof-data/) so they're bundled on Vercel,
// where `../logs` — outside the app root — isn't traced into the serverless function. The source of
// truth stays the repo-root logs/; this copy is generated at build/dev time and is gitignored.
import { mkdirSync, copyFileSync, existsSync } from "node:fs";
import { join } from "node:path";

const src = join(process.cwd(), "..", "logs");
const dest = join(process.cwd(), "proof-data");
const files = ["lifecycle-log.json", "ai-decisions.json"];

mkdirSync(dest, { recursive: true });
let copied = 0;
for (const f of files) {
  const from = join(src, f);
  if (existsSync(from)) {
    copyFileSync(from, join(dest, f));
    copied++;
  } else {
    console.warn(`copy-proof-data: source missing, skipped: ${from}`);
  }
}
console.log(`copy-proof-data: copied ${copied}/${files.length} artifact(s) into proof-data/`);
