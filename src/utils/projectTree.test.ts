import { describe, expect, it } from "vitest";
import { editorLanguage, isExcludedDirectory, transformProjectTree } from "./projectTree";
import { ProjectNode } from "../types/project";

const directory = (name: string, children: ProjectNode[] = []): ProjectNode => ({ name, path: name, kind: "directory", children });
const file = (name: string): ProjectNode => ({ name, path: name, kind: "file", children: [] });

describe("project tree logic", () => {
  it("filters hidden and excluded directories", () => {
    expect([".git", ".cache", "node_modules", "target"].every(isExcludedDirectory)).toBe(true);
    expect(isExcludedDirectory("src")).toBe(false);
  });

  it("uses plaintext for unknown and absent extensions", () => {
    expect(editorLanguage("notes.txt")).toBe("plaintext");
    expect(editorLanguage("data.unknown")).toBe("plaintext");
    expect(editorLanguage("README")).toBe("plaintext");
  });

  it("keeps every file recursively and sorts directories first", () => {
    const result = transformProjectTree([file("z.ts"), directory("src", [file("logo.png"), file("App.tsx")]), directory(".git"), file("a.exe")]);
    expect(result).toEqual([directory("src", [file("App.tsx"), file("logo.png")]), file("a.exe"), file("z.ts")]);
  });
});
