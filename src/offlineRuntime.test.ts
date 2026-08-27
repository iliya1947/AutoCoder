import { readFileSync, readdirSync, statSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const runtimeRoots = ["src", "index.html"];
const remoteReference = /(?:https?:)?\/\/(?:[\w-]+\.)+[\w-]+/i;
const cdnReference = /cdn\.jsdelivr\.net|unpkg\.com|cdnjs\.cloudflare\.com/i;

function runtimeFiles(path: string): string[] {
  const absolute = resolve(path);
  if (!statSync(absolute).isDirectory()) return [absolute];
  return readdirSync(absolute).flatMap((entry) => runtimeFiles(resolve(absolute, entry)));
}

describe("offline frontend architecture", () => {
  it("contains no remote runtime asset or network references", () => {
    const references = runtimeRoots
      .flatMap(runtimeFiles)
      .filter((path) => !path.endsWith("offlineRuntime.test.ts"))
      .flatMap((path) => readFileSync(path, "utf8").split("\n")
        .map((line, index) => ({ path, line: index + 1, text: line.trim() }))
        .filter(({ text }) => remoteReference.test(text) || cdnReference.test(text)));

    expect(references, JSON.stringify(references, null, 2)).toEqual([]);
  });

  it("keeps Tauri frontend connections limited to local IPC", () => {
    const config = JSON.parse(readFileSync(resolve("src-tauri/tauri.conf.json"), "utf8"));
    expect(config.app.security.csp).toContain("connect-src ipc: http://ipc.localhost");
    expect(config.app.security.csp).not.toMatch(/connect-src[^;]*https?:\/\/(?!ipc\.localhost)/);
  });
});
