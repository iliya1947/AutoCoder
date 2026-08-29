import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { editorContextKey, isCurrentProjectSession, isLatestFileRead, isLatestFileSave, markFileSaved } from "../App";
import { Editor, selectedText } from "./Editor";
import { ProjectExplorer } from "./ProjectExplorer";
import { beginTerminalRun, completeTerminalRun, navigateTerminalHistory, TerminalPanel, TerminalResult } from "./TerminalPanel";

describe("panel states", () => {
  it("ignores stale file reads and save completions", () => {
    expect(isLatestFileRead(1, 2)).toBe(false);
    expect(isLatestFileRead(2, 2)).toBe(true);
    expect(isLatestFileSave(1, 2)).toBe(false);
    expect(isLatestFileSave(2, 2)).toBe(true);

    const current = { name: "b.txt", path: "b.txt", content: "B", savedContent: "B" };
    expect(markFileSaved(current, "a.txt", "A", "A changed")).toBe(current);
    expect(markFileSaved(current, "b.txt", "older B", "changed B")).toBe(current);
    expect(markFileSaved(current, "b.txt", "B", "changed B")).toEqual({
      ...current,
      savedContent: "changed B",
    });
  });

  it("ignores asynchronous file completions from a previous project session", () => {
    expect(isCurrentProjectSession(3, 3)).toBe(true);
    expect(isCurrentProjectSession(3, 4)).toBe(false);
  });

  it("distinguishes file proposal completions from a changed editor context", () => {
    const original = { name: "a.txt", path: "a.txt", content: "A", savedContent: "A" };
    expect(editorContextKey(original)).toBe(editorContextKey({ ...original }));
    expect(editorContextKey(original)).not.toBe(editorContextKey({ ...original, content: "edited" }));
    expect(editorContextKey(original)).not.toBe(editorContextKey({ name: "b.txt", path: "b.txt", content: "B", savedContent: "B" }));
    expect(editorContextKey(original)).not.toBe(editorContextKey(null));
  });

  it("maps a cleared Monaco selection to null", () => {
    const model = { getValueInRange: (selection: string) => selection === "selected" ? "файл номер 2" : "" };
    expect(selectedText(model, "selected")).toBe("файл номер 2");
    expect(selectedText(model, "cleared")).toBeNull();
  });
  it("renders project loading and error states", () => {
    const props = { project: null, activePath: undefined, onOpenProject: () => undefined, onOpenFile: () => undefined };
    expect(renderToStaticMarkup(<ProjectExplorer {...props} status="loading" />)).toContain("Загрузка файлов");
    expect(renderToStaticMarkup(<ProjectExplorer {...props} status="error" />)).toContain('role="alert"');
    const guarded = renderToStaticMarkup(<ProjectExplorer {...props} status="error" error="Cancel the active Terminal command before switching projects." />);
    expect(guarded).toContain("Cancel the active Terminal command before switching projects.");
  });

  it("renders file loading and error states", () => {
    const props = { file: null, saving: false, onChange: () => undefined, onSelectionChange: () => undefined, onSave: () => undefined };
    expect(renderToStaticMarkup(<Editor {...props} status="loading" />)).toContain("Загрузка файла");
    const error = renderToStaticMarkup(<Editor {...props} status="error" error="Ошибка чтения" />);
    expect(error).toContain('role="alert"');
    expect(error).toContain("Ошибка чтения");
  });

  it("disables terminal commands until a project is open", () => {
    const closed = renderToStaticMarkup(<TerminalPanel projectOpen={false} />);
    expect(closed).toContain("Откройте проект");
    expect(closed).toContain("disabled");
    const opened = renderToStaticMarkup(<TerminalPanel projectOpen />);
    expect(opened).toContain("Здесь появится вывод команды");
  });

  it("navigates terminal command history and restores the unfinished draft", () => {
    const history = ["npm test", "npm run build"];
    const previous = navigateTerminalHistory(history, history.length, "", "previous", "git status");
    expect(previous).toEqual({ command: "npm run build", index: 1, draft: "git status" });
    expect(navigateTerminalHistory(history, previous.index, previous.draft, "previous", previous.command))
      .toEqual({ command: "npm test", index: 0, draft: "git status" });
    expect(navigateTerminalHistory(history, previous.index, previous.draft, "next", previous.command))
      .toEqual({ command: "git status", index: 2, draft: "git status" });
  });

  it("keeps each reviewable terminal result paired with its completed run", () => {
    const resultA: TerminalResult = { exitCode: 0, stdout: "FIRST\n", stderr: "", cancelled: false };
    const completedA = completeTerminalRun(beginTerminalRun("echo FIRST"), resultA);
    expect(completedA).toEqual({
      status: "completed",
      transcript: { command: "echo FIRST", result: resultA },
    });

    // Editing the input is separate from the immutable completed transcript.
    const editedInput = "echo EDITED";
    expect(editedInput).toBe("echo EDITED");
    expect(completedA.status === "completed" && completedA.transcript.command).toBe("echo FIRST");

    // Starting B atomically removes A from the reviewable state.
    const runningB = beginTerminalRun("ping 127.0.0.1 -t");
    expect(runningB).toEqual({ status: "running", command: "ping 127.0.0.1 -t" });
    expect("transcript" in runningB).toBe(false);

    const cancelledB: TerminalResult = { exitCode: null, stdout: "partial\n", stderr: "", cancelled: true };
    expect(completeTerminalRun(runningB, cancelledB)).toEqual({
      status: "completed",
      transcript: { command: "ping 127.0.0.1 -t", result: cancelledB },
    });
  });
});
