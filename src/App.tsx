import { useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { confirm } from "@tauri-apps/plugin-dialog";
import "./App.css";
import { ChatPanel } from "./components/ChatPanel";
import type { TerminalProposal, TerminalResultDraft } from "./components/ChatPanel";
import { BackupDialog, BackupEntry } from "./components/BackupDialog";
import { Editor, EditorStatus } from "./components/Editor";
import { ProjectExplorer, ProjectStatus } from "./components/ProjectExplorer";
import { WorkspaceHeader } from "./components/WorkspaceHeader";
import { TerminalPanel } from "./components/TerminalPanel";
import { useTranslation } from "./hooks/useTranslation";
import { FileReadResult, OpenedFile, OpenProjectResult, ProjectNode, ProjectTree, RefreshProjectResult } from "./types/project";
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

export function nextProjectSession(currentSession: number, sessionChanged: boolean): number {
  return sessionChanged ? currentSession + 1 : currentSession;
}

export function editorContextKey(file: OpenedFile | null): string {
  return JSON.stringify(file ? [file.path, file.content, file.savedContent] : null);
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

export function refreshedOpenFile(current: OpenedFile | null, content: string | null): OpenedFile | null {
  return current && content !== null ? { ...current, content, savedContent: content } : null;
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
  const openingProject = useRef(false);
  const refreshConfirmationPending = useRef(false);
  const applyingFileProposal = useRef(false);
  const currentProjectSession = useRef(0);
  const currentEditorContext = useRef(editorContextKey(openFile));
  currentEditorContext.current = editorContextKey(openFile);
  const isDirty = openFile !== null && openFile.content !== openFile.savedContent;

  const handleOpenProject = async () => {
    if (openingProject.current || (isDirty && !window.confirm(t("editor.discard_confirm")))) return;
    openingProject.current = true;
    setProjectError("");
    setProjectStatus("loading");
    try {
      const selected = await invoke<OpenProjectResult | null>("open_project");
      if (selected) {
        const transformed = { ...selected.project, children: transformProjectTree(selected.project.children) };
        setProject(transformed);
        if (selected.sessionChanged) {
          latestFileRead.current += 1;
          latestFileSave.current += 1;
          currentProjectSession.current = nextProjectSession(currentProjectSession.current, selected.sessionChanged);
          setProjectSession(currentProjectSession.current);
          setProposedCommand(null);
          setTerminalResultDraft(null);
          setOpenFile(null);
          setSelection(null);
          setEditorStatus("idle");
          setSaving(false);
        }
        setProjectStatus("opened");
      } else setProjectStatus(project ? "opened" : "idle");
    } catch (error) {
      setProjectError(operationError(t("files.open_error"), error));
      setProjectStatus("error");
    } finally {
      openingProject.current = false;
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

  const handleRefreshProject = async () => {
    if (!project || openingProject.current || saving || refreshConfirmationPending.current) return;
    if (isDirty) {
      refreshConfirmationPending.current = true;
      let shouldDiscard = false;
      try {
        shouldDiscard = await confirm(t("editor.refresh_discard_confirm"), {
          title: "AutoCoder",
          kind: "warning",
        });
      } catch (error) {
        setProjectError(operationError(t("files.refresh_error"), error));
      } finally {
        refreshConfirmationPending.current = false;
      }
      if (!shouldDiscard) return;
    }
    openingProject.current = true;
    const requestEditorContext = currentEditorContext.current;
    latestFileRead.current += 1;
    latestFileSave.current += 1;
    setProjectError("");
    setProjectStatus("loading");
    try {
      const refreshed = await invoke<RefreshProjectResult>("refresh_project", { openFilePath: openFile?.path ?? null });
      setProject({ ...refreshed.project, children: transformProjectTree(refreshed.project.children) });
      if (requestEditorContext === currentEditorContext.current) {
        setOpenFile((current) => refreshedOpenFile(current, refreshed.openFileContent));
        setSelection(null);
        setEditorStatus(refreshed.openFileContent === null ? "idle" : "ready");
        setEditorError("");
      }
      setProjectStatus("opened");
    } catch (error) {
      setProjectError(operationError(t("files.refresh_error"), error));
      setProjectStatus("error");
    } finally {
      openingProject.current = false;
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
    <ProjectExplorer project={project} status={projectStatus} error={projectError} activePath={openFile?.path} onOpenProject={handleOpenProject} onRefreshProject={handleRefreshProject} onOpenFile={handleOpenFile} />
    <section className="center-workspace"><Editor file={openFile} status={editorStatus} error={editorError} saving={saving} onChange={(content) => setOpenFile((current) => current ? { ...current, content } : current)} onSelectionChange={setSelection} onSave={handleSave} />
    <TerminalPanel key={projectSession} projectOpen={project !== null} proposedCommand={proposedCommand} onReviewResult={(transcript) => setTerminalResultDraft({ ...transcript, id: Date.now() })} /></section>
    <ChatPanel key={projectSession} openFile={openFile} selection={selection} project={project} terminalResultDraft={terminalResultDraft} onReviewCommand={setProposedCommand} onApplyProposal={async (proposal) => {
      if (applyingFileProposal.current) return;
      applyingFileProposal.current = true;
      const requestSession = currentProjectSession.current;
      const requestEditorContext = currentEditorContext.current;
      try {
      if (proposal.operation === "create") {
        try {
          const updated = await invoke<ProjectTree>("create_project_file", { relativePath: proposal.path, content: proposal.content });
          if (!isCurrentProjectSession(requestSession, currentProjectSession.current)) return;
          setProject({ ...updated, children: transformProjectTree(updated.children) });
          if (requestEditorContext !== currentEditorContext.current) return;
          setOpenFile({ name: proposal.path.split(/[\\/]/).pop() ?? proposal.path, path: proposal.path, content: proposal.content, savedContent: proposal.content });
          setSelection(null);
          setEditorStatus("ready");
          setEditorError("");
        } catch (error) {
          if (!isCurrentProjectSession(requestSession, currentProjectSession.current) || requestEditorContext !== currentEditorContext.current) return;
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
          if (requestEditorContext !== currentEditorContext.current) return;
          setOpenFile(null);
          setSelection(null);
          setEditorStatus("idle");
          setEditorError("");
        } catch (error) {
          if (!isCurrentProjectSession(requestSession, currentProjectSession.current) || requestEditorContext !== currentEditorContext.current) return;
          setEditorError(operationError(t("editor.delete_error"), error));
        }
      } else {
        setOpenFile((current) => current?.path === proposal.path && current.content === proposal.originalContent
          ? { ...current, content: proposal.content }
          : current);
        setSelection(null);
      }
      } finally {
        applyingFileProposal.current = false;
      }
    }} />
  </main><BackupDialog key={projectSession} open={backupsOpen} onClose={() => setBackupsOpen(false)} onRestored={(backup, updated) => handleRestored(backup, updated, projectSession)} canRestore={() => !isDirty || window.confirm(t("editor.discard_confirm"))} /></div>;
}

export default App;
