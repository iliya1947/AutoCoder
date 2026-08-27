import { describe, expect, it } from "vitest";
import { buildChatRequest, buildLineDiff, canApplyProposal, chatContextKey, ChatMessage, messagesForCurrentContext } from "./ChatPanel";

describe("chat request", () => {
  const messages: ChatMessage[] = [{ role: "user", content: "Explain this file" }];

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
        openFile: { path: "src/main.ts", content: "const current = true;" },
        selection: { state: "none" },
      },
    });
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
      openFile: { path: "two.txt", content: "Тестовый файл номер 2" },
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

  it("applies a proposal only to the unchanged source file", () => {
    const proposal = { path: "src/main.ts", originalContent: "old", content: "new" };

    expect(canApplyProposal({ name: "main.ts", path: "src/main.ts", content: "old", savedContent: "old" }, proposal)).toBe(true);
    expect(canApplyProposal({ name: "main.ts", path: "src/main.ts", content: "edited", savedContent: "old" }, proposal)).toBe(false);
    expect(canApplyProposal({ name: "other.ts", path: "src/other.ts", content: "old", savedContent: "old" }, proposal)).toBe(false);
  });

  it("builds a line diff with stable old and new line numbers", () => {
    expect(buildLineDiff("first\nold\nlast", "first\nnew\nlast\nextra")).toEqual([
      { kind: "context", content: "first", oldLine: 1, newLine: 1 },
      { kind: "added", content: "new", oldLine: null, newLine: 2 },
      { kind: "removed", content: "old", oldLine: 2, newLine: null },
      { kind: "context", content: "last", oldLine: 3, newLine: 3 },
      { kind: "added", content: "extra", oldLine: null, newLine: 4 },
    ]);
  });

  it("represents an empty-file replacement without a phantom line", () => {
    expect(buildLineDiff("", "created")).toEqual([
      { kind: "added", content: "created", oldLine: null, newLine: 1 },
    ]);
  });
});
