import { FormEvent, useState } from "react";
import Editor from "@monaco-editor/react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";
import { Language, useTranslation } from "./hooks/useTranslation";

const languageNames: Record<Language, string> = { ru: "Русский", en: "English", he: "עברית" };

type FileTreeNode = {
  name: string;
  path: string;
  kind: "directory" | "file";
  children: FileTreeNode[];
};

type ProjectTree = { name: string; children: FileTreeNode[] };
type ProjectStatus = "idle" | "loading" | "error" | "opened";
type OpenFile = { name: string; path: string; content: string; savedContent: string };

const extensionLanguages: Record<string, string> = {
  c: "c", cpp: "cpp", cc: "cpp", cs: "csharp", css: "css", go: "go", html: "html",
  java: "java", js: "javascript", jsx: "javascript", json: "json", md: "markdown",
  php: "php", py: "python", rb: "ruby", rs: "rust", sh: "shell", sql: "sql",
  ts: "typescript", tsx: "typescript", xml: "xml", yaml: "yaml", yml: "yaml",
};

function editorLanguage(fileName: string) {
  const extension = fileName.includes(".") ? fileName.split(".").pop()?.toLowerCase() ?? "" : "";
  return extensionLanguages[extension] ?? "plaintext";
}

type FileTreeProps = {
  nodes: FileTreeNode[];
  activePath?: string;
  depth?: number;
  onOpenFile: (node: FileTreeNode) => void;
};

function FileTree({ nodes, activePath, depth = 0, onOpenFile }: FileTreeProps) {
  return (
    <ul className="file-tree">
      {nodes.map((node) => (
        <li key={node.path}>
          {node.kind === "file" ? (
            <button
              type="button"
              className={`tree-node file ${activePath === node.path ? "active" : ""}`}
              style={{ paddingInlineStart: `${depth * 16 + 8}px` }}
              onClick={() => onOpenFile(node)}
            >
              {node.name}
            </button>
          ) : (
            <span className="tree-node directory" style={{ paddingInlineStart: `${depth * 16 + 8}px` }}>
              {node.name}
            </span>
          )}
          {node.kind === "directory" && node.children.length > 0 && (
            <FileTree nodes={node.children} activePath={activePath} depth={depth + 1} onOpenFile={onOpenFile} />
          )}
        </li>
      ))}
    </ul>
  );
}

