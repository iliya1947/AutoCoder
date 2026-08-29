import { describe, expect, it } from "vitest";
import { operationError } from "./invokeError";

describe("operationError", () => {
  it("keeps string diagnostics returned by Tauri commands", () => {
    expect(operationError("Не удалось сохранить файл.", "Destination changed on disk."))
      .toBe("Не удалось сохранить файл. Destination changed on disk.");
  });

  it("keeps Error diagnostics and avoids an empty suffix", () => {
    expect(operationError("Restore failed.", new Error("Recovery copy: backup.tmp")))
      .toBe("Restore failed. Recovery copy: backup.tmp");
    expect(operationError("Restore failed.", "   ")).toBe("Restore failed.");
  });
});
