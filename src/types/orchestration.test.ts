import { describe, expect, it } from "vitest";
import { autonomyMode, canProposeAction, completeTask, continueTask, continuesAfterToolResult, EXECUTION_LIMIT_REASON, finishTask, markActionRunning, proposeAction, recordResult, startTask, stopTask, taskSnapshot } from "./orchestration";

describe("orchestration task state", () => {
  it("persists the selected autonomy mode and safely normalizes older tasks", () => {
    const supervised = startTask("safe", "Review every action");
    const stepByStep = startTask("manual", "Pause between steps", "step_by_step");

    expect(autonomyMode(supervised)).toBe("supervised");
    expect(continuesAfterToolResult(supervised)).toBe(true);
    expect(taskSnapshot(stepByStep).autonomy).toEqual({ mode: "step_by_step" });
    expect(continuesAfterToolResult(stepByStep)).toBe(false);
    expect(autonomyMode({ ...supervised, autonomy: undefined })).toBe("supervised");
  });
  it("stops an unfinished task without conflating it with completion or action refusal", () => {
    const proposed = proposeAction(startTask("stop-me", "Wait for review"), "terminal", { command: "npm test" });
    const stopped = stopTask(proposed.task);

    expect(stopped).toMatchObject({
      status: "stopped",
      conclusion: { outcome: "stopped", reason: "The task was stopped by the user." },
    });
    expect(stopped.actions[0].status).toBe("proposed");
    expect(stopped.actions[0].result).toBeUndefined();
    expect(taskSnapshot(stopped).conclusion?.outcome).toBe("stopped");
  });

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

  it("bounds model turns in persisted task state", () => {
    let task = startTask("bounded", "Do bounded work");
    task = { ...task, execution: { ...task.execution, maxModelTurns: 2 } };
    task = continueTask(task);
    expect(task.execution.modelTurns).toBe(2);
    task = continueTask(task);
    expect(task).toMatchObject({ status: "blocked", conclusion: { outcome: "blocked", reason: EXECUTION_LIMIT_REASON } });
    expect(taskSnapshot(task).execution).toEqual({ modelTurns: 2, maxModelTurns: 2, maxActions: 8 });
  });

  it("bounds reviewable actions independently from model turns", () => {
    const task = { ...startTask("actions", "Do work"), execution: { modelTurns: 1, maxModelTurns: 12, maxActions: 1 } };
    expect(canProposeAction(task)).toBe(true);
    expect(canProposeAction(proposeAction(task, "terminal", { command: "test" }).task)).toBe(false);
  });
});
