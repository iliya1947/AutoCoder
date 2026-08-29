import { FormEvent, KeyboardEvent, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../hooks/useTranslation";

export type TerminalResult = {
  exitCode: number | null;
  stdout: string;
  stderr: string;
  cancelled: boolean;
};

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

export function TerminalPanel({ projectOpen, proposedCommand }: { projectOpen: boolean; proposedCommand?: { command: string } | null }) {
  const { t } = useTranslation();
  const [command, setCommand] = useState("");
  const [result, setResult] = useState<TerminalResult | null>(null);
  const [running, setRunning] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [error, setError] = useState("");
  const history = useRef<string[]>([]);
  const historyIndex = useRef(0);
  const historyDraft = useRef("");

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
      setResult(null);
      setError("");
      history.current = [];
      historyIndex.current = 0;
      historyDraft.current = "";
    }
  }, [projectOpen]);

  const run = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const value = command.trim();
    if (!projectOpen || !value || running) return;
    if (history.current.at(-1) !== value) history.current.push(value);
    historyIndex.current = history.current.length;
    historyDraft.current = "";
    setRunning(true);
    setCancelling(false);
    setError("");
    try {
      setResult(await invoke<TerminalResult>("execute_project_command", { command: value }));
    } catch (reason) {
      setResult(null);
      setError(String(reason));
    } finally {
      setRunning(false);
      setCancelling(false);
    }
  };

  const cancel = async () => {
    if (!running || cancelling) return;
    setCancelling(true);
    setError("");
    try {
      const accepted = await invoke<boolean>("cancel_project_command");
      if (!accepted) setError(t("terminal.cancel_unavailable"));
    } catch (reason) {
      setError(String(reason));
      setCancelling(false);
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
