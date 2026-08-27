import { readFileSync, readdirSync, statSync } from "node:fs";
import { resolve } from "node:path";

const forbidden = /cdn\.jsdelivr\.net|unpkg\.com|cdnjs\.cloudflare\.com/i;

function files(path) {
  if (!statSync(path).isDirectory()) return [path];
  return readdirSync(path).flatMap((entry) => files(resolve(path, entry)));
}

const matches = files(resolve("dist"))
  .filter((path) => forbidden.test(readFileSync(path, "utf8")))
  .map((path) => path.slice(process.cwd().length + 1));

if (matches.length) {
  console.error(`Forbidden CDN runtime references found in: ${matches.join(", ")}`);
  process.exit(1);
}

console.log("Offline runtime check passed: production assets contain no forbidden CDN references.");