function App() {
  const { t, lang, changeLanguage } = useTranslation();
  const [message, setMessage] = useState("");
  const [messages, setMessages] = useState<string[]>([]);
  const [project, setProject] = useState<ProjectTree | null>(null);
  const [projectStatus, setProjectStatus] = useState<ProjectStatus>("idle");
  const [openFile, setOpenFile] = useState<OpenFile | null>(null);
  const [editorError, setEditorError] = useState("");
  const [saving, setSaving] = useState(false);
  const isDirty = openFile !== null && openFile.content !== openFile.savedContent;

  const handleOpenProject = async () => {
    if (isDirty && !window.confirm(t("editor.discard_confirm"))) return;
    setProjectStatus("loading");
    setEditorError("");
    try {
      const selectedProject = await invoke<ProjectTree | null>("open_project");
      if (selectedProject) {
        setProject(selectedProject);
        setOpenFile(null);
        setProjectStatus("opened");
      } else {
        setProjectStatus(project ? "opened" : "idle");
      }
    } catch {
      setProjectStatus("error");
    }
  };

  const handleOpenFile = async (node: FileTreeNode) => {
    if (node.path === openFile?.path) return;
    if (isDirty && !window.confirm(t("editor.discard_confirm"))) return;
    setEditorError("");
    try {
      const content = await invoke<string>("read_project_file", { relativePath: node.path });
      setOpenFile({ name: node.name, path: node.path, content, savedContent: content });
    } catch {
      setEditorError(t("editor.read_error"));
    }
  };

  const handleSave = async () => {
    if (!openFile || !isDirty) return;
    const contentToSave = openFile.content;
    setSaving(true);
    setEditorError("");
    try {
      await invoke("save_project_file", { relativePath: openFile.path, content: contentToSave });
      setOpenFile((current) => current ? { ...current, savedContent: contentToSave } : current);
    } catch {
      setEditorError(t("editor.save_error"));
    } finally {
      setSaving(false);
    }
  };

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const trimmedMessage = message.trim();
    if (!trimmedMessage) return;
    setMessages((currentMessages) => [...currentMessages, trimmedMessage]);
    setMessage("");
  };

  return (
    <div className="app-shell">
      <header className="app-header">
        <div><h1>{t("app.title")}</h1><p>{t("app.tagline")}</p></div>
        <label className="language-picker">
          <span>{t("settings.language")}</span>
          <select aria-label={t("settings.language")} value={lang} onChange={(event) => changeLanguage(event.target.value as Language)}>
            {Object.entries(languageNames).map(([language, name]) => <option key={language} value={language}>{name}</option>)}
          </select>
        </label>
      </header>

      <main className="workspace">
        <aside className="file-panel" aria-label={t("sidebar.files")}>
          <div className="panel-heading">
            <h2>{t("sidebar.files")}</h2>
            <button type="button" className="open-project-button" onClick={handleOpenProject} disabled={projectStatus === "loading"}>
              {t("files.open_project")}
            </button>
          </div>
          <nav>
            {projectStatus === "loading" && <p className="project-state">{t("files.loading")}</p>}
            {projectStatus === "error" && <p className="project-state error">{t("files.open_error")}</p>}
            {projectStatus === "idle" && <p className="project-state">{t("files.not_opened")}</p>}
            {projectStatus === "opened" && project && (
              <div className="project-tree">
                <p className="project-name">{project.name}</p>
                <FileTree nodes={project.children} activePath={openFile?.path} onOpenFile={handleOpenFile} />
              </div>
            )}
          </nav>
        </aside>

        <section className="editor-panel" aria-labelledby="editor-heading">
          <div className="panel-heading editor-heading">
            <h2 id="editor-heading">
              {openFile?.name ?? t("editor.no_file")}
              {isDirty && <span className="dirty-indicator" title={t("editor.unsaved")} aria-label={t("editor.unsaved")}>●</span>}
            </h2>
            <button type="button" className="save-button" onClick={handleSave} disabled={!isDirty || saving}>
              {saving ? t("editor.saving") : t("editor.save")}
            </button>
          </div>
          {editorError && <p className="editor-error" role="alert">{editorError}</p>}
          {openFile ? (
            <div className="monaco-container">
              <Editor
                path={openFile.path}
                language={editorLanguage(openFile.name)}
                value={openFile.content}
                theme="vs-dark"
                onChange={(value) => setOpenFile((current) => current ? { ...current, content: value ?? "" } : current)}
                options={{ automaticLayout: true, minimap: { enabled: false }, wordWrap: "on" }}
              />
            </div>
          ) : (
            <div className="editor-placeholder">
              <p className="eyebrow">{t("editor.placeholder_label")}</p>
              <h2>{t("editor.no_file")}</h2>
              <p>{t("editor.placeholder_description")}</p>
            </div>
          )}
        </section>

        <aside className="chat-panel" aria-label={t("sidebar.chat")}>
          <div className="panel-heading"><h2>{t("sidebar.chat")}</h2><span className="status-dot">{t("chat.local")}</span></div>
          <div className="chat-messages" aria-live="polite">
            {messages.length === 0 ? <p className="empty-chat">{t("chat.empty")}</p> : messages.map((chatMessage, index) => <p className="user-message" key={`${chatMessage}-${index}`}>{chatMessage}</p>)}
          </div>
          <form className="chat-form" onSubmit={handleSubmit}>
            <textarea aria-label={t("chat.placeholder")} placeholder={t("chat.placeholder")} value={message} onChange={(event) => setMessage(event.target.value)} rows={3} />
            <button type="submit" disabled={!message.trim()}>{t("chat.send")}</button>
          </form>
        </aside>
      </main>
    </div>
  );
}

export default App;
