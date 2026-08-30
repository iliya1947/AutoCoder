import { useTranslation } from "../hooks/useTranslation";
import { ProjectNode, ProjectTree } from "../types/project";

export type ProjectStatus = "idle" | "loading" | "error" | "opened";

function FileTree({ nodes, activePath, depth = 0, onOpenFile }: { nodes: ProjectNode[]; activePath?: string; depth?: number; onOpenFile: (node: ProjectNode) => void }) {
  return <ul className="file-tree">{nodes.map((node) => <li key={node.path}>
    {node.kind === "file" ? <button type="button" className={`tree-node file ${activePath === node.path ? "active" : ""}`} style={{ paddingInlineStart: `${depth * 16 + 8}px` }} onClick={() => onOpenFile(node)}>{node.name}</button>
      : <span className="tree-node directory" style={{ paddingInlineStart: `${depth * 16 + 8}px` }}>{node.name}</span>}
    {node.kind === "directory" && node.children.length > 0 && <FileTree nodes={node.children} activePath={activePath} depth={depth + 1} onOpenFile={onOpenFile} />}
  </li>)}</ul>;
}

export function ProjectExplorer({ project, status, error, activePath, onOpenProject, onRefreshProject, onOpenFile }: { project: ProjectTree | null; status: ProjectStatus; error?: string; activePath?: string; onOpenProject: () => void; onRefreshProject?: () => void; onOpenFile: (node: ProjectNode) => void }) {
  const { t } = useTranslation();
  return <aside className="file-panel" aria-label={t("sidebar.files")}>
    <div className="panel-heading"><h2>{t("sidebar.files")}</h2><div className="project-actions">{project && <button type="button" className="open-project-button" onClick={onRefreshProject} disabled={status === "loading"}>{t("files.refresh_project")}</button>}<button type="button" className="open-project-button" onClick={onOpenProject} disabled={status === "loading"}>{t("files.open_project")}</button></div></div>
    <nav>
      {status === "loading" && <p className="project-state" role="status">{t("files.loading")}</p>}
      {status === "error" && <p className="project-state error" role="alert">{error || t("files.open_error")}</p>}
      {status === "idle" && !project && <p className="project-state">{t("files.not_opened")}</p>}
      {project && <div className="project-tree"><p className="project-name">{project.name}</p>{project.children.length ? <FileTree nodes={project.children} activePath={activePath} onOpenFile={onOpenFile} /> : <p className="project-state">{t("files.no_supported_files")}</p>}</div>}
    </nav>
  </aside>;
}
