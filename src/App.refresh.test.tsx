// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";

const { confirm, invoke, monacoChangeCallbacks, operationOrder } = vi.hoisted(() => ({
  confirm: vi.fn(),
  invoke: vi.fn(),
  monacoChangeCallbacks: [] as Array<(value: string) => void>,
  operationOrder: [] as string[],
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ confirm }));
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
  afterEach(cleanup);
  beforeEach(() => {
    localStorage.clear();
    invoke.mockReset();
    confirm.mockReset();
    monacoChangeCallbacks.length = 0;
    operationOrder.length = 0;
  });

  it("restores the persisted project and saved disk file without a folder dialog", async () => {
    invoke.mockImplementation((command: string) => {
      if (command === "restore_workspace") return Promise.resolve({
        project: { name: "restored", children: [{ name: "saved.txt", path: "saved.txt", kind: "file", children: [] }] },
        openFile: { path: "saved.txt", content: "disk content" },
      });
      if (command === "load_project_history") return Promise.resolve({ chatMessages: [], terminalRuns: [] });
      return Promise.resolve(null);
    });

    render(<App />);

    expect(await screen.findByRole("button", { name: "saved.txt" })).toBeTruthy();
    expect((await screen.findByRole("textbox", { name: "Monaco editor" }) as HTMLTextAreaElement).value).toBe("disk content");
    expect(screen.queryByLabelText("Есть несохранённые изменения")).toBeNull();
    expect(invoke).not.toHaveBeenCalledWith("open_project", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("remember_project_file", expect.anything());
  });

  it("requires the native packaged-app confirmation before manual deletion", async () => {
    invoke.mockImplementation((command: string) => {
      if (command === "restore_workspace") return Promise.resolve({
        project: { name: "restored", children: [{ name: "assets", path: "assets", kind: "directory", children: [] }] },
        openFile: null,
      });
      if (command === "load_project_history") return Promise.resolve({ chatMessages: [], terminalRuns: [] });
      return Promise.resolve(null);
    });
    confirm.mockResolvedValue(false);
    render(<App />);

    fireEvent.click(await screen.findByRole("button", { name: "assets" }));
    fireEvent.click(screen.getByRole("button", { name: "Удалить" }));

    await waitFor(() => expect(confirm).toHaveBeenCalledWith(
      expect.stringContaining("assets"),
      { title: "AutoCoder", kind: "warning" },
    ));
    expect(invoke).not.toHaveBeenCalledWith("delete_project_entry", expect.anything());
  });

  it("confirms and keeps Monaco text dirty when Refresh is cancelled", async () => {
    invoke.mockImplementation((command: string) => {
      operationOrder.push(command);
      if (command === "open_project") return Promise.resolve({
        project: { name: "project", children: [{ name: "notes.txt", path: "notes.txt", kind: "file", children: [] }] },
        sessionChanged: true,
      });
      if (command === "read_project_file") return Promise.resolve({ content: "saved text" });
      if (command === "load_project_history") return Promise.resolve({ chatMessages: [], terminalRuns: [] });
      if (command === "refresh_project") return Promise.resolve({
        project: { name: "project", children: [] },
        openFileContent: "disk text",
      });
      return Promise.resolve(null);
    });
    confirm.mockImplementation(async () => {
      operationOrder.push("confirm");
      return false;
    });

    render(<App />);
    const openProject = screen.getByRole("button", { name: "Открыть проект" });
    await waitFor(() => expect((openProject as HTMLButtonElement).disabled).toBe(false));
    fireEvent.click(openProject);
    fireEvent.click(await screen.findByRole("button", { name: "notes.txt" }));
    const editor = await screen.findByRole("textbox", { name: "Monaco editor" });

    fireEvent.change(editor, { target: { value: "unsaved Monaco text" } });
    expect(screen.getByLabelText("Есть несохранённые изменения")).toBeTruthy();
    expect(new Set(monacoChangeCallbacks).size).toBe(1);
    fireEvent.click(screen.getByRole("button", { name: "Обновить" }));

    await waitFor(() => expect(confirm).toHaveBeenCalledWith(
      "Обновить проект и отменить несохранённые изменения открытого файла?",
      { title: "AutoCoder", kind: "warning" },
    ));
    expect(invoke).not.toHaveBeenCalledWith("refresh_project", expect.anything());
    expect(operationOrder).toEqual(["restore_workspace", "open_project", "load_project_history", "load_project_history", "read_project_file", "remember_project_file", "confirm"]);
    await waitFor(() => expect((screen.getByRole("textbox", { name: "Monaco editor" }) as HTMLTextAreaElement).value).toBe("unsaved Monaco text"));
    expect(screen.getByLabelText("Есть несохранённые изменения")).toBeTruthy();
  });
});
