import { describe, expect, it } from "vitest";
import { buildChatRequest, chatContextKey, ChatMessage, messagesForCurrentContext } from "./ChatPanel";

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
      context: { openFile: { path: "src/main.ts", content: "const current = true;" } },
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
      path: "src/main.ts",
      content: "answer = 42",
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

    expect(request.context?.selection).toBeUndefined();
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
    expect(request.context?.selection).toBeUndefined();
    expect(request.messages).toEqual([{ role: "user", content: "Что выделено?" }]);
  });
});
