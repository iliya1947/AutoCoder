import { FormEvent, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../hooks/useTranslation";
import { OpenedFile, ProjectNode, ProjectTree } from "../types/project";
import type { TerminalTranscript } from "./TerminalPanel";
import { completeTask, continueTask, markActionRunning, proposeAction, recordResult, startTask, taskSnapshot } from "../types/orchestration";
import type { OrchestrationSnapshot, OrchestrationTask, ToolKind } from "../types/orchestration";

export type ChatMessage = { role: "user" | "assistant"; content: string };
type ProjectHistory = { chatMessages: ChatMessage[]; terminalRuns: TerminalTranscript[]; orchestrationTask?: OrchestrationTask | null };
export type FileProposal =
  | { operation: "replace"; path: string; content: string; originalContent: string }
  | { operation: "create"; path: string; content: string }
  | { operation: "delete"; path: string; originalContent: string; expectedSavedContent: string };
export type DiffLine = { kind: "context" | "removed" | "added"; content: string; oldLine: number | null; newLine: number | null };
export type TerminalProposal = { command: string };
export type ToolResult = { id: number; content: string };
type ChatResponse = { message: ChatMessage; proposal?: FileProposal | null; commandProposal?: TerminalProposal | null; projectKey: string };
type SelectionContext =
  | { state: "active"; path: string; content: string }
  | { state: "none" };
export type ChatRequest = {
  messages: ChatMessage[];
  context: {
    openFile?: { path: string; content: string; savedContent: string };
    selection?: SelectionContext;
    project?: { name: string; entries: string[] };
  } | null;
  orchestration?: OrchestrationSnapshot;
};

export function chatContextKey(openFile: OpenedFile | null, selection: string | null, project: ProjectTree | null): string {
  return JSON.stringify([
    project ? [project.name, projectEntries(project.children)] : null,
    openFile ? [openFile.path, openFile.content, openFile.savedContent] : null,
    selection,
  ]);
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

export function isChatResponseCurrent(requestContextKey: string, currentContextKey: string): boolean {
  return requestContextKey === currentContextKey;
}

export function chatRequestError(
  requestContextKey: string,
  currentContextKey: string,
  error: unknown,
  contextChangedMessage: string,
): string {
  if (!isChatResponseCurrent(requestContextKey, currentContextKey)) return contextChangedMessage;
  return error instanceof Error ? error.message : String(error);
}

export function formatTerminalResultDraft({ command, result }: TerminalTranscript): string {
  const status = result.cancelled ? "cancelled" : `exit code: ${result.exitCode ?? "unknown"}`;
  const sections = [`Command: ${command}`, `Status: ${status}`];
  if (result.stdout) sections.push(`stdout:\n${result.stdout}`);
  if (result.stderr) sections.push(`stderr:\n${result.stderr}`);
  return sections.join("\n\n");
}

export function formatFileToolResult(proposal: FileProposal, outcome: "completed" | "failed", details?: string): string {
  const action = proposal.operation === "replace"
    ? `updated the editor buffer for ${proposal.path} (not saved to disk yet)`
    : proposal.operation === "create"
      ? `created ${proposal.path} on disk`
      : `deleted ${proposal.path} from disk after creating a safety backup`;
  return [
    "AutoCoder File Tool result (this is factual output from the explicitly approved action):",
    `Status: ${outcome}`,
    `Action: ${action}`,
    ...(details ? [`Details: ${details}`] : []),
    "Continue the task from this result. If another action is needed, propose exactly one next File Tool or Terminal Tool action for review.",
  ].join("\n");
}

export function formatTerminalToolResult(transcript: TerminalTranscript): string {
  return [
    "AutoCoder Terminal Tool result (this is factual output from the explicitly approved command):",
    formatTerminalResultDraft(transcript),
    "Continue the task from this result. If another action is needed, propose exactly one next File Tool or Terminal Tool action for review.",
  ].join("\n\n");
}

export function formatActionLifecycleResult(tool: ToolKind, outcome: "declined" | "interrupted"): string {
  const detail = outcome === "declined"
    ? "declined by the user; the proposed action was not executed"
    : "interrupted by an application restart; AutoCoder cannot prove whether the action completed and will not run it again";
  return [
    `AutoCoder ${tool === "file" ? "File" : "Terminal"} Tool lifecycle event:`,
    `Status: ${detail}.`,
    "Continue without assuming that action occurred. Re-check the current project context before proposing another action.",
  ].join("\n");
}

export function canApplyProposal(openFile: OpenedFile | null, proposal: FileProposal): boolean {
  if (proposal.operation === "create") return true;
  if (openFile?.path !== proposal.path || openFile.content !== proposal.originalContent) return false;
  return proposal.operation !== "delete"
    || (openFile.content === openFile.savedContent && openFile.savedContent === proposal.expectedSavedContent);
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
  orchestration?: OrchestrationSnapshot,
): ChatRequest {
  const context: NonNullable<ChatRequest["context"]> = {};
  if (openFile) context.openFile = { path: openFile.path, content: openFile.content, savedContent: openFile.savedContent };
  if (openFile) {
    context.selection = selection
      ? { state: "active", path: openFile.path, content: selection }
      : { state: "none" };
  }
  if (project) context.project = { name: project.name, entries: projectEntries(project.children) };
  return {
    messages,
    context: Object.keys(context).length > 0 ? context : null,
    ...(orchestration ? { orchestration } : {}),
  };
}

export function ChatPanel({ openFile, selection, project, toolResult, onApplyProposal, onReviewCommand }: { openFile: OpenedFile | null; selection: string | null; project: ProjectTree | null; toolResult?: ToolResult | null; onApplyProposal: (proposal: FileProposal) => void | Promise<void>; onReviewCommand: (proposal: TerminalProposal) => void }) {
  const { t } = useTranslation();
  const [message, setMessage] = useState("");
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [sending, setSending] = useState(false);
  const [applying, setApplying] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [proposal, setProposal] = useState<FileProposal | null>(null);
  const [commandProposal, setCommandProposal] = useState<TerminalProposal | null>(null);
  const [recoveredTask, setRecoveredTask] = useState<OrchestrationTask | null>(null);
  const lastRequestContext = useRef<string | null>(null);
  const requestInFlight = useRef(false);
  const applyInFlight = useRef(false);
  const latestRequest = useRef(0);
  const consumedToolResult = useRef<number | null>(null);
  const currentContext = useRef(chatContextKey(openFile, selection, project));
  const currentTask = useRef<OrchestrationTask | null>(null);
  const taskCounter = useRef(0);
  currentContext.current = chatContextKey(openFile, selection, project);

  useEffect(() => () => {
    latestRequest.current += 1;
    requestInFlight.current = false;
    applyInFlight.current = false;
  }, []);

  useEffect(() => {
    if (!project) return;
    let current = true;
    invoke<ProjectHistory>("load_project_history")
      .then((history) => { if (current) {
        setMessages(history.chatMessages);
        const task = history.orchestrationTask ?? null;
        currentTask.current = task;
        setRecoveredTask(task && task.status !== "completed" && task.status !== "failed" ? task : null);
        lastRequestContext.current = currentContext.current;
      } })
      .catch((reason) => { if (current) setError(String(reason)); });
    return () => { current = false; };
  }, [project?.name]);

  useEffect(() => {
    // Review actions are snapshots of the context which produced them. This is
    // especially important for create and command proposals, which cannot be
    // validated against the currently open file.
    setProposal(null);
    setCommandProposal(null);
    if (currentTask.current?.status === "awaiting_approval") setRecoveredTask(currentTask.current);
  }, [openFile?.path, openFile?.content, openFile?.savedContent, selection, project]);

  const updateTask = (next: OrchestrationTask | null) => {
    currentTask.current = next;
  };

  const persistTask = async (next: OrchestrationTask | null) => {
    await invoke("save_orchestration_task", { task: next });
    updateTask(next);
  };

  const sendContent = async (rawContent: string, preserveHistory = false, continuedTask?: OrchestrationTask) => {
    const content = rawContent.trim();
    if (!content || requestInFlight.current) return;
    requestInFlight.current = true;
    const requestId = ++latestRequest.current;
    const contextKey = chatContextKey(openFile, selection, project);
    const requestTask = continuedTask
      ? continueTask(continuedTask)
      : startTask(`task-${Date.now()}-${++taskCounter.current}`, content);
    try {
      await persistTask(requestTask);
    } catch (reason) {
      requestInFlight.current = false;
      setError(String(reason));
      return;
    }
    setRecoveredTask(null);
    const requestMessages = preserveHistory
      ? [...messages, { role: "user" as const, content }]
      : messagesForCurrentContext(messages, content, lastRequestContext.current, contextKey);
    lastRequestContext.current = contextKey;
    const pendingMessages = [...messages, { role: "user" as const, content }];
    setMessages(pendingMessages);
    setMessage("");
    setSending(true);
    setError(null);
    // A proposal belongs to the response that produced it. Once a new request
    // starts, remove the old review action so it cannot be applied while a
    // different answer is pending.
    setProposal(null);
    setCommandProposal(null);
    try {
      const response = await invoke<ChatResponse>("send_chat_message", {
        request: buildChatRequest(requestMessages, openFile, selection, project, taskSnapshot(requestTask)),
      });
      if (requestId !== latestRequest.current) return;
      if (!isChatResponseCurrent(contextKey, currentContext.current)) {
        setError(t("chat.context_changed"));
        return;
      }
      const completedMessages = [...pendingMessages, response.message];
      const responseAction = response.proposal
        ? { tool: "file" as ToolKind, payload: response.proposal }
        : response.commandProposal
          ? { tool: "terminal" as ToolKind, payload: response.commandProposal }
          : null;
      const nextTask = responseAction
        ? proposeAction(requestTask, responseAction.tool, responseAction.payload, contextKey).task
        : completeTask(requestTask);
      await invoke("save_chat_exchange", {
        projectKey: response.projectKey,
        userMessage: { role: "user", content },
        assistantMessage: response.message,
        orchestrationTask: nextTask,
      });
      updateTask(nextTask);
      setMessages(completedMessages);
      setProposal(response.proposal ?? null);
      setCommandProposal(response.commandProposal ?? null);
    } catch (error) {
      if (requestId !== latestRequest.current) return;
      if (isChatResponseCurrent(contextKey, currentContext.current)) {
        console.error("send_chat_message failed", error);
      }
      setError(chatRequestError(contextKey, currentContext.current, error, t("chat.context_changed")));
    } finally {
      if (requestId === latestRequest.current) {
        requestInFlight.current = false;
        setSending(false);
      }
    }
  };

  useEffect(() => {
    if (!toolResult || sending || consumedToolResult.current === toolResult.id) return;
    consumedToolResult.current = toolResult.id;
    const activeTask = currentTask.current;
    const activeAction = activeTask?.actions.at(-1);
    if (!activeTask || !activeAction || !["proposed", "running"].includes(activeAction.status)) return;
    const outcome = toolResult.content.includes("Status: failed") ? "failed" : toolResult.content.includes("Status: cancelled") ? "cancelled" : "completed";
    const withResult = recordResult(markActionRunning(activeTask, activeAction.id), {
      id: `${activeAction.id}:result`, actionId: activeAction.id, tool: activeAction.tool, outcome, content: toolResult.content,
    });
    void persistTask(withResult).then(() => sendContent(toolResult.content, true, withResult)).catch((reason) => setError(String(reason)));
  }, [toolResult, sending]);

  const finishWithoutExecution = async (outcome: "declined" | "interrupted") => {
    const task = currentTask.current;
    const action = task?.actions.at(-1);
    if (!task || !action || !["proposed", "running"].includes(action.status)) return;
    const content = formatActionLifecycleResult(action.tool, outcome);
    const withResult = recordResult(task, { id: `${action.id}:result`, actionId: action.id, tool: action.tool, outcome, content });
    setProposal(null);
    setCommandProposal(null);
    setRecoveredTask(null);
    await persistTask(withResult);
    await sendContent(content, true, withResult);
  };

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    void sendContent(message);
  };

  return <aside className="chat-panel" aria-label={t("sidebar.chat")}>
    <div className="panel-heading"><h2>{t("sidebar.chat")}</h2><span className="status-dot">{t("chat.ollama")}</span>{messages.length > 0 && <button type="button" className="secondary-button" onClick={async () => { try { await invoke("clear_project_history", { kind: "chat" }); await persistTask(null); setMessages([]); setRecoveredTask(null); } catch (reason) { setError(String(reason)); } }}>{t("chat.clear_history")}</button>}</div>
    <div className="chat-messages" aria-live="polite">
      {recoveredTask && (() => {
        const action = recoveredTask.actions.at(-1);
        const canReview = recoveredTask.status === "awaiting_approval" && action?.status === "proposed" && action.contextKey === currentContext.current;
        const canResume = recoveredTask.status === "thinking" || recoveredTask.status === "awaiting_ai";
        return <section className="orchestration-recovery">
          <strong>{t("chat.task_recovered")}</strong>
          <p>{canResume ? t("chat.resume_pending") : canReview ? t("chat.resume_review") : t("chat.resume_stale")}</p>
          {canResume && <button type="button" onClick={() => {
            const content = recoveredTask.status === "thinking" ? recoveredTask.goal : recoveredTask.actions.at(-1)?.result?.content;
            if (content) void sendContent(content, recoveredTask.actions.length > 0, recoveredTask);
          }}>{t("chat.resume_task")}</button>}
          {canReview && <button type="button" onClick={() => {
            if (action?.tool === "file") setProposal(action.payload as FileProposal);
            if (action?.tool === "terminal") setCommandProposal(action.payload as TerminalProposal);
            setRecoveredTask(null);
          }}>{t("chat.review_recovered")}</button>}
          {!canResume && !canReview && <button type="button" onClick={() => void finishWithoutExecution(action?.status === "running" ? "interrupted" : "declined")}>{t("chat.continue_safely")}</button>}
        </section>;
      })()}
      {messages.length === 0 && !sending ? <p className="empty-chat">{t("chat.empty")}</p> : messages.map((item, index) => <p className={`${item.role}-message`} key={`${item.role}-${index}`}>{item.content}</p>)}
      {sending && <p className="chat-status">{t("chat.sending")}</p>}
      {error && <p className="chat-error" role="alert">
        {t("chat.error")}<br /><code>{error}</code>
      </p>}
      {proposal && <section className="file-proposal">
        <strong>{t(proposal.operation === "create" ? "chat.proposal_create" : proposal.operation === "delete" ? "chat.proposal_delete" : "chat.proposal")}: {proposal.path}</strong>
        <div className="proposal-diff" role="table" aria-label={t("chat.proposal_diff")}>
          {buildLineDiff(proposal.operation === "create" ? "" : proposal.originalContent, proposal.operation === "delete" ? "" : proposal.content).map((line, index) => <div className={`diff-line ${line.kind}`} role="row" key={`${line.kind}-${index}`}>
            <span className="diff-line-number" role="cell">{line.oldLine ?? ""}</span>
            <span className="diff-line-number" role="cell">{line.newLine ?? ""}</span>
            <span className="diff-marker" aria-hidden="true">{line.kind === "added" ? "+" : line.kind === "removed" ? "−" : " "}</span>
            <code role="cell">{line.content || " "}</code>
          </div>)}
        </div>
        {canApplyProposal(openFile, proposal)
          ? <div className="proposal-actions">
            <button type="button" disabled={applying} onClick={async () => {
              if (applyInFlight.current) return;
              applyInFlight.current = true;
              setApplying(true);
              try {
                const active = currentTask.current?.actions.at(-1);
                if (active) await persistTask(markActionRunning(currentTask.current!, active.id));
                await onApplyProposal(proposal);
                setProposal(null);
              } finally {
                applyInFlight.current = false;
                setApplying(false);
              }
            }}>{t(proposal.operation === "create" ? "chat.create_proposal" : proposal.operation === "delete" ? "chat.delete_proposal" : "chat.apply_proposal")}</button>
            <button type="button" disabled={applying} className="secondary-button" onClick={() => void finishWithoutExecution("declined")}>{t("chat.dismiss_proposal")}</button>
          </div>
          : <>
            <p className="proposal-stale">{t(proposal.operation === "delete" && openFile?.content !== openFile?.savedContent ? "chat.proposal_delete_dirty" : "chat.proposal_stale")}</p>
            <button type="button" className="secondary-button" onClick={() => void finishWithoutExecution("declined")}>{t("chat.dismiss_proposal")}</button>
          </>}
      </section>}
      {commandProposal && <section className="file-proposal terminal-proposal">
        <strong>{t("chat.command_proposal")}</strong>
        <pre><code>{commandProposal.command}</code></pre>
        <p>{t("chat.command_review_warning")}</p>
        <div className="proposal-actions">
          <button type="button" onClick={async () => {
            const active = currentTask.current?.actions.at(-1);
            if (active) await persistTask(markActionRunning(currentTask.current!, active.id));
            onReviewCommand(commandProposal); setCommandProposal(null);
          }}>{t("chat.review_command")}</button>
          <button type="button" className="secondary-button" onClick={() => void finishWithoutExecution("declined")}>{t("chat.dismiss_proposal")}</button>
        </div>
      </section>}
    </div>
    <form className="chat-form" onSubmit={handleSubmit}><textarea aria-label={t("chat.placeholder")} placeholder={t("chat.placeholder")} value={message} onChange={(event) => setMessage(event.target.value)} rows={3} disabled={sending} /><button type="submit" disabled={!message.trim() || sending}>{t("chat.send")}</button></form>
  </aside>;
}
