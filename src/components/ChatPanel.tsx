import { FormEvent, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../hooks/useTranslation";
import { OpenedFile, ProjectNode, ProjectTree } from "../types/project";

export type ChatMessage = { role: "user" | "assistant"; content: string };
type ChatResponse = { message: ChatMessage };
export type ChatRequest = {
  messages: ChatMessage[];
  context: {
    openFile?: { path: string; content: string };
    selection?: { path: string; content: string };
    project?: { name: string; entries: string[] };
  } | null;
};

export function chatContextKey(openFile: OpenedFile | null, selection: string | null, project: ProjectTree | null): string {
  return JSON.stringify([project?.name ?? null, openFile?.path ?? null, selection]);
}

export function messagesForCurrentContext(
  messages: ChatMessage[],
  content: string,
  previousContextKey: string | null,
  currentContextKey: string,
): ChatMessage[] {
  const history = previousContextKey === currentContextKey ? messages : [];
  return [...history, { role: "user", content }];
}

function projectEntries(nodes: ProjectNode[]): string[] {
  return nodes.flatMap((node) => [
    `${node.kind === "directory" ? "directory" : "file"}: ${node.path}`,
    ...(node.kind === "directory" ? projectEntries(node.children) : []),
  ]);
}

export function buildChatRequest(
  messages: ChatMessage[],
  openFile: OpenedFile | null,
  selection: string | null,
  project: ProjectTree | null,
): ChatRequest {
  const context: NonNullable<ChatRequest["context"]> = {};
  if (openFile) context.openFile = { path: openFile.path, content: openFile.content };
  if (openFile && selection) context.selection = { path: openFile.path, content: selection };
  if (project) context.project = { name: project.name, entries: projectEntries(project.children) };
  return {
    messages,
    context: Object.keys(context).length > 0 ? context : null,
  };
}

export function ChatPanel({ openFile, selection, project }: { openFile: OpenedFile | null; selection: string | null; project: ProjectTree | null }) {
  const { t } = useTranslation();
  const [message, setMessage] = useState("");
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const lastRequestContext = useRef<string | null>(null);

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const content = message.trim();
    if (!content || sending) return;
    const contextKey = chatContextKey(openFile, selection, project);
    const requestMessages = messagesForCurrentContext(messages, content, lastRequestContext.current, contextKey);
    lastRequestContext.current = contextKey;
    setMessages((current) => [...current, { role: "user", content }]);
    setMessage("");
    setSending(true);
    setError(null);
    try {
      const response = await invoke<ChatResponse>("send_chat_message", {
        request: buildChatRequest(requestMessages, openFile, selection, project),
      });
      setMessages((current) => [...current, response.message]);
    } catch (error) {
      console.error("send_chat_message failed", error);
      setError(error instanceof Error ? error.message : String(error));
    } finally {
      setSending(false);
    }
  };

  return <aside className="chat-panel" aria-label={t("sidebar.chat")}>
    <div className="panel-heading"><h2>{t("sidebar.chat")}</h2><span className="status-dot">{t("chat.ollama")}</span></div>
    <div className="chat-messages" aria-live="polite">
      {messages.length === 0 && !sending ? <p className="empty-chat">{t("chat.empty")}</p> : messages.map((item, index) => <p className={`${item.role}-message`} key={`${item.role}-${index}`}>{item.content}</p>)}
      {sending && <p className="chat-status">{t("chat.sending")}</p>}
      {error && <p className="chat-error" role="alert">
        {t("chat.error")}
        {import.meta.env.DEV && <><br /><code>{error}</code></>}
      </p>}
    </div>
    <form className="chat-form" onSubmit={handleSubmit}><textarea aria-label={t("chat.placeholder")} placeholder={t("chat.placeholder")} value={message} onChange={(event) => setMessage(event.target.value)} rows={3} disabled={sending} /><button type="submit" disabled={!message.trim() || sending}>{t("chat.send")}</button></form>
  </aside>;
}
