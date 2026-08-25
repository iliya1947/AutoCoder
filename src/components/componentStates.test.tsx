import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { Editor } from "./Editor";
import { ProjectExplorer } from "./ProjectExplorer";

describe("panel states", () => {
  it("renders project loading and error states", () => {
    const props = { project: null, activePath: undefined, onOpenProject: () => undefined, onOpenFile: () => undefined };
    expect(renderToStaticMarkup(<ProjectExplorer {...props} status="loading" />)).toContain("Загрузка файлов");
    expect(renderToStaticMarkup(<ProjectExplorer {...props} status="error" />)).toContain('role="alert"');
  });

  it("renders file loading and error states", () => {
    const props = { file: null, saving: false, onChange: () => undefined, onSave: () => undefined };
    expect(renderToStaticMarkup(<Editor {...props} status="loading" />)).toContain("Загрузка файла");
    const error = renderToStaticMarkup(<Editor {...props} status="error" error="Ошибка чтения" />);
    expect(error).toContain('role="alert"');
    expect(error).toContain("Ошибка чтения");
  });
});
