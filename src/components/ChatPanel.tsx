import { FormEvent, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../hooks/useTranslation";
import { OpenedFile, ProjectNode, ProjectTree } from "../types/project";

export type ChatMessage = { role: "user" | "assistant"; content: string };
export type FileProposal = { path: string; content: string; originalContent: string };
export type DiffLine = { kind: "context" | "removed" | "added"; content: string; oldLine: number | null; newLine: number | null };
type ChatResponse = { message: ChatMessage; proposal?: FileProposal | null };
type SelectionContext =
  | { state: "active"; path: string; content: string }
  | { state: "none" };
export type ChatRequest = {
  messages: ChatMessage[];
  context: {
    openFile?: { path: string; content: string };
    selection?: SelectionContext;
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

export function canApplyProposal(openFile: OpenedFile | null, proposal: FileProposal): boolean {
  return openFile?.path === proposal.path && openFile.content === proposal.originalContent;
}

export function buildLineDiff(originalContent: string, proposedContent: string): DiffLine[] {
  const original = originalContent === "" ? [] : originalContent.split("\n");
  const proposed = proposedContent === "" ? [] : proposedContent.split("\n");

  // Keep rendering predictable for unusually large proposals instead of building
  // an unbounded LCS table in the UI thread.
  if (original.length * proposed.length > 1_000_000) {
    return [
      ...original.map((content, index) => ({ kind: "removed" as const, content, oldLine: index + 1, newLine: null })),
      ...proposed.map((content, index) => ({ kind: "added" as const, content, oldLine: null, newLine: index + 1 })),
    ];
  }

  const lengths = Array.from({ length: original.length + 1 }, () => new Uint32Array(proposed.length + 1));
  for (let oldIndex = original.length - 1; oldIndex >= 0; oldIndex -= 1) {
    for (let newIndex = proposed.length - 1; newIndex >= 0; newIndex -= 1) {
      lengths[oldIndex][newIndex] = original[oldIndex] === proposed[newIndex]
        ? lengths[oldIndex + 1][newIndex + 1] + 1
        : Math.max(lengths[oldIndex + 1][newIndex], lengths[oldIndex][newIndex + 1]);
    }
  }

  const result: DiffLine[] = [];
  let oldIndex = 0;
  let newIndex = 0;
  // The table stores suffix lengths, so reconstruction also moves forward.
  // Prefer a removal when both moves preserve the same LCS: replacements are
  // then rendered in the conventional removed-before-added order.
  while (oldIndex < original.length || newIndex < proposed.length) {
    if (oldIndex < original.length && newIndex < proposed.length && original[oldIndex] === proposed[newIndex]) {
      result.push({ kind: "context", content: original[oldIndex], oldLine: oldIndex + 1, newLine: newIndex + 1 });
      oldIndex += 1;
      newIndex += 1;
    } else if (newIndex < proposed.length && (oldIndex === original.length || lengths[oldIndex][newIndex + 1] > lengths[oldIndex + 1][newIndex])) {
      result.push({ kind: "added", content: proposed[newIndex], oldLine: null, newLine: newIndex + 1 });
      newIndex += 1;
    } else {
      result.push({ kind: "removed", content: original[oldIndex], oldLine: oldIndex + 1, newLine: null });
      oldIndex += 1;
    }
  }
  return result;
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
  if (openFile) {
    context.selection = selection
      ? { state: "active", path: openFile.path, content: selection }
      : { state: "none" };
  }
  if (project) context.project = { name: project.name, entries: projectEntries(project.children) };
  return {
    messages,
    context: Object.keys(context).length > 0 ? context : null,
  };
}

export function ChatPanel({ openFile, selection, project, onApplyProposal }: { openFile: OpenedFile | null; selection: string | null; project: ProjectTree | null; onApplyProposal: (proposal: FileProposal) => void }) {
  const { t } = useTranslation();
  const [message, setMessage] = useState("");
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [proposal, setProposal] = useState<FileProposal | null>(null);
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
    // A proposal belongs to the response that produced it. Once a new request
    // starts, remove the old review action so it cannot be applied while a
    // different answer is pending.
    setProposal(null);
    try {
      const response = await invoke<ChatResponse>("send_chat_message", {
        request: buildChatRequest(requestMessages, openFile, selection, project),
      });
      setMessages((current) => [...current, response.message]);
      setProposal(response.proposal ?? null);
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
      {proposal && <section className="file-proposal">
        <strong>{t("chat.proposal")}: {proposal.path}</strong>
        <div className="proposal-diff" role="table" aria-label={t("chat.proposal_diff")}>
          {buildLineDiff(proposal.originalContent, proposal.content).map((line, index) => <div className={`diff-line ${line.kind}`} role="row" key={`${line.kind}-${index}`}>
            <span className="diff-line-number" role="cell">{line.oldLine ?? ""}</span>
            <span className="diff-line-number" role="cell">{line.newLine ?? ""}</span>
            <span className="diff-marker" aria-hidden="true">{line.kind === "added" ? "+" : line.kind === "removed" ? "−" : " "}</span>
            <code role="cell">{line.content || " "}</code>
          </div>)}
        </div>
        {canApplyProposal(openFile, proposal)
          ? <div className="proposal-actions">
            <button type="button" onClick={() => { onApplyProposal(proposal); setProposal(null); }}>{t("chat.apply_proposal")}</button>
            <button type="button" className="secondary-button" onClick={() => setProposal(null)}>{t("chat.dismiss_proposal")}</button>
          </div>
          : <>
            <p className="proposal-stale">{t("chat.proposal_stale")}</p>
            <button type="button" className="secondary-button" onClick={() => setProposal(null)}>{t("chat.dismiss_proposal")}</button>
          </>}
      </section>}
    </div>
    <form className="chat-form" onSubmit={handleSubmit}><textarea aria-label={t("chat.placeholder")} placeholder={t("chat.placeholder")} value={message} onChange={(event) => setMessage(event.target.value)} rows={3} disabled={sending} /><button type="submit" disabled={!message.trim() || sending}>{t("chat.send")}</button></form>
  </aside>;
}
