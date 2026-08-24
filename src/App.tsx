import { FormEvent, useState } from "react";
import "./App.css";
import { Language, useTranslation } from "./hooks/useTranslation";

const languageNames: Record<Language, string> = {
  ru: "Русский",
  en: "English",
  he: "עברית",
};

function App() {
  const { t, lang, changeLanguage } = useTranslation();
  const [message, setMessage] = useState("");
  const [messages, setMessages] = useState<string[]>([]);

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
            <button type="button" className="icon-button" aria-label={t("files.add")}>
              +
            </button>
          </div>
          <nav>
            <ul className="file-tree">
              <li className="folder">autocoder</li>
              <li className="file nested active">README.md</li>
              <li className="file nested">src</li>
              <li className="file nested-double">App.tsx</li>
              <li className="file nested-double">main.tsx</li>
            </ul>
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
