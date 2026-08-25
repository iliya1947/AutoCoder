import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";
import { ChatPanel } from "./components/ChatPanel";
import { Editor, EditorStatus } from "./components/Editor";
import { ProjectExplorer, ProjectStatus } from "./components/ProjectExplorer";
import { WorkspaceHeader } from "./components/WorkspaceHeader";
import { useTranslation } from "./hooks/useTranslation";
import { FileReadResult, OpenedFile, ProjectNode, ProjectTree } from "./types/project";
import { transformProjectTree } from "./utils/projectTree";

function App() {
  const { t } = useTranslation();
  const [project, setProject] = useState<ProjectTree | null>(null);
  const [projectStatus, setProjectStatus] = useState<ProjectStatus>("idle");
  const [openFile, setOpenFile] = useState<OpenedFile | null>(null);
  const [editorStatus, setEditorStatus] = useState<EditorStatus>("idle");
  const [editorError, setEditorError] = useState("");
  const [saving, setSaving] = useState(false);
  const isDirty = openFile !== null && openFile.content !== openFile.savedContent;

  const handleOpenProject = async () => {
    if (isDirty && !window.confirm(t("editor.discard_confirm"))) return;
    setProjectStatus("loading");
    try {
      const selected = await invoke<ProjectTree | null>("open_project");
      if (selected) {
        setProject({ ...selected, children: transformProjectTree(selected.children) });
        setOpenFile(null);
        setEditorStatus("idle");
        setProjectStatus("opened");
      } else setProjectStatus(project ? "opened" : "idle");
    } catch { setProjectStatus("error"); }
  };

  const handleOpenFile = async (node: ProjectNode) => {
    if (node.path === openFile?.path || (isDirty && !window.confirm(t("editor.discard_confirm")))) return;
    setOpenFile(null);
    setEditorError("");
    setEditorStatus("loading");
    try {
      const result = await invoke<FileReadResult>("read_project_file", { relativePath: node.path });
      setOpenFile({ name: node.name, path: node.path, content: result.content, savedContent: result.content });
      setEditorStatus("ready");
    } catch { setEditorError(t("editor.read_error")); setEditorStatus("error"); }
  };

  const handleSave = async () => {
    if (!openFile || !isDirty) return;
    const content = openFile.content;
    setSaving(true);
    setEditorError("");
    try {
      await invoke("save_project_file", { relativePath: openFile.path, content });
      setOpenFile((current) => current ? { ...current, savedContent: content } : current);
    } catch { setEditorError(t("editor.save_error")); }
    finally { setSaving(false); }
  };

  return <div className="app-shell"><WorkspaceHeader /><main className="workspace">
    <ProjectExplorer project={project} status={projectStatus} activePath={openFile?.path} onOpenProject={handleOpenProject} onOpenFile={handleOpenFile} />
    <Editor file={openFile} status={editorStatus} error={editorError} saving={saving} onChange={(content) => setOpenFile((current) => current ? { ...current, content } : current)} onSave={handleSave} />
    <ChatPanel />
  </main></div>;
}

export default App;
