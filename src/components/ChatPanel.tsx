import { FormEvent, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../hooks/useTranslation";
import { OpenedFile } from "../types/project";

export type ChatMessage = { role: "user" | "assistant"; content: string };
type ChatResponse = { message: ChatMessage };
export type ChatRequest = {
  messages: ChatMessage[];
  context: { openFile: { path: string; content: string } } | null;
};

export function buildChatRequest(messages: ChatMessage[], openFile: OpenedFile | null): ChatRequest {
  return {
    messages,
    context: openFile ? { openFile: { path: openFile.path, content: openFile.content } } : null,
  };
}

export function ChatPanel({ openFile }: { openFile: OpenedFile | null }) {
  const { t } = useTranslation();
  const [message, setMessage] = useState("");
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState(false);

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const content = message.trim();
    if (!content || sending) return;
    const nextMessages: ChatMessage[] = [...messages, { role: "user", content }];
    setMessages(nextMessages);
    setMessage("");
    setSending(true);
    setError(false);
    try {
      const response = await invoke<ChatResponse>("send_chat_message", {
        request: buildChatRequest(nextMessages, openFile),
      });
      setMessages((current) => [...current, response.message]);
    } catch {
      setError(true);
    } finally {
      setSending(false);
    }
  };

  return <aside className="chat-panel" aria-label={t("sidebar.chat")}>
    <div className="panel-heading"><h2>{t("sidebar.chat")}</h2><span className="status-dot">{t("chat.ollama")}</span></div>
    <div className="chat-messages" aria-live="polite">
      {messages.length === 0 && !sending ? <p className="empty-chat">{t("chat.empty")}</p> : messages.map((item, index) => <p className={`${item.role}-message`} key={`${item.role}-${index}`}>{item.content}</p>)}
      {sending && <p className="chat-status">{t("chat.sending")}</p>}
      {error && <p className="chat-error" role="alert">{t("chat.error")}</p>}
    </div>
    <form className="chat-form" onSubmit={handleSubmit}><textarea aria-label={t("chat.placeholder")} placeholder={t("chat.placeholder")} value={message} onChange={(event) => setMessage(event.target.value)} rows={3} disabled={sending} /><button type="submit" disabled={!message.trim() || sending}>{t("chat.send")}</button></form>
  </aside>;
}
