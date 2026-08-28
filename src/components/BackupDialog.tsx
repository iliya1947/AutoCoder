import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../hooks/useTranslation";
import { ProjectTree } from "../types/project";

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

export function BackupDialog({ open, onClose, onRestored, canRestore }: Props) {
  const { t, lang } = useTranslation();
  const [backups, setBackups] = useState<BackupEntry[]>([]);
  const [selected, setSelected] = useState<BackupEntry | null>(null);
  const [status, setStatus] = useState<"idle" | "loading" | "restoring" | "error">("idle");

  useEffect(() => {
    if (!open) return;
    setStatus("loading");
    setSelected(null);
    invoke<BackupEntry[]>("list_project_backups")
      .then((items) => { setBackups(items); setSelected(items[0] ?? null); setStatus("idle"); })
      .catch(() => setStatus("error"));
  }, [open]);

  if (!open) return null;
  const restore = async () => {
    if (!selected || !canRestore()) return;
    setStatus("restoring");
    try {
      const project = await invoke<ProjectTree>("restore_project_backup", {
        backupId: selected.id,
        expectedCurrentContent: selected.currentContent,
      });
      onRestored(selected, project);
      onClose();
    } catch { setStatus("error"); }
  };

  return <div className="dialog-backdrop" role="presentation">
    <section className="backup-dialog" role="dialog" aria-modal="true" aria-labelledby="backup-title">
      <div className="panel-heading"><h2 id="backup-title">{t("backups.title")}</h2><button className="secondary-button" onClick={onClose} aria-label={t("common.close")}>×</button></div>
      {status === "loading" ? <p className="project-state">{t("backups.loading")}</p> : backups.length === 0 ? <p className="project-state">{t("backups.empty")}</p> : <div className="backup-browser">
        <ul className="backup-list">{backups.map((backup) => <li key={backup.id}><button className={selected?.id === backup.id ? "active" : ""} onClick={() => setSelected(backup)}><strong>{backup.relativePath}</strong><span>{new Date(backup.createdAtUnixMs).toLocaleString(lang)}</span></button></li>)}</ul>
        <div className="backup-preview"><p>{selected?.relativePath}</p><pre>{selected?.content}</pre></div>
      </div>}
      {status === "error" && <p className="editor-error" role="alert">{t("backups.error")}</p>}
      <div className="proposal-actions backup-actions"><button className="secondary-button" onClick={onClose}>{t("common.cancel")}</button><button className="save-button" disabled={!selected || status === "loading" || status === "restoring"} onClick={restore}>{status === "restoring" ? t("backups.restoring") : t("backups.restore")}</button></div>
    </section>
  </div>;
}
