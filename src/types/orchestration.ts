export type ToolKind = "file" | "terminal";
export type TaskStatus = "idle" | "thinking" | "awaiting_approval" | "running" | "awaiting_ai" | "completed" | "blocked" | "failed";
export type TaskConclusion = { outcome: "completed" | "blocked"; reason: string };
export type ExecutionPolicy = {
  modelTurns: number;
  maxModelTurns: number;
  maxActions: number;
};

export const DEFAULT_EXECUTION_LIMITS = { maxModelTurns: 12, maxActions: 8 } as const;
export const EXECUTION_LIMIT_REASON = "The orchestration execution limit was reached. Start a new task to continue.";

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
  conclusion?: TaskConclusion;
};

export type OrchestrationSnapshot = Pick<OrchestrationTask, "id" | "status" | "goal" | "conclusion" | "execution"> & {
  actions: Array<Pick<OrchestrationAction, "id" | "tool" | "status"> & { result?: Pick<OrchestrationResult, "outcome"> }>;
};

export function startTask(id: string, goal: string): OrchestrationTask {
  return { id, goal, status: "thinking", nextSequence: 1, actions: [], execution: { modelTurns: 1, ...DEFAULT_EXECUTION_LIMITS } };
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

export function finishTask(task: OrchestrationTask, conclusion: TaskConclusion): OrchestrationTask {
  return { ...task, status: conclusion.outcome, conclusion };
}

export function taskSnapshot(task: OrchestrationTask): OrchestrationSnapshot {
  return {
    id: task.id,
    status: task.status,
    goal: task.goal,
    execution: normalizedExecution(task),
    ...(task.conclusion ? { conclusion: task.conclusion } : {}),
    actions: task.actions.map(({ id, tool, status, result }) => ({ id, tool, status, ...(result ? { result: { outcome: result.outcome } } : {}) })),
  };
}

function normalizedExecution(task: OrchestrationTask): ExecutionPolicy {
  // Tasks persisted by releases before the policy was introduced remain resumable,
  // but acquire the same finite limits before another model turn is started.
  return task.execution ?? { modelTurns: Math.max(1, task.actions.length + 1), ...DEFAULT_EXECUTION_LIMITS };
}
