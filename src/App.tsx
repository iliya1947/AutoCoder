import { FormEvent, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";
import { Language, useTranslation } from "./hooks/useTranslation";

const languageNames: Record<Language, string> = {
  ru: "Русский",
  en: "English",
  he: "עברית",
};

type FileTreeNode = {
  name: string;
  kind: "directory" | "file";
  children: FileTreeNode[];
};

type ProjectTree = {
  name: string;
  children: FileTreeNode[];
};

type ProjectStatus = "idle" | "loading" | "error" | "opened";

type FileTreeProps = {
  nodes: FileTreeNode[];
  depth?: number;
};

function FileTree({ nodes, depth = 0 }: FileTreeProps) {
  return (
    <ul className="file-tree">
      {nodes.map((node) => (
        <li key={`${depth}-${node.kind}-${node.name}`}>
          <span className={`tree-node ${node.kind}`} style={{ paddingInlineStart: `${depth * 16}px` }}>
            {node.name}
          </span>
          {node.kind === "directory" && node.children.length > 0 && (
            <FileTree nodes={node.children} depth={depth + 1} />
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

  const handleOpenProject = async () => {
    setProjectStatus("loading");

    try {
      const selectedProject = await invoke<ProjectTree | null>("open_project");
      if (selectedProject) {
        setProject(selectedProject);
        setProjectStatus("opened");
      } else {
        setProjectStatus(project ? "opened" : "idle");
      }
    } catch {
      setProjectStatus("error");
    }
  };

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const trimmedMessage = message.trim();

    if (!trimmedMessage) {
      return;
    }

    setMessages((currentMessages) => [...currentMessages, trimmedMessage]);
    setMessage("");
  };

  return (
    <div className="app-shell">
      <header className="app-header">
        <div>
          <h1>{t("app.title")}</h1>
          <p>{t("app.tagline")}</p>
        </div>
        <label className="language-picker">
          <span>{t("settings.language")}</span>
          <select
            aria-label={t("settings.language")}
            value={lang}
            onChange={(event) => changeLanguage(event.target.value as Language)}
          >
            {Object.entries(languageNames).map(([language, name]) => (
              <option key={language} value={language}>
                {name}
              </option>
            ))}
          </select>
        </label>
      </header>

      <main className="workspace">
        <aside className="file-panel" aria-label={t("sidebar.files")}>
          <div className="panel-heading">
            <h2>{t("sidebar.files")}</h2>
            <button
              type="button"
              className="open-project-button"
              onClick={handleOpenProject}
              disabled={projectStatus === "loading"}
            >
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
                <FileTree nodes={project.children} />
              </div>
            )}
          </nav>
        </aside>

        <section className="editor-panel" aria-labelledby="editor-heading">
          <div className="panel-heading editor-heading">
            <h2 id="editor-heading">README.md</h2>
            <button type="button" className="save-button">
              {t("editor.save")}
            </button>
          </div>
          <div className="editor-placeholder">
            <p className="eyebrow">{t("editor.placeholder_label")}</p>
            <h2>{t("editor.no_file")}</h2>
            <p>{t("editor.placeholder_description")}</p>
          </div>
        </section>

        <aside className="chat-panel" aria-label={t("sidebar.chat")}>
          <div className="panel-heading">
            <h2>{t("sidebar.chat")}</h2>
            <span className="status-dot">{t("chat.local")}</span>
          </div>
          <div className="chat-messages" aria-live="polite">
            {messages.length === 0 ? (
              <p className="empty-chat">{t("chat.empty")}</p>
            ) : (
              messages.map((chatMessage, index) => (
                <p className="user-message" key={`${chatMessage}-${index}`}>
                  {chatMessage}
                </p>
              ))
            )}
          </div>
          <form className="chat-form" onSubmit={handleSubmit}>
            <textarea
              aria-label={t("chat.placeholder")}
              placeholder={t("chat.placeholder")}
              value={message}
              onChange={(event) => setMessage(event.target.value)}
              rows={3}
            />
            <button type="submit" disabled={!message.trim()}>
              {t("chat.send")}
            </button>
          </form>
        </aside>
      </main>
    </div>
  );
}

export default App;
