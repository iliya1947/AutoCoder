import { ProjectNode } from "../types/project";

export const EXCLUDED_DIRECTORIES = new Set([".git", ".idea", ".venv", ".vscode", "dist", "node_modules", "target"]);

export function isExcludedDirectory(name: string): boolean {
  return name.startsWith(".") || EXCLUDED_DIRECTORIES.has(name.toLowerCase());
}

/** Converts an untrusted backend tree into the tree the explorer can safely display. */
export function transformProjectTree(nodes: ProjectNode[]): ProjectNode[] {
  return nodes
    .filter((node) => node.kind === "file" || (node.kind === "directory" && !isExcludedDirectory(node.name)))
    .map((node) => ({
      ...node,
      children: node.kind === "directory" ? transformProjectTree(node.children ?? []) : [],
    }))
    .sort((left, right) => {
      if (left.kind !== right.kind) return left.kind === "directory" ? -1 : 1;
      return left.name.localeCompare(right.name, undefined, { sensitivity: "base" });
    });
}

export const extensionLanguages: Record<string, string> = {
  c: "c", cpp: "cpp", cc: "cpp", cs: "csharp", css: "css", go: "go", html: "html",
  java: "java", js: "javascript", jsx: "javascript", json: "json", md: "markdown",
  php: "php", py: "python", rb: "ruby", rs: "rust", sh: "shell", sql: "sql",
  ts: "typescript", tsx: "typescript", xml: "xml", yaml: "yaml", yml: "yaml",
};

export function editorLanguage(fileName: string): string {
  const extension = fileName.includes(".") ? fileName.split(".").pop()?.toLowerCase() ?? "" : "";
  return extensionLanguages[extension] ?? "plaintext";
}
