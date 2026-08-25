import { describe, expect, it } from "vitest";
import { buildChatRequest, ChatMessage } from "./ChatPanel";

describe("chat request", () => {
  const messages: ChatMessage[] = [{ role: "user", content: "Explain this file" }];

  it("includes the current unsaved open-file content", () => {
    const request = buildChatRequest(messages, {
      name: "main.ts",
      path: "src/main.ts",
      content: "const current = true;",
      savedContent: "const current = false;",
    });

    expect(request).toEqual({
      messages,
      context: { openFile: { path: "src/main.ts", content: "const current = true;" } },
    });
  });

  it("sends no context when no file is open", () => {
    expect(buildChatRequest(messages, null)).toEqual({ messages, context: null });
  });
});
