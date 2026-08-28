import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { Editor, selectedText } from "./Editor";
import { ProjectExplorer } from "./ProjectExplorer";
import { TerminalPanel } from "./TerminalPanel";

describe("panel states", () => {
  it("maps a cleared Monaco selection to null", () => {
    const model = { getValueInRange: (selection: string) => selection === "selected" ? "файл номер 2" : "" };
    expect(selectedText(model, "selected")).toBe("файл номер 2");
    expect(selectedText(model, "cleared")).toBeNull();
  });
  it("renders project loading and error states", () => {
    const props = { project: null, activePath: undefined, onOpenProject: () => undefined, onOpenFile: () => undefined };
    expect(renderToStaticMarkup(<ProjectExplorer {...props} status="loading" />)).toContain("Загрузка файлов");
    expect(renderToStaticMarkup(<ProjectExplorer {...props} status="error" />)).toContain('role="alert"');
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
});
