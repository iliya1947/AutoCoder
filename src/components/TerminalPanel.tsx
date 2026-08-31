import { FormEvent, KeyboardEvent, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../hooks/useTranslation";

export type TerminalResult = {
  exitCode: number | null;
  stdout: string;
  stderr: string;
  cancelled: boolean;
};

export type TerminalTranscript = { command: string; result: TerminalResult };
type ProjectHistory = { chatMessages: unknown[]; terminalRuns: TerminalTranscript[] };
export type TerminalExecution =
  | { status: "idle" }
  | { status: "running"; command: string }
  | { status: "completed"; transcript: TerminalTranscript };

export function beginTerminalRun(command: string): TerminalExecution {
  return { status: "running", command };
}

export function completeTerminalRun(execution: TerminalExecution, result: TerminalResult): TerminalExecution {
  return execution.status === "running"
    ? { status: "completed", transcript: { command: execution.command, result } }
    : execution;
}

export function isCurrentTerminalRun(requestRunId: number, activeRunId: number | null): boolean {
  return requestRunId === activeRunId;
}

export function navigateTerminalHistory(
  history: string[],
  index: number,
  draft: string,
  direction: "previous" | "next",
  current: string,
) {
  if (history.length === 0) return { command: current, index, draft };
  if (direction === "previous") {
    const nextIndex = Math.max(0, index - 1);
    return { command: history[nextIndex], index: nextIndex, draft: index === history.length ? current : draft };
  }
  const nextIndex = Math.min(history.length, index + 1);
  return { command: nextIndex === history.length ? draft : history[nextIndex], index: nextIndex, draft };
}

export function TerminalPanel({ projectOpen, proposedCommand, onCompleted }: { projectOpen: boolean; proposedCommand?: { command: string } | null; onCompleted?: (transcript: TerminalTranscript) => void }) {
  const { t } = useTranslation();
  const [command, setCommand] = useState("");
  const [execution, setExecution] = useState<TerminalExecution>({ status: "idle" });
  const [cancelling, setCancelling] = useState(false);
  const [error, setError] = useState("");
  const history = useRef<string[]>([]);
  const historyIndex = useRef(0);
  const historyDraft = useRef("");
  const nextRunId = useRef(0);
  const activeRunId = useRef<number | null>(null);
  const cancelInFlight = useRef(false);
  const running = execution.status === "running";
  const transcript = execution.status === "completed" ? execution.transcript : null;
  const result = transcript?.result ?? null;

  useEffect(() => {
    if (projectOpen && proposedCommand) {
      setCommand(proposedCommand.command);
      historyIndex.current = history.current.length;
      historyDraft.current = proposedCommand.command;
    }
  }, [projectOpen, proposedCommand]);

  useEffect(() => {
    if (!projectOpen) {
      setCommand("");
      setExecution({ status: "idle" });
      setError("");
      history.current = [];
      historyIndex.current = 0;
      historyDraft.current = "";
      activeRunId.current = null;
      cancelInFlight.current = false;
    }
  }, [projectOpen]);

  useEffect(() => {
    if (!projectOpen) return;
    let current = true;
    invoke<ProjectHistory>("load_project_history").then((stored) => {
      if (!current) return;
      history.current = stored.terminalRuns.map((run) => run.command);
      historyIndex.current = history.current.length;
      const latest = stored.terminalRuns.at(-1);
      if (latest) setExecution({ status: "completed", transcript: latest });
    }).catch((reason) => { if (current) setError(String(reason)); });
    return () => { current = false; };
  }, [projectOpen]);

  useEffect(() => () => {
    activeRunId.current = null;
    cancelInFlight.current = false;
  }, []);

  const run = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const value = command.trim();
    if (!projectOpen || !value || activeRunId.current !== null) return;
    const runId = ++nextRunId.current;
    activeRunId.current = runId;
    if (history.current.at(-1) !== value) history.current.push(value);
    historyIndex.current = history.current.length;
    historyDraft.current = "";
    setExecution(beginTerminalRun(value));
    setCancelling(false);
    setError("");
    try {
      const completedResult = await invoke<TerminalResult>("execute_project_command", { command: value });
      if (!isCurrentTerminalRun(runId, activeRunId.current)) return;
      setExecution((current) => completeTerminalRun(current, completedResult));
      onCompleted?.({ command: value, result: completedResult });
    } catch (reason) {
      if (!isCurrentTerminalRun(runId, activeRunId.current)) return;
      setExecution({ status: "idle" });
      setError(String(reason));
    } finally {
      if (isCurrentTerminalRun(runId, activeRunId.current)) {
        activeRunId.current = null;
        cancelInFlight.current = false;
        setCancelling(false);
      }
    }
  };

  const cancel = async () => {
    const runId = activeRunId.current;
    if (runId === null || cancelInFlight.current) return;
    cancelInFlight.current = true;
    setCancelling(true);
    setError("");
    try {
      const accepted = await invoke<boolean>("cancel_project_command");
      if (isCurrentTerminalRun(runId, activeRunId.current) && !accepted) setError(t("terminal.cancel_unavailable"));
    } catch (reason) {
      if (isCurrentTerminalRun(runId, activeRunId.current)) {
        setError(String(reason));
        cancelInFlight.current = false;
        setCancelling(false);
      }
    }
  };

  const navigateHistory = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
    event.preventDefault();
    const next = navigateTerminalHistory(
      history.current,
      historyIndex.current,
      historyDraft.current,
      event.key === "ArrowUp" ? "previous" : "next",
      command,
    );
    historyIndex.current = next.index;
    historyDraft.current = next.draft;
    setCommand(next.command);
  };

  const editCommand = (value: string) => {
    setCommand(value);
    historyIndex.current = history.current.length;
    historyDraft.current = value;
  };

  return <section className="terminal-panel" aria-label={t("terminal.title")}>
    <div className="panel-heading">
      <h2>{t("terminal.title")}</h2>
      {result && <span className={!result.cancelled && result.exitCode === 0 ? "terminal-success" : "terminal-failure"}>
        {result.cancelled ? t("terminal.cancelled") : `${t("terminal.exit_code")}: ${result.exitCode ?? t("terminal.unknown_exit")}`}
      </span>}
      {history.current.length > 0 && !running && <button type="button" className="secondary-button" onClick={async () => { try { await invoke("clear_project_history", { kind: "terminal" }); history.current = []; historyIndex.current = 0; setExecution({ status: "idle" }); } catch (reason) { setError(String(reason)); } }}>{t("terminal.clear_history")}</button>}
    </div>
    <div className="terminal-output" aria-live="polite">
      {!projectOpen
        ? <p>{t("terminal.open_project")}</p>
        : !result && !error && <p>{t("terminal.empty")}</p>}
      {result?.stdout && <pre>{result.stdout}</pre>}
      {result?.stderr && <pre className="terminal-stderr">{result.stderr}</pre>}
      {error && <pre className="terminal-stderr" role="alert">{error}</pre>}
    </div>
    <form className="terminal-form" onSubmit={run}>
      <span aria-hidden="true">›</span>
      <input aria-label={t("terminal.command")} value={command} onChange={(event) => editCommand(event.target.value)} onKeyDown={navigateHistory} disabled={!projectOpen || running} placeholder={t("terminal.placeholder")} />
      <button type="submit" disabled={!projectOpen || !command.trim() || running}>{running ? t("terminal.running") : t("terminal.run")}</button>
      {running && <button type="button" className="terminal-cancel" onClick={cancel} disabled={cancelling}>
        {cancelling ? t("terminal.cancelling") : t("terminal.cancel")}
      </button>}
    </form>
  </section>;
}
