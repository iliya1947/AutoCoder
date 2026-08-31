export type ToolKind = "file" | "terminal";
export type TaskStatus = "idle" | "thinking" | "awaiting_approval" | "running" | "awaiting_ai" | "completed" | "failed";

export type OrchestrationResult = {
  id: string;
  actionId: string;
  tool: ToolKind;
  outcome: "completed" | "failed" | "cancelled" | "declined" | "interrupted";
  content: string;
};

export type OrchestrationAction<T = unknown> = {
  id: string;
  tool: ToolKind;
  payload: T;
  status: "proposed" | "running" | "completed" | "failed" | "cancelled";
  contextKey: string;
  result?: OrchestrationResult;
};

export type OrchestrationTask = {
  id: string;
  status: TaskStatus;
  goal: string;
  nextSequence: number;
  actions: OrchestrationAction[];
};

export type OrchestrationSnapshot = Pick<OrchestrationTask, "id" | "status" | "goal"> & {
  actions: Array<Pick<OrchestrationAction, "id" | "tool" | "status"> & { result?: Pick<OrchestrationResult, "outcome"> }>;
};

export function startTask(id: string, goal: string): OrchestrationTask {
  return { id, goal, status: "thinking", nextSequence: 1, actions: [] };
}

export function continueTask(task: OrchestrationTask): OrchestrationTask {
  return { ...task, status: "thinking" };
}

export function proposeAction<T>(task: OrchestrationTask, tool: ToolKind, payload: T, contextKey = ""): { task: OrchestrationTask; action: OrchestrationAction<T> } {
  const action: OrchestrationAction<T> = { id: `${task.id}:action:${task.nextSequence}`, tool, payload, contextKey, status: "proposed" };
  return { action, task: { ...task, status: "awaiting_approval", nextSequence: task.nextSequence + 1, actions: [...task.actions, action] } };
}

export function markActionRunning(task: OrchestrationTask, actionId: string): OrchestrationTask {
  return { ...task, status: "running", actions: task.actions.map((action) => action.id === actionId ? { ...action, status: "running" } : action) };
}

export function recordResult(task: OrchestrationTask, result: OrchestrationResult): OrchestrationTask {
  const status = result.outcome === "completed" ? "completed" : result.outcome === "declined" || result.outcome === "interrupted" ? "cancelled" : result.outcome;
  return {
    ...task,
    status: "awaiting_ai",
    actions: task.actions.map((action) => action.id === result.actionId ? { ...action, status, result } : action),
  };
}

export function completeTask(task: OrchestrationTask): OrchestrationTask {
  return { ...task, status: "completed" };
}

export function taskSnapshot(task: OrchestrationTask): OrchestrationSnapshot {
  return {
    id: task.id,
    status: task.status,
    goal: task.goal,
    actions: task.actions.map(({ id, tool, status, result }) => ({ id, tool, status, ...(result ? { result: { outcome: result.outcome } } : {}) })),
  };
}
