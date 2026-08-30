import { useEffect, useState } from "react";
import { useTranslation } from "../hooks/useTranslation";
import { ProjectNode, ProjectTree } from "../types/project";

export type ProjectStatus = "idle" | "loading" | "error" | "opened";
export type ExplorerCreateKind = "file" | "directory";

function FileTree({ nodes, activePath, selectedPath, depth = 0, onOpenFile, onSelect }: {
  nodes: ProjectNode[]; activePath?: string; selectedPath?: string; depth?: number;
  onOpenFile: (node: ProjectNode) => void; onSelect: (node: ProjectNode) => void;
}) {
  return <ul className="file-tree">{nodes.map((node) => <li key={node.path}>
    <button type="button" className={`tree-node ${node.kind} ${activePath === node.path ? "active" : ""} ${selectedPath === node.path ? "selected" : ""}`}
      style={{ paddingInlineStart: `${depth * 16 + 8}px` }}
      onClick={() => { onSelect(node); if (node.kind === "file") onOpenFile(node); }}
      title={node.path}>{node.name}</button>
    {node.kind === "directory" && node.children.length > 0 && <FileTree nodes={node.children} activePath={activePath} selectedPath={selectedPath} depth={depth + 1} onOpenFile={onOpenFile} onSelect={onSelect} />}
  </li>)}</ul>;
}

export function ProjectExplorer({ project, status, error, activePath, onOpenProject, onRefreshProject, onOpenFile, onCreate, onRename, onDelete }: {
  project: ProjectTree | null; status: ProjectStatus; error?: string; activePath?: string;
  onOpenProject: () => void; onRefreshProject?: () => void; onOpenFile: (node: ProjectNode) => void;
  onCreate?: (parentPath: string, kind: ExplorerCreateKind) => void;
  onRename?: (node: ProjectNode) => void; onDelete?: (node: ProjectNode) => void;
}) {
  const { t } = useTranslation();
  const [selected, setSelected] = useState<ProjectNode | null>(null);
  useEffect(() => setSelected(null), [project]);
  const parentPath = selected?.kind === "directory" ? selected.path : selected?.path.split(/[\\/]/).slice(0, -1).join("/") ?? "";
  return <aside className="file-panel" aria-label={t("sidebar.files")}>
    <div className="panel-heading"><h2>{t("sidebar.files")}</h2><div className="project-actions">{project && <button type="button" className="open-project-button" onClick={onRefreshProject} disabled={status === "loading"}>{t("files.refresh_project")}</button>}<button type="button" className="open-project-button" onClick={onOpenProject} disabled={status === "loading"}>{t("files.open_project")}</button></div></div>
    {project && <div className="explorer-actions" aria-label={t("files.actions")}>
      <button type="button" onClick={() => onCreate?.(parentPath, "file")} disabled={status === "loading"}>{t("files.new_file")}</button>
      <button type="button" onClick={() => onCreate?.(parentPath, "directory")} disabled={status === "loading"}>{t("files.new_folder")}</button>
      <button type="button" onClick={() => selected && onRename?.(selected)} disabled={!selected || status === "loading"}>{t("files.rename")}</button>
      <button type="button" className="danger" onClick={() => selected && onDelete?.(selected)} disabled={!selected || status === "loading"}>{t("files.delete")}</button>
    </div>}
    <nav>
      {status === "loading" && <p className="project-state" role="status">{t("files.loading")}</p>}
      {status === "error" && <p className="project-state error" role="alert">{error || t("files.open_error")}</p>}
      {status === "idle" && !project && <p className="project-state">{t("files.not_opened")}</p>}
      {project && <div className="project-tree"><p className="project-name" onClick={() => setSelected(null)}>{project.name}</p>{project.children.length ? <FileTree nodes={project.children} activePath={activePath} selectedPath={selected?.path} onOpenFile={onOpenFile} onSelect={setSelected} /> : <p className="project-state">{t("files.no_supported_files")}</p>}</div>}
    </nav>
  </aside>;
}
