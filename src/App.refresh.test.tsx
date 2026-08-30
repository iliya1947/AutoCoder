// @vitest-environment jsdom
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";

const { invoke, monacoChangeCallbacks } = vi.hoisted(() => ({
  invoke: vi.fn(),
  monacoChangeCallbacks: [] as Array<(value: string) => void>,
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@monaco-editor/react", async () => {
  const React = await import("react");
  return {
    default: ({ value, onChange }: { value: string; onChange: (value: string) => void }) => {
      monacoChangeCallbacks.push(onChange);
      return React.createElement("textarea", {
        "aria-label": "Monaco editor",
        value,
        onChange: (event: React.ChangeEvent<HTMLTextAreaElement>) => onChange(event.target.value),
      });
    },
  };
});

describe("refresh with a Monaco edit", () => {
  beforeEach(() => {
    localStorage.clear();
    invoke.mockReset();
    monacoChangeCallbacks.length = 0;
  });

  it("confirms and keeps Monaco text dirty when Refresh is cancelled", async () => {
    invoke.mockImplementation((command: string) => {
      if (command === "open_project") return Promise.resolve({
        project: { name: "project", children: [{ name: "notes.txt", path: "notes.txt", kind: "file", children: [] }] },
        sessionChanged: true,
      });
      if (command === "read_project_file") return Promise.resolve({ content: "saved text" });
      if (command === "refresh_project") return Promise.resolve({
        project: { name: "project", children: [] },
        openFileContent: "disk text",
      });
      return Promise.resolve(null);
    });
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);

    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "Открыть проект" }));
    fireEvent.click(await screen.findByRole("button", { name: "notes.txt" }));
    const editor = await screen.findByRole("textbox", { name: "Monaco editor" });

    fireEvent.change(editor, { target: { value: "unsaved Monaco text" } });
    expect(screen.getByLabelText("Есть несохранённые изменения")).toBeTruthy();
    expect(new Set(monacoChangeCallbacks).size).toBe(1);
    fireEvent.click(screen.getByRole("button", { name: "Обновить" }));

    expect(confirm).toHaveBeenCalledWith("Обновить проект и отменить несохранённые изменения открытого файла?");
    expect(invoke).not.toHaveBeenCalledWith("refresh_project", expect.anything());
    await waitFor(() => expect((screen.getByRole("textbox", { name: "Monaco editor" }) as HTMLTextAreaElement).value).toBe("unsaved Monaco text"));
    expect(screen.getByLabelText("Есть несохранённые изменения")).toBeTruthy();
  });
});
