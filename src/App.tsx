import { useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";
import { ChatPanel } from "./components/ChatPanel";
import type { TerminalProposal, TerminalResultDraft } from "./components/ChatPanel";
import { BackupDialog, BackupEntry } from "./components/BackupDialog";
import { Editor, EditorStatus } from "./components/Editor";
import { ProjectExplorer, ProjectStatus } from "./components/ProjectExplorer";
import { WorkspaceHeader } from "./components/WorkspaceHeader";
import { TerminalPanel } from "./components/TerminalPanel";
import { useTranslation } from "./hooks/useTranslation";
import { FileReadResult, OpenedFile, ProjectNode, ProjectTree } from "./types/project";
import { transformProjectTree } from "./utils/projectTree";
import { operationError } from "./utils/invokeError";

export function isLatestFileRead(requestId: number, latestRequestId: number): boolean {
  return requestId === latestRequestId;
}

export function isLatestFileSave(requestId: number, latestRequestId: number): boolean {
  return requestId === latestRequestId;
}

export function isCurrentProjectSession(requestSession: number, currentSession: number): boolean {
  return requestSession === currentSession;
}

export function markFileSaved(
  current: OpenedFile | null,
  path: string,
  expectedContent: string,
  savedContent: string,
): OpenedFile | null {
  return current?.path === path && current.savedContent === expectedContent
    ? { ...current, savedContent }
    : current;
}

function App() {
  const { t } = useTranslation();
  const [project, setProject] = useState<ProjectTree | null>(null);
  const [projectSession, setProjectSession] = useState(0);
  const [projectStatus, setProjectStatus] = useState<ProjectStatus>("idle");
  const [projectError, setProjectError] = useState("");
  const [openFile, setOpenFile] = useState<OpenedFile | null>(null);
  const [selection, setSelection] = useState<string | null>(null);
  const [editorStatus, setEditorStatus] = useState<EditorStatus>("idle");
  const [editorError, setEditorError] = useState("");
  const [saving, setSaving] = useState(false);
  const [backupsOpen, setBackupsOpen] = useState(false);
  const [proposedCommand, setProposedCommand] = useState<TerminalProposal | null>(null);
  const [terminalResultDraft, setTerminalResultDraft] = useState<TerminalResultDraft | null>(null);
  const latestFileRead = useRef(0);
  const latestFileSave = useRef(0);
  const currentProjectSession = useRef(0);
  const isDirty = openFile !== null && openFile.content !== openFile.savedContent;

  const handleOpenProject = async () => {
    if (isDirty && !window.confirm(t("editor.discard_confirm"))) return;
    setProjectError("");
    setProjectStatus("loading");
    try {
      const selected = await invoke<ProjectTree | null>("open_project");
      if (selected) {
        latestFileRead.current += 1;
        latestFileSave.current += 1;
        const transformed = { ...selected, children: transformProjectTree(selected.children) };
        setProject(transformed);
        currentProjectSession.current += 1;
        setProjectSession(currentProjectSession.current);
        setProposedCommand(null);
        setTerminalResultDraft(null);
        setOpenFile(null);
        setSelection(null);
        setEditorStatus("idle");
        setSaving(false);
        setProjectStatus("opened");
      } else setProjectStatus(project ? "opened" : "idle");
    } catch (error) {
      setProjectError(error instanceof Error ? error.message : String(error));
      setProjectStatus("error");
    }
  };

  const handleOpenFile = async (node: ProjectNode) => {
    if (node.path === openFile?.path || (isDirty && !window.confirm(t("editor.discard_confirm")))) return;
    setOpenFile(null);
    setSelection(null);
    setEditorError("");
    setEditorStatus("loading");
    const requestId = ++latestFileRead.current;
    try {
      const result = await invoke<FileReadResult>("read_project_file", { relativePath: node.path });
      if (!isLatestFileRead(requestId, latestFileRead.current)) return;
      setOpenFile({ name: node.name, path: node.path, content: result.content, savedContent: result.content });
      setEditorStatus("ready");
    } catch (error) {
      if (!isLatestFileRead(requestId, latestFileRead.current)) return;
      setEditorError(operationError(t("editor.read_error"), error));
      setEditorStatus("error");
    }
  };

  const handleSave = async () => {
    if (!openFile || !isDirty) return;
    const content = openFile.content;
    const expectedContent = openFile.savedContent;
    const requestId = ++latestFileSave.current;
    setSaving(true);
    setEditorError("");
    try {
      await invoke("save_project_file", { relativePath: openFile.path, content, expectedContent });
      if (!isLatestFileSave(requestId, latestFileSave.current)) return;
      setOpenFile((current) => markFileSaved(current, openFile.path, expectedContent, content));
    } catch (error) {
      if (isLatestFileSave(requestId, latestFileSave.current)) {
        setEditorError(operationError(t("editor.save_error"), error));
      }
    } finally {
      if (isLatestFileSave(requestId, latestFileSave.current)) setSaving(false);
    }
  };

  const handleRestored = (backup: BackupEntry, updated: ProjectTree, requestSession: number) => {
    if (!isCurrentProjectSession(requestSession, currentProjectSession.current)) return;
    latestFileRead.current += 1;
    latestFileSave.current += 1;
    setSaving(false);
    setProject({ ...updated, children: transformProjectTree(updated.children) });
    setOpenFile({ name: backup.relativePath.split("/").pop() ?? backup.relativePath, path: backup.relativePath, content: backup.content, savedContent: backup.content });
    setSelection(null);
    setEditorStatus("ready");
    setEditorError("");
  };

  return <div className="app-shell"><WorkspaceHeader onOpenBackups={() => setBackupsOpen(true)} backupsDisabled={!project} /><main className="workspace">
    <ProjectExplorer project={project} status={projectStatus} error={projectError} activePath={openFile?.path} onOpenProject={handleOpenProject} onOpenFile={handleOpenFile} />
    <section className="center-workspace"><Editor file={openFile} status={editorStatus} error={editorError} saving={saving} onChange={(content) => setOpenFile((current) => current ? { ...current, content } : current)} onSelectionChange={setSelection} onSave={handleSave} />
    <TerminalPanel key={projectSession} projectOpen={project !== null} proposedCommand={proposedCommand} onReviewResult={(transcript) => setTerminalResultDraft({ ...transcript, id: Date.now() })} /></section>
    <ChatPanel key={projectSession} openFile={openFile} selection={selection} project={project} terminalResultDraft={terminalResultDraft} onReviewCommand={setProposedCommand} onApplyProposal={async (proposal) => {
      const requestSession = currentProjectSession.current;
      if (proposal.operation === "create") {
        try {
          const updated = await invoke<ProjectTree>("create_project_file", { relativePath: proposal.path, content: proposal.content });
          if (!isCurrentProjectSession(requestSession, currentProjectSession.current)) return;
          setProject({ ...updated, children: transformProjectTree(updated.children) });
          setOpenFile({ name: proposal.path.split(/[\\/]/).pop() ?? proposal.path, path: proposal.path, content: proposal.content, savedContent: proposal.content });
          setEditorStatus("ready");
          setEditorError("");
        } catch (error) {
          if (!isCurrentProjectSession(requestSession, currentProjectSession.current)) return;
          setEditorError(operationError(t("editor.create_error"), error));
        }
      } else if (proposal.operation === "delete") {
        try {
          const updated = await invoke<ProjectTree>("delete_project_file", {
            relativePath: proposal.path,
            expectedContent: proposal.expectedSavedContent,
          });
          if (!isCurrentProjectSession(requestSession, currentProjectSession.current)) return;
          setProject({ ...updated, children: transformProjectTree(updated.children) });
          setOpenFile(null);
          setEditorStatus("idle");
          setEditorError("");
        } catch (error) {
          if (!isCurrentProjectSession(requestSession, currentProjectSession.current)) return;
          setEditorError(operationError(t("editor.delete_error"), error));
        }
      } else {
        setOpenFile((current) => current?.path === proposal.path && current.content === proposal.originalContent
          ? { ...current, content: proposal.content }
          : current);
      }
      setSelection(null);
    }} />
  </main><BackupDialog open={backupsOpen} onClose={() => setBackupsOpen(false)} onRestored={(backup, updated) => handleRestored(backup, updated, projectSession)} canRestore={() => !isDirty || window.confirm(t("editor.discard_confirm"))} /></div>;
}

export default App;
