import { ProjectNode } from "../types/project";

export const EXCLUDED_DIRECTORIES = new Set([".git", ".idea", ".venv", ".vscode", "dist", "node_modules", "target"]);

const TEXT_EXTENSIONS = new Set([
  "c", "cc", "cpp", "cs", "css", "go", "h", "hpp", "html", "java", "js", "jsx", "json", "md",
  "php", "py", "rb", "rs", "sh", "sql", "toml", "ts", "tsx", "txt", "xml", "yaml", "yml",
]);
const TEXT_FILE_NAMES = new Set(["dockerfile", "license", "makefile", "readme"]);

export function isExcludedDirectory(name: string): boolean {
  return name.startsWith(".") || EXCLUDED_DIRECTORIES.has(name.toLowerCase());
}

export function isSupportedTextFile(name: string): boolean {
  const normalized = name.toLowerCase();
  if (TEXT_FILE_NAMES.has(normalized)) return true;
  const dot = normalized.lastIndexOf(".");
  return dot > 0 && TEXT_EXTENSIONS.has(normalized.slice(dot + 1));
}

/** Converts an untrusted backend tree into the tree the explorer can safely display. */
export function transformProjectTree(nodes: ProjectNode[]): ProjectNode[] {
  return nodes
    .filter((node) => node.kind === "directory" ? !isExcludedDirectory(node.name) : isSupportedTextFile(node.name))
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
