import { FormEvent, useState } from "react";
import { useTranslation } from "../hooks/useTranslation";

export function ChatPanel() {
  const { t } = useTranslation();
  const [message, setMessage] = useState("");
  const [messages, setMessages] = useState<string[]>([]);
  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const value = message.trim();
    if (!value) return;
    setMessages((current) => [...current, value]);
    setMessage("");
  };
  return <aside className="chat-panel" aria-label={t("sidebar.chat")}>
    <div className="panel-heading"><h2>{t("sidebar.chat")}</h2><span className="status-dot">{t("chat.local")}</span></div>
    <div className="chat-messages" aria-live="polite">{messages.length === 0 ? <p className="empty-chat">{t("chat.empty")}</p> : messages.map((value, index) => <p className="user-message" key={`${value}-${index}`}>{value}</p>)}</div>
    <form className="chat-form" onSubmit={handleSubmit}><textarea aria-label={t("chat.placeholder")} placeholder={t("chat.placeholder")} value={message} onChange={(event) => setMessage(event.target.value)} rows={3} /><button type="submit" disabled={!message.trim()}>{t("chat.send")}</button></form>
  </aside>;
}
