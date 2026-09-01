import { describe, expect, it } from "vitest";
import { buildChatRequest, buildLineDiff, canApplyProposal, chatContextKey, chatRequestError, ChatMessage, formatActionLifecycleResult, formatFileToolResult, formatTerminalResultDraft, formatTerminalToolResult, isChatResponseCurrent, messagesForCurrentContext, taskProgress, terminalResultMatchesAction } from "./ChatPanel";
import { proposeAction, recordResult, startTask, stopTask, taskSnapshot } from "../types/orchestration";

describe("chat request", () => {
  const messages: ChatMessage[] = [{ role: "user", content: "Explain this file" }];

  it("renders a user-stopped task as its own terminal progress state", () => {
    const progress = taskProgress(stopTask(startTask("stopped", "Stop safely")), false);

    expect(progress).toMatchObject({ tone: "stopped", statusKey: "task.status_stopped", nextKey: "task.next_new" });
  });

  it("includes the persisted autonomy policy in the Tauri chat request", () => {
    const request = buildChatRequest(messages, null, null, null, taskSnapshot(
      startTask("task-step", "Work one step at a time", "step_by_step"),
    ));

    expect(request.orchestration?.autonomy).toEqual({ mode: "step_by_step" });
  });

  it("sends factual action payloads and results in the orchestration snapshot", () => {
    const initial = startTask("task-history", "Append the requested line");
    const proposed = proposeAction(initial, "terminal", { command: "echo actual payload" }, "context", "requirement-2").task;
    const completed = recordResult(proposed, {
      id: "result-1", actionId: "task-history:action:1", tool: "terminal", outcome: "completed",
      content: "Command: echo actual payload\nStatus: exit code: 0",
    });

    expect(taskSnapshot(completed).actions).toEqual([{
      id: "task-history:action:1", tool: "terminal", payload: { command: "echo actual payload" }, requirementId: "requirement-2", status: "completed",
      result: {
        id: "result-1", actionId: "task-history:action:1", tool: "terminal", outcome: "completed",
        content: "Command: echo actual payload\nStatus: exit code: 0",
      },
    }]);
  });

  it("invalidates chat context when the saved disk baseline changes", () => {
    const file = { name: "a.ts", path: "a.ts", content: "edited", savedContent: "old" };
    expect(chatContextKey(file, null, null)).not.toBe(chatContextKey({ ...file, savedContent: "edited" }, null, null));
  });

  it("formats terminal output as an editable chat draft without sending it", () => {
    expect(formatTerminalResultDraft({
      command: "npm test",
      result: { exitCode: 1, stdout: "one passed", stderr: "one failed", cancelled: false },
    })).toBe("Command: npm test\n\nStatus: exit code: 1\n\nstdout:\none passed\n\nstderr:\none failed");
    expect(formatTerminalResultDraft({
      command: "long task",
      result: { exitCode: null, stdout: "partial", stderr: "", cancelled: true },
    })).toBe("Command: long task\n\nStatus: cancelled\n\nstdout:\npartial");
  });

  it("formats factual tool feedback for the next orchestration turn", () => {
    expect(formatFileToolResult({ operation: "replace", path: "main.ts", originalContent: "old", content: "new" }, "completed"))
      .toContain("updated the editor buffer for main.ts (not saved to disk yet)");
    const terminalFeedback = formatTerminalToolResult({
      command: "npm test",
      result: { exitCode: 0, stdout: "passed", stderr: "", cancelled: false },
    });
    expect(terminalFeedback).toContain("AutoCoder Terminal Tool result");
    expect(terminalFeedback).toContain("Status: exit code: 0");
    expect(terminalFeedback).toContain("propose exactly one next File Tool or Terminal Tool action");
  });

  it("accepts a terminal result only for its exact action and executed command", () => {
    const action = { id: "task:action:2", tool: "terminal" as const, payload: { command: "type README.md" }, status: "running" as const, contextKey: "context" };
    const exact = { id: 4, actionId: action.id, tool: "terminal" as const, command: "type README.md", content: "result" };

    expect(terminalResultMatchesAction(exact, action)).toBe(true);
    expect(terminalResultMatchesAction({ ...exact, actionId: "task:action:1" }, action)).toBe(false);
    expect(terminalResultMatchesAction({ ...exact, command: "echo stale" }, action)).toBe(false);
  });

  it("summarizes persisted task transitions without treating declined actions as completed", () => {
    const task = {
      id: "task-1", goal: "Fix and verify", status: "awaiting_approval" as const, nextSequence: 2,
      execution: { modelTurns: 1, maxModelTurns: 12, maxActions: 8 },
      actions: [{ id: "action-1", tool: "terminal" as const, payload: { command: "npm test" }, status: "proposed" as const, contextKey: "context" }],
    };
    expect(taskProgress(task, false)).toMatchObject({
      tone: "waiting", completedSteps: 0, totalSteps: 1,
      waitingKey: "task.waiting_approval", nextKey: "task.next_approval", tool: "terminal",
    });
    expect(taskProgress({ ...task, status: "running", actions: [{ ...task.actions[0], status: "running" }] }, false, true))
      .toMatchObject({ tone: "waiting", waitingKey: "task.waiting_safety", nextKey: "task.next_safety" });
  });

  it("shows explicit persisted conclusions", () => {
    const completed = {
      id: "task-2", goal: "Build", status: "completed" as const, nextSequence: 1, actions: [],
      execution: { modelTurns: 1, maxModelTurns: 12, maxActions: 8 },
      conclusion: { outcome: "completed" as const, reason: "Build passed." },
    };
    expect(taskProgress(completed, false)).toMatchObject({ tone: "completed", statusKey: "task.status_completed", nextKey: "task.next_done" });
    expect(taskProgress({ ...completed, status: "blocked", conclusion: { outcome: "blocked", reason: "SDK missing." } }, false))
      .toMatchObject({ tone: "blocked", statusKey: "task.status_blocked", nextKey: "task.next_blocked" });
  });

  it("formats explicit decline and uncertain restart results", () => {
    expect(formatActionLifecycleResult("file", "declined")).toContain("was not executed");
    expect(formatActionLifecycleResult("terminal", "interrupted")).toContain("will not run it again");
  });

  it("includes the current unsaved open-file content", () => {
    const request = buildChatRequest(messages, {
      name: "main.ts",
      path: "src/main.ts",
      content: "const current = true;",
      savedContent: "const current = false;",
    }, null, null);

    expect(request).toEqual({
      messages,
      context: {
        openFile: { path: "src/main.ts", content: "const current = true;", savedContent: "const current = false;" },
        selection: { state: "none" },
      },
    });
  });

  it("tells the model when Terminal deleted a file whose dirty buffer was protected", () => {
    const request = buildChatRequest(messages, {
      name: "notes.txt", path: "notes.txt", content: "unsaved draft", savedContent: "disk before deletion", existsOnDisk: false,
    }, null, { name: "project", children: [] });

    expect(request.context?.openFile).toEqual({
      path: "notes.txt", content: "unsaved draft", savedContent: "disk before deletion", existsOnDisk: false,
    });
    expect(chatContextKey({ name: "notes.txt", path: "notes.txt", content: "unsaved draft", savedContent: "disk before deletion" }, null, null))
      .not.toBe(chatContextKey({ name: "notes.txt", path: "notes.txt", content: "unsaved draft", savedContent: "disk before deletion", existsOnDisk: false }, null, null));
  });

  it("includes the read-only project structure", () => {
    expect(buildChatRequest(messages, null, null, {
      name: "AutoCoder",
      children: [{
        name: "src", path: "src", kind: "directory", children: [
          { name: "main.ts", path: "src/main.ts", kind: "file", children: [] },
        ],
      }],
    })).toEqual({
      messages,
      context: { project: { name: "AutoCoder", entries: ["directory: src", "file: src/main.ts"] } },
    });
  });

  it("sends no context when no project or file is open", () => {
    expect(buildChatRequest(messages, null, null, null)).toEqual({ messages, context: null });
  });

  it("includes the current editor selection with its file path", () => {
    const file = {
      name: "main.ts",
      path: "src/main.ts",
      content: "const answer = 42;",
      savedContent: "const answer = 42;",
    };

    expect(buildChatRequest(messages, file, "answer = 42", null).context?.selection).toEqual({
      state: "active",
      path: "src/main.ts",
      content: "answer = 42",
    });
  });

  it("explicitly distinguishes no active selection from open-file content", () => {
    const file = {
      name: "two.txt",
      path: "two.txt",
      content: "Тестовый файл номер 2",
      savedContent: "Тестовый файл номер 2",
    };

    expect(buildChatRequest(messages, file, null, null).context).toEqual({
      openFile: { path: "two.txt", content: "Тестовый файл номер 2", savedContent: "Тестовый файл номер 2" },
      selection: { state: "none" },
    });
  });

  it("does not resend selection-derived history after the selection is cleared", () => {
    const oldHistory: ChatMessage[] = [
      { role: "user", content: "Что выделено?" },
      { role: "assistant", content: "файл номер 2" },
    ];
    const file = { name: "two.txt", path: "two.txt", content: "Тестовый файл номер 2", savedContent: "Тестовый файл номер 2" };
    const previousKey = chatContextKey(file, "файл номер 2", null);
    const currentKey = chatContextKey(file, null, null);

    const request = buildChatRequest(
      messagesForCurrentContext(oldHistory, "Что сейчас выделено?", previousKey, currentKey),
      file,
      null,
      null,
    );

    expect(request.context?.selection).toEqual({ state: "none" });
    expect(request.context?.openFile?.content).toBe("Тестовый файл номер 2");
    expect(request.messages).toEqual([{ role: "user", content: "Что сейчас выделено?" }]);
    expect(JSON.stringify(request.messages)).not.toContain("файл номер 2");
  });

  it("does not resend selection-derived history after another file is opened", () => {
    const first = { name: "one.txt", path: "one.txt", content: "old", savedContent: "old" };
    const second = { name: "two.txt", path: "two.txt", content: "new", savedContent: "new" };
    const history: ChatMessage[] = [{ role: "assistant", content: "old selection" }];
    const requestMessages = messagesForCurrentContext(
      history,
      "Что выделено?",
      chatContextKey(first, "old selection", null),
      chatContextKey(second, null, null),
    );

    const request = buildChatRequest(requestMessages, second, null, null);
    expect(request.context?.selection).toEqual({ state: "none" });
    expect(request.messages).toEqual([{ role: "user", content: "Что выделено?" }]);
  });

  it("does not resend history after the open file content changes", () => {
    const original = { name: "main.ts", path: "main.ts", content: "const value = 1;", savedContent: "const value = 1;" };
    const edited = { ...original, content: "const value = 2;" };
    const history: ChatMessage[] = [{ role: "assistant", content: "The value is 1." }];

    expect(messagesForCurrentContext(
      history,
      "What is the value now?",
      chatContextKey(original, null, null),
      chatContextKey(edited, null, null),
    )).toEqual([{ role: "user", content: "What is the value now?" }]);
  });

  it("does not resend history after the project structure changes", () => {
    const originalProject = { name: "demo", children: [] };
    const updatedProject = {
      name: "demo",
      children: [{ name: "new.ts", path: "new.ts", kind: "file" as const, children: [] }],
    };

    expect(messagesForCurrentContext(
      [{ role: "assistant", content: "The project is empty." }],
      "List the files now.",
      chatContextKey(null, null, originalProject),
      chatContextKey(null, null, updatedProject),
    )).toEqual([{ role: "user", content: "List the files now." }]);
  });

  it("rejects a response when its editor or project context changed in flight", () => {
    const original = { name: "main.ts", path: "main.ts", content: "old", savedContent: "old" };
    const edited = { ...original, content: "new" };
    const project = { name: "demo", children: [] };

    expect(isChatResponseCurrent(
      chatContextKey(original, null, project),
      chatContextKey(original, null, project),
    )).toBe(true);
    expect(isChatResponseCurrent(
      chatContextKey(original, null, project),
      chatContextKey(edited, null, project),
    )).toBe(false);
    expect(isChatResponseCurrent(
      chatContextKey(original, null, project),
      chatContextKey(original, null, { name: "demo", children: [{ name: "new.ts", path: "new.ts", kind: "file", children: [] }] }),
    )).toBe(false);
  });

  it("does not show an obsolete backend failure after the context changed", () => {
    expect(chatRequestError("old", "new", "Ollama unavailable", "Context changed"))
      .toBe("Context changed");
    expect(chatRequestError("same", "same", new Error("Ollama unavailable"), "Context changed"))
      .toBe("Ollama unavailable");
  });

  it("applies a proposal only to the unchanged source file", () => {
    const proposal = { operation: "replace" as const, path: "src/main.ts", originalContent: "old", content: "new" };

    expect(canApplyProposal({ name: "main.ts", path: "src/main.ts", content: "old", savedContent: "old" }, proposal)).toBe(true);
    expect(canApplyProposal({ name: "main.ts", path: "src/main.ts", content: "edited", savedContent: "old" }, proposal)).toBe(false);
    expect(canApplyProposal({ name: "other.ts", path: "src/other.ts", content: "old", savedContent: "old" }, proposal)).toBe(false);
    expect(canApplyProposal(null, { operation: "create", path: "src/new.ts", content: "new" })).toBe(true);
    const deletion = { operation: "delete" as const, path: "src/main.ts", originalContent: "old", expectedSavedContent: "old" };
    expect(canApplyProposal({ name: "main.ts", path: "src/main.ts", content: "old", savedContent: "old" }, deletion)).toBe(true);
    expect(canApplyProposal({ name: "main.ts", path: "src/main.ts", content: "old", savedContent: "older" }, deletion)).toBe(false);
    expect(canApplyProposal({ name: "main.ts", path: "src/main.ts", content: "edited", savedContent: "edited" }, deletion)).toBe(false);
    expect(canApplyProposal({ name: "main.ts", path: "src/main.ts", content: "old", savedContent: "external" }, deletion)).toBe(false);
  });

  it("builds a line diff with stable old and new line numbers", () => {
    expect(buildLineDiff("first\nold\nlast", "first\nnew\nlast\nextra")).toEqual([
      { kind: "context", content: "first", oldLine: 1, newLine: 1 },
      { kind: "removed", content: "old", oldLine: 2, newLine: null },
      { kind: "added", content: "new", oldLine: null, newLine: 2 },
      { kind: "context", content: "last", oldLine: 3, newLine: 3 },
      { kind: "added", content: "extra", oldLine: null, newLine: 4 },
    ]);
  });

  it("keeps unchanged Russian lines as context while reconstructing an LCS diff", () => {
    const original = [
      "Первая строка",
      "Вторая строка",
      "Третья строка",
      "Четвертая строка",
      "Пятая строка",
    ].join("\n");
    const proposed = [
      "Первая строка",
      "Вторая строка изменена",
      "Третья строка",
      "Пятая строка",
      "Шестая строка",
    ].join("\n");

    expect(buildLineDiff(original, proposed)).toEqual([
      { kind: "context", content: "Первая строка", oldLine: 1, newLine: 1 },
      { kind: "removed", content: "Вторая строка", oldLine: 2, newLine: null },
      { kind: "added", content: "Вторая строка изменена", oldLine: null, newLine: 2 },
      { kind: "context", content: "Третья строка", oldLine: 3, newLine: 3 },
      { kind: "removed", content: "Четвертая строка", oldLine: 4, newLine: null },
      { kind: "context", content: "Пятая строка", oldLine: 5, newLine: 4 },
      { kind: "added", content: "Шестая строка", oldLine: null, newLine: 5 },
    ]);
  });

  it("represents an empty-file replacement without a phantom line", () => {
    expect(buildLineDiff("", "created")).toEqual([
      { kind: "added", content: "created", oldLine: null, newLine: 1 },
    ]);
  });
});
