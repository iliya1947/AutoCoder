import { describe, expect, it } from "vitest";
import { isExcludedDirectory, isSupportedTextFile, transformProjectTree } from "./projectTree";
import { ProjectNode } from "../types/project";

const directory = (name: string, children: ProjectNode[] = []): ProjectNode => ({ name, path: name, kind: "directory", children });
const file = (name: string): ProjectNode => ({ name, path: name, kind: "file", children: [] });

describe("project tree logic", () => {
  it("filters hidden and excluded directories", () => {
    expect([".git", ".cache", "node_modules", "target"].every(isExcludedDirectory)).toBe(true);
    expect(isExcludedDirectory("src")).toBe(false);
  });

  it("selects supported text files", () => {
    expect(["App.tsx", "README", "Dockerfile", "config.toml"].every(isSupportedTextFile)).toBe(true);
    expect(["photo.png", "archive.zip", ".env"].some(isSupportedTextFile)).toBe(false);
  });

  it("transforms recursively, filters files, and sorts directories first", () => {
    const result = transformProjectTree([file("z.ts"), directory("src", [file("logo.png"), file("App.tsx")]), directory(".git"), file("a.exe")]);
    expect(result).toEqual([directory("src", [file("App.tsx")]), file("z.ts")]);
  });
});
