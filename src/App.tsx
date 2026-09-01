import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { confirm } from "@tauri-apps/plugin-dialog";
import "./App.css";
import { ChatPanel, formatFileToolResult, formatTerminalToolResult } from "./components/ChatPanel";
import type { TerminalProposal, ToolResult } from "./components/ChatPanel";
import { BackupDialog, BackupEntry } from "./components/BackupDialog";
import { Editor, EditorStatus } from "./components/Editor";
import { ExplorerCreateKind, ProjectExplorer, ProjectStatus } from "./components/ProjectExplorer";
import { WorkspaceHeader } from "./components/WorkspaceHeader";
import { TerminalPanel, TerminalTranscript } from "./components/TerminalPanel";
import { useTranslation } from "./hooks/useTranslation";
import { FileReadResult, OpenedFile, OpenProjectResult, ProjectNode, ProjectTree, RefreshProjectResult, RestoredWorkspace } from "./types/project";
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
  return JSON.stringify(file ? [file.path, file.content, file.savedContent, file.existsOnDisk ?? true] : null);
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

export function reconciledTerminalOpenFile(current: OpenedFile | null, diskContent: string | null): OpenedFile | null {
  if (!current) return null;
  const dirty = current.content !== current.savedContent;
  if (diskContent === null) return dirty ? { ...current, existsOnDisk: false } : null;
  if (dirty) return { ...current, savedContent: diskContent, existsOnDisk: true };
  return { ...current, content: diskContent, savedContent: diskContent, existsOnDisk: true };
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
  const [toolResult, setToolResult] = useState<ToolResult | null>(null);
  const latestFileRead = useRef(0);
  const latestFileSave = useRef(0);
  const openingProject = useRef(false);
  const refreshConfirmationPending = useRef(false);
  const applyingFileProposal = useRef(false);
  const manualFileOperation = useRef(false);
  const workspaceRestore = useRef(0);
  const currentProjectSession = useRef(0);
  const nextToolResultId = useRef(0);
  const currentEditorContext = useRef(editorContextKey(openFile));
  const currentOpenFile = useRef(openFile);
  currentEditorContext.current = editorContextKey(openFile);
  currentOpenFile.current = openFile;
  const isDirty = openFile !== null && (openFile.content !== openFile.savedContent || openFile.existsOnDisk === false);

  useEffect(() => {
    let active = true;
    const requestId = ++workspaceRestore.current;
    setProjectStatus("loading");
    invoke<RestoredWorkspace | null>("restore_workspace")
      .then((restored) => {
        if (!active || requestId !== workspaceRestore.current || !restored) return;
        setProject({ ...restored.project, children: transformProjectTree(restored.project.children) });
        if (restored.openFile) {
          const path = restored.openFile.path;
          setOpenFile({
            name: path.split(/[\\/]/).pop() ?? path,
            path,
            content: restored.openFile.content,
            savedContent: restored.openFile.content,
          });
          setEditorStatus("ready");
        }
        currentProjectSession.current += 1;
        setProjectSession(currentProjectSession.current);
        setProjectStatus("opened");
      })
      .catch(() => {})
      .finally(() => {
        if (!active) return;
        if (requestId !== workspaceRestore.current) return;
        setProjectStatus((status) => status === "loading" ? "idle" : status);
      });
    return () => { active = false; };
  }, []);

  const handleOpenProject = async () => {
    if (openingProject.current || (isDirty && !window.confirm(t("editor.discard_confirm")))) return;
    workspaceRestore.current += 1;
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
          setToolResult(null);
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
      void invoke("remember_project_file", { relativePath: node.path, requestId }).catch(() => {});
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

  const updateTreeAfterManualOperation = (updated: ProjectTree) => {
    setProject({ ...updated, children: transformProjectTree(updated.children) });
    setProjectStatus("opened");
    setProjectError("");
  };

  const handleCreateEntry = async (parentPath: string, kind: ExplorerCreateKind) => {
    if (manualFileOperation.current) return;
    if (kind === "file" && isDirty && !window.confirm(t("editor.discard_confirm"))) return;
    const name = window.prompt(t(kind === "file" ? "files.new_file_prompt" : "files.new_folder_prompt"));
    if (!name) return;
    manualFileOperation.current = true;
    const requestSession = currentProjectSession.current;
    const relativePath = parentPath ? `${parentPath}/${name}` : name;
    setProjectError("");
    try {
      const updated = await invoke<ProjectTree>(kind === "file" ? "create_project_file" : "create_project_directory", kind === "file" ? { relativePath, content: "" } : { relativePath });
      if (!isCurrentProjectSession(requestSession, currentProjectSession.current)) return;
      updateTreeAfterManualOperation(updated);
      if (kind === "file") {
        latestFileRead.current += 1;
        latestFileSave.current += 1;
        setOpenFile({ name, path: relativePath, content: "", savedContent: "" });
        setSelection(null); setEditorStatus("ready"); setEditorError(""); setSaving(false);
        void invoke("remember_project_file", { relativePath, requestId: latestFileRead.current }).catch(() => {});
      }
    } catch (error) {
      if (!isCurrentProjectSession(requestSession, currentProjectSession.current)) return;
      setProjectError(operationError(t("files.operation_error"), error));
      setProjectStatus("error");
    } finally {
      manualFileOperation.current = false;
    }
  };

  const handleRenameEntry = async (node: ProjectNode) => {
    if (manualFileOperation.current) return;
    const name = window.prompt(t("files.rename_prompt"), node.name);
    if (!name || name === node.name) return;
    manualFileOperation.current = true;
    const requestSession = currentProjectSession.current;
    const parent = node.path.split(/[\\/]/).slice(0, -1).join("/");
    const newPath = parent ? `${parent}/${name}` : name;
    try {
      const updated = await invoke<ProjectTree>("rename_project_entry", { relativePath: node.path, newRelativePath: newPath });
      if (!isCurrentProjectSession(requestSession, currentProjectSession.current)) return;
      updateTreeAfterManualOperation(updated);
      if (openFile && (openFile.path === node.path || openFile.path.startsWith(`${node.path}/`))) {
        const renamedPath = `${newPath}${openFile.path.slice(node.path.length)}`;
        setOpenFile({ ...openFile, name: renamedPath.split("/").pop() ?? renamedPath, path: renamedPath });
        setSelection(null);
        void invoke("remember_project_file", { relativePath: renamedPath, requestId: ++latestFileRead.current }).catch(() => {});
      }
    } catch (error) {
      if (!isCurrentProjectSession(requestSession, currentProjectSession.current)) return;
      setProjectError(operationError(t("files.operation_error"), error));
      setProjectStatus("error");
    } finally {
      manualFileOperation.current = false;
    }
  };

  const handleDeleteEntry = async (node: ProjectNode) => {
    if (manualFileOperation.current) return;
    manualFileOperation.current = true;
    const affectsOpenFile = !!openFile && (openFile.path === node.path || openFile.path.startsWith(`${node.path}/`));
    const requestSession = currentProjectSession.current;
    try {
      if (affectsOpenFile && isDirty && !await confirm(t("editor.discard_confirm"), { title: "AutoCoder", kind: "warning" })) return;
      const approved = await confirm(
        t(node.kind === "directory" ? "files.delete_folder_confirm" : "files.delete_file_confirm").replace("{name}", node.name),
        { title: "AutoCoder", kind: "warning" },
      );
      if (!approved) return;
      const updated = await invoke<ProjectTree>("delete_project_entry", { relativePath: node.path });
      if (!isCurrentProjectSession(requestSession, currentProjectSession.current)) return;
      updateTreeAfterManualOperation(updated);
      if (affectsOpenFile) {
        latestFileRead.current += 1;
        latestFileSave.current += 1;
        setOpenFile(null); setSelection(null); setEditorStatus("idle"); setEditorError(""); setSaving(false);
      }
    } catch (error) {
      if (!isCurrentProjectSession(requestSession, currentProjectSession.current)) return;
      setProjectError(operationError(t("files.operation_error"), error));
      setProjectStatus("error");
    } finally {
      manualFileOperation.current = false;
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
    if (backup.isDirectory) {
      setSelection(null);
      setEditorError("");
      return;
    }
    setOpenFile({ name: backup.relativePath.split("/").pop() ?? backup.relativePath, path: backup.relativePath, content: backup.content, savedContent: backup.content });
    setSelection(null);
    setEditorStatus("ready");
    setEditorError("");
    void invoke("remember_project_file", { relativePath: backup.relativePath, requestId: latestFileRead.current }).catch(() => {});
  };

  const handleTerminalCompleted = async (transcript: TerminalTranscript) => {
    const requestSession = currentProjectSession.current;
    let file = currentOpenFile.current;
    try {
      let refreshed = await invoke<RefreshProjectResult>("refresh_project", { openFilePath: file?.path ?? null });
      if (!isCurrentProjectSession(requestSession, currentProjectSession.current)) return;
      // An edit to the same buffer is reconciled against the just-read disk
      // baseline. If another file was opened meanwhile, read that path too so
      // neither the editor nor a following model turn receives mismatched data.
      const latestFile = currentOpenFile.current;
      if (latestFile?.path !== file?.path) {
        file = latestFile;
        refreshed = await invoke<RefreshProjectResult>("refresh_project", { openFilePath: file?.path ?? null });
        if (!isCurrentProjectSession(requestSession, currentProjectSession.current)) return;
      } else {
        file = latestFile;
      }
      const reconciled = reconciledTerminalOpenFile(file, refreshed.openFileContent);
      // Publish the refreshed tree/editor snapshot in the same React update as the
      // tool result. Chat can only continue after these values reach its props.
      currentOpenFile.current = reconciled;
      currentEditorContext.current = editorContextKey(reconciled);
      setProject({ ...refreshed.project, children: transformProjectTree(refreshed.project.children) });
      setOpenFile(reconciled);
      setSelection(null);
      setEditorStatus(reconciled ? "ready" : "idle");
      setEditorError("");
      setProjectStatus("opened");
      setProjectError("");
      if (transcript.actionId) {
        setToolResult({ id: ++nextToolResultId.current, actionId: transcript.actionId, tool: "terminal", command: transcript.command, content: formatTerminalToolResult(transcript) });
      }
    } catch (error) {
      // Do not advance orchestration with a snapshot that could be stale.
      setProjectError(operationError(t("files.refresh_error"), error));
      setProjectStatus("error");
    }
  };

  return <div className="app-shell"><WorkspaceHeader onOpenBackups={() => setBackupsOpen(true)} backupsDisabled={!project} /><main className="workspace">
    <ProjectExplorer project={project} status={projectStatus} error={projectError} activePath={openFile?.path} onOpenProject={handleOpenProject} onRefreshProject={handleRefreshProject} onOpenFile={handleOpenFile} onCreate={handleCreateEntry} onRename={handleRenameEntry} onDelete={handleDeleteEntry} />
    <section className="center-workspace"><Editor key={`${projectSession}:${openFile?.path ?? ""}`} file={openFile} status={editorStatus} error={editorError} saving={saving} onChange={(content) => setOpenFile((current) => current ? { ...current, content } : current)} onSelectionChange={setSelection} onSave={handleSave} />
    <TerminalPanel key={projectSession} projectOpen={project !== null} proposedCommand={proposedCommand} onCompleted={handleTerminalCompleted} /></section>
    <ChatPanel key={projectSession} openFile={openFile} selection={selection} project={project} toolResult={toolResult} onReviewCommand={setProposedCommand} onApplyProposal={async (proposal) => {
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
          const requestId = ++latestFileRead.current;
          void invoke("remember_project_file", { relativePath: proposal.path, requestId }).catch(() => {});
          setToolResult({ id: ++nextToolResultId.current, content: formatFileToolResult(proposal, "completed") });
        } catch (error) {
          if (!isCurrentProjectSession(requestSession, currentProjectSession.current) || requestEditorContext !== currentEditorContext.current) return;
          setEditorError(operationError(t("editor.create_error"), error));
          setToolResult({ id: ++nextToolResultId.current, content: formatFileToolResult(proposal, "failed", String(error)) });
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
          setToolResult({ id: ++nextToolResultId.current, content: formatFileToolResult(proposal, "completed") });
        } catch (error) {
          if (!isCurrentProjectSession(requestSession, currentProjectSession.current) || requestEditorContext !== currentEditorContext.current) return;
          setEditorError(operationError(t("editor.delete_error"), error));
          setToolResult({ id: ++nextToolResultId.current, content: formatFileToolResult(proposal, "failed", String(error)) });
        }
      } else {
        setOpenFile((current) => current?.path === proposal.path && current.content === proposal.originalContent
          ? { ...current, content: proposal.content }
          : current);
        setSelection(null);
        setToolResult({ id: ++nextToolResultId.current, content: formatFileToolResult(proposal, "completed") });
      }
      } finally {
        applyingFileProposal.current = false;
      }
    }} />
  </main><BackupDialog key={projectSession} open={backupsOpen} onClose={() => setBackupsOpen(false)} onRestored={(backup, updated) => handleRestored(backup, updated, projectSession)} canRestore={() => !isDirty || confirm(t("editor.discard_confirm"), { title: "AutoCoder", kind: "warning" })} /></div>;
}

export default App;
