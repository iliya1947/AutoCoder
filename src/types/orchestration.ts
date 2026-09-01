export type ToolKind = "file" | "terminal";
export type TaskStatus = "idle" | "thinking" | "awaiting_approval" | "running" | "awaiting_ai" | "completed" | "blocked" | "stopped" | "failed";
export type TaskConclusion = { outcome: "completed" | "blocked" | "stopped"; reason: string };
export type ExecutionPolicy = {
  modelTurns: number;
  maxModelTurns: number;
  maxActions: number;
};
export type AutonomyMode = "supervised" | "step_by_step";

export const DEFAULT_EXECUTION_LIMITS = { maxModelTurns: 12, maxActions: 8 } as const;
export const EXECUTION_LIMIT_REASON = "The orchestration execution limit was reached. Start a new task to continue.";
export const USER_STOP_REASON = "The task was stopped by the user.";

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
  execution: ExecutionPolicy;
  autonomy?: { mode: AutonomyMode };
  conclusion?: TaskConclusion;
};

export type OrchestrationSnapshot = Pick<OrchestrationTask, "id" | "status" | "goal" | "conclusion" | "execution"> & {
  autonomy: { mode: AutonomyMode };
  actions: Array<Pick<OrchestrationAction, "id" | "tool" | "payload" | "status"> & { result?: OrchestrationResult }>;
};

export function startTask(id: string, goal: string, mode: AutonomyMode = "supervised"): OrchestrationTask {
  return { id, goal, status: "thinking", nextSequence: 1, actions: [], execution: { modelTurns: 1, ...DEFAULT_EXECUTION_LIMITS }, autonomy: { mode } };
}

export function continueTask(task: OrchestrationTask): OrchestrationTask {
  const execution = normalizedExecution(task);
  if (execution.modelTurns >= execution.maxModelTurns) {
    return finishTask({ ...task, execution }, { outcome: "blocked", reason: EXECUTION_LIMIT_REASON });
  }
  return { ...task, status: "thinking", execution: { ...execution, modelTurns: execution.modelTurns + 1 } };
}

export function canProposeAction(task: OrchestrationTask): boolean {
  return task.actions.length < normalizedExecution(task).maxActions;
}

export function blockAtExecutionLimit(task: OrchestrationTask): OrchestrationTask {
  return finishTask({ ...task, execution: normalizedExecution(task) }, { outcome: "blocked", reason: EXECUTION_LIMIT_REASON });
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
  return finishTask(task, { outcome: "completed", reason: "The model reported that the goal was completed." });
}

export function stopTask(task: OrchestrationTask): OrchestrationTask {
  if (isTaskFinished(task)) return task;
  return finishTask(task, { outcome: "stopped", reason: USER_STOP_REASON });
}

export function isTaskFinished(task: OrchestrationTask): boolean {
  return ["completed", "blocked", "stopped", "failed"].includes(task.status);
}

export function finishTask(task: OrchestrationTask, conclusion: TaskConclusion): OrchestrationTask {
  return { ...task, status: conclusion.outcome, conclusion };
}

export function taskSnapshot(task: OrchestrationTask): OrchestrationSnapshot {
  return {
    id: task.id,
    status: task.status,
    goal: task.goal,
    execution: normalizedExecution(task),
    autonomy: normalizedAutonomy(task),
    ...(task.conclusion ? { conclusion: task.conclusion } : {}),
    // The backend prompt needs the action evidence, not only lifecycle labels:
    // an exit code cannot tell the model what command actually ran, and a
    // completed file action does not identify the content that was applied.
    actions: task.actions.map(({ id, tool, payload, status, result }) => ({ id, tool, payload, status, ...(result ? { result } : {}) })),
  };
}

export function autonomyMode(task: OrchestrationTask): AutonomyMode {
  return normalizedAutonomy(task).mode;
}

export function continuesAfterToolResult(task: OrchestrationTask): boolean {
  return autonomyMode(task) === "supervised";
}

function normalizedAutonomy(task: OrchestrationTask): { mode: AutonomyMode } {
  // Existing persisted tasks keep the historical, approval-gated behaviour.
  return task.autonomy ?? { mode: "supervised" };
}

function normalizedExecution(task: OrchestrationTask): ExecutionPolicy {
  // Tasks persisted by releases before the policy was introduced remain resumable,
  // but acquire the same finite limits before another model turn is started.
  return task.execution ?? { modelTurns: Math.max(1, task.actions.length + 1), ...DEFAULT_EXECUTION_LIMITS };
}
