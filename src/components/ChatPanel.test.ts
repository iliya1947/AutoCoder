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
    }, null);

    expect(request).toEqual({
      messages,
      context: { openFile: { path: "src/main.ts", content: "const current = true;" } },
    });
  });

  it("includes the read-only project structure", () => {
    expect(buildChatRequest(messages, null, {
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
    expect(buildChatRequest(messages, null, null)).toEqual({ messages, context: null });
  });
});
