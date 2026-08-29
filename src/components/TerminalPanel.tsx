import { FormEvent, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../hooks/useTranslation";

export type TerminalResult = {
  exitCode: number | null;
  stdout: string;
  stderr: string;
};

export function TerminalPanel({ projectOpen, proposedCommand }: { projectOpen: boolean; proposedCommand?: { command: string } | null }) {
  const { t } = useTranslation();
  const [command, setCommand] = useState("");
  const [result, setResult] = useState<TerminalResult | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (projectOpen && proposedCommand) setCommand(proposedCommand.command);
  }, [projectOpen, proposedCommand]);

  const run = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const value = command.trim();
    if (!projectOpen || !value || running) return;
    setRunning(true);
    setError("");
    try {
      setResult(await invoke<TerminalResult>("execute_project_command", { command: value }));
    } catch (reason) {
      setResult(null);
      setError(String(reason));
    } finally {
      setRunning(false);
    }
  };

  return <section className="terminal-panel" aria-label={t("terminal.title")}>
    <div className="panel-heading">
      <h2>{t("terminal.title")}</h2>
      {result && <span className={result.exitCode === 0 ? "terminal-success" : "terminal-failure"}>
        {t("terminal.exit_code")}: {result.exitCode ?? t("terminal.unknown_exit")}
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
      <input aria-label={t("terminal.command")} value={command} onChange={(event) => setCommand(event.target.value)} disabled={!projectOpen || running} placeholder={t("terminal.placeholder")} />
      <button type="submit" disabled={!projectOpen || !command.trim() || running}>{running ? t("terminal.running") : t("terminal.run")}</button>
    </form>
  </section>;
}
