import MonacoEditor, { OnMount } from "@monaco-editor/react";
import { useEffect, useRef } from "react";
import { useTranslation } from "../hooks/useTranslation";
import { OpenedFile } from "../types/project";
import { editorLanguage } from "../utils/projectTree";

export type EditorStatus = "idle" | "loading" | "error" | "ready";

export function selectedText<T>(model: { getValueInRange: (selection: T) => string } | null, selection: T): string | null {
  const selected = model?.getValueInRange(selection) ?? "";
  return selected.length > 0 ? selected : null;
}

export function Editor({ file, status, error, saving, onChange, onSelectionChange, onSave }: { file: OpenedFile | null; status: EditorStatus; error?: string; saving: boolean; onChange: (content: string) => void; onSelectionChange: (content: string | null) => void; onSave: () => void }) {
  const { t } = useTranslation();
  const isDirty = file !== null && file.content !== file.savedContent;
  const selectionCallback = useRef(onSelectionChange);
  const listenerDisposer = useRef<(() => void) | null>(null);
  selectionCallback.current = onSelectionChange;

  useEffect(() => {
    // A Monaco model can change without remounting this component. Clear the
    // parent state after that render, including any selection restored by Monaco.
    selectionCallback.current(null);
  }, [file?.path]);

  useEffect(() => () => listenerDisposer.current?.(), []);

  const handleMount: OnMount = (editor) => {
    listenerDisposer.current?.();
    const selectionListener = editor.onDidChangeCursorSelection(({ selection }) => {
      selectionCallback.current(selectedText(editor.getModel(), selection));
    });
    const modelListener = editor.onDidChangeModel(() => selectionCallback.current(null));
    listenerDisposer.current = () => {
      selectionListener.dispose();
      modelListener.dispose();
    };
  };
  return <section className="editor-panel" aria-labelledby="editor-heading">
    <div className="panel-heading editor-heading"><h2 id="editor-heading">{file?.name ?? t("editor.no_file")}{isDirty && <span className="dirty-indicator" title={t("editor.unsaved")} aria-label={t("editor.unsaved")}>●</span>}</h2><button type="button" className="save-button" onClick={onSave} disabled={!isDirty || saving}>{saving ? t("editor.saving") : t("editor.save")}</button></div>
    {error && status !== "error" && <p className="editor-error" role="alert">{error}</p>}
    {status === "loading" && <div className="editor-placeholder" role="status"><p>{t("editor.loading")}</p></div>}
    {status === "error" && <div className="editor-placeholder error-state" role="alert"><h2>{t("editor.read_error_title")}</h2><p>{error || t("editor.read_error")}</p></div>}
    {status !== "loading" && status !== "error" && file && <div className="monaco-container"><MonacoEditor path={file.path} language={editorLanguage(file.name)} value={file.content} theme="vs-dark" onMount={handleMount} onChange={(value) => onChange(value ?? "")} options={{ automaticLayout: true, minimap: { enabled: false }, wordWrap: "on" }} /></div>}
    {status === "idle" && !file && <div className="editor-placeholder"><p className="eyebrow">{t("editor.placeholder_label")}</p><h2>{t("editor.no_file")}</h2><p>{t("editor.placeholder_description")}</p></div>}
  </section>;
}
