import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../hooks/useTranslation";
import { ProjectTree } from "../types/project";
import { operationError } from "../utils/invokeError";

export type BackupEntry = {
  id: string;
  createdAtUnixMs: number;
  relativePath: string;
  content: string;
  currentContent: string | null;
};

type Props = {
  open: boolean;
  onClose: () => void;
  onRestored: (backup: BackupEntry, project: ProjectTree) => void;
  canRestore: () => boolean;
};

export function isLatestBackupRequest(requestId: number, latestRequestId: number): boolean {
  return requestId === latestRequestId;
}

export function BackupDialog({ open, onClose, onRestored, canRestore }: Props) {
  const { t, lang } = useTranslation();
  const [backups, setBackups] = useState<BackupEntry[]>([]);
  const [selected, setSelected] = useState<BackupEntry | null>(null);
  const [status, setStatus] = useState<"idle" | "loading" | "restoring" | "error">("idle");
  const [error, setError] = useState("");
  const latestListRequest = useRef(0);
  const latestRestoreRequest = useRef(0);
  const restoreInFlight = useRef(false);

  useEffect(() => () => { latestRestoreRequest.current += 1; restoreInFlight.current = false; }, []);

  useEffect(() => {
    if (!open || restoreInFlight.current) return;
    const requestId = ++latestListRequest.current;
    setStatus("loading");
    setError("");
    setSelected(null);
    invoke<BackupEntry[]>("list_project_backups")
      .then((items) => {
        if (!isLatestBackupRequest(requestId, latestListRequest.current)) return;
        setBackups(items);
        setSelected(items[0] ?? null);
        setStatus("idle");
      })
      .catch((reason) => {
        if (!isLatestBackupRequest(requestId, latestListRequest.current)) return;
        setError(operationError(t("backups.error"), reason));
        setStatus("error");
      });
    return () => { latestListRequest.current += 1; };
  }, [open, lang]);

  if (!open) return null;
  const close = () => {
    latestListRequest.current += 1;
    latestRestoreRequest.current += 1;
    restoreInFlight.current = false;
    onClose();
  };
  const restore = async () => {
    if (!selected || restoreInFlight.current || !canRestore()) return;
    restoreInFlight.current = true;
    // A list completion must not turn the dialog back to idle while restore is
    // still in flight (for example after a language change).
    latestListRequest.current += 1;
    const requestId = ++latestRestoreRequest.current;
    setStatus("restoring");
    setError("");
    try {
      const project = await invoke<ProjectTree>("restore_project_backup", {
        backupId: selected.id,
        expectedCurrentContent: selected.currentContent,
      });
      if (!isLatestBackupRequest(requestId, latestRestoreRequest.current)) return;
      onRestored(selected, project);
      close();
    } catch (reason) {
      if (!isLatestBackupRequest(requestId, latestRestoreRequest.current)) return;
      setError(operationError(t("backups.error"), reason));
      setStatus("error");
    } finally {
      if (isLatestBackupRequest(requestId, latestRestoreRequest.current)) restoreInFlight.current = false;
    }
  };

  return <div className="dialog-backdrop" role="presentation">
    <section className="backup-dialog" role="dialog" aria-modal="true" aria-labelledby="backup-title">
      <div className="panel-heading"><h2 id="backup-title">{t("backups.title")}</h2><button className="secondary-button" onClick={close} aria-label={t("common.close")}>×</button></div>
      {status === "loading" ? <p className="project-state">{t("backups.loading")}</p> : backups.length === 0 ? <p className="project-state">{t("backups.empty")}</p> : <div className="backup-browser">
        <ul className="backup-list">{backups.map((backup) => <li key={backup.id}><button className={selected?.id === backup.id ? "active" : ""} onClick={() => setSelected(backup)}><strong>{backup.relativePath}</strong><span>{new Date(backup.createdAtUnixMs).toLocaleString(lang)}</span></button></li>)}</ul>
        <div className="backup-preview"><p>{selected?.relativePath}</p><pre>{selected?.content}</pre></div>
      </div>}
      {status === "error" && <p className="editor-error" role="alert">{error || t("backups.error")}</p>}
      <div className="proposal-actions backup-actions"><button className="secondary-button" onClick={close}>{t("common.cancel")}</button><button className="save-button" disabled={!selected || status === "loading" || status === "restoring"} onClick={restore}>{status === "restoring" ? t("backups.restoring") : t("backups.restore")}</button></div>
    </section>
  </div>;
}
