import { describe, expect, it } from "vitest";
import { completeTask, continueTask, finishTask, markActionRunning, proposeAction, recordResult, startTask, taskSnapshot } from "./orchestration";

describe("orchestration task state", () => {
  it("links every proposal and factual result across multiple approved steps", () => {
    let task = startTask("task-1", "Fix and test the project");
    const first = proposeAction(task, "file", { path: "main.ts" });
    task = markActionRunning(first.task, first.action.id);
    task = recordResult(task, { id: "result-1", actionId: first.action.id, tool: "file", outcome: "completed", content: "changed" });
    const second = proposeAction(continueTask(task), "terminal", { command: "npm test" });
    task = recordResult(markActionRunning(second.task, second.action.id), {
      id: "result-2", actionId: second.action.id, tool: "terminal", outcome: "completed", content: "passed",
    });
    task = completeTask(continueTask(task));

    expect(task.status).toBe("completed");
    expect(task.actions.map(({ id, status, result }) => [id, status, result?.actionId])).toEqual([
      ["task-1:action:1", "completed", "task-1:action:1"],
      ["task-1:action:2", "completed", "task-1:action:2"],
    ]);
    expect(taskSnapshot(task).actions).toHaveLength(2);
  });

  it("records a declined proposal as a result instead of leaving approval pending", () => {
    const proposed = proposeAction(startTask("task-2", "Try a safe command"), "terminal", { command: "npm test" }, "context-1");
    const task = recordResult(proposed.task, {
      id: "declined-1", actionId: proposed.action.id, tool: "terminal", outcome: "declined", content: "not executed",
    });

    expect(task.status).toBe("awaiting_ai");
    expect(task.actions[0]).toMatchObject({ status: "cancelled", contextKey: "context-1", result: { outcome: "declined" } });
  });

  it("persists an explicit blocked conclusion separately from an action failure", () => {
    const proposed = proposeAction(startTask("task-3", "Build the project"), "terminal", { command: "npm test" });
    const withFailure = recordResult(markActionRunning(proposed.task, proposed.action.id), {
      id: "failed-1", actionId: proposed.action.id, tool: "terminal", outcome: "failed", content: "SDK missing",
    });
    const task = finishTask(continueTask(withFailure), { outcome: "blocked", reason: "The required SDK is unavailable." });

    expect(task.status).toBe("blocked");
    expect(task.actions[0].status).toBe("failed");
    expect(taskSnapshot(task).conclusion).toEqual({ outcome: "blocked", reason: "The required SDK is unavailable." });
  });
});
