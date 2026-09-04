const form = document.querySelector("#task-form");
const result = document.querySelector("#result");
const intentField = document.querySelector("#intent");
const pendingKey = "autocoder.pending-create-task.v1";
const lastTaskKey = "autocoder.last-task-id.v1";
const projection = document.querySelector("#projection");

async function showProjection(taskId) {
  const task = await window.__TAURI__.core.invoke("get_task", { taskId });
  document.querySelector("#projection-task").textContent = task.task_id;
  document.querySelector("#projection-state").textContent = task.state;
  document.querySelector("#projection-revision").textContent = task.stream_revision;
  projection.hidden = false;
}

function newSubmission(intent) {
  const id = crypto.randomUUID();
  return {
    contract_version: 1,
    workspace_id: "workspace-default",
    task_id: `task-${id}`,
    intent,
    event_id: `event-${id}`,
    idempotency_key: `ui-${id}`,
    expected_revision: 0,
  };
}

async function submit(submission) {
  localStorage.setItem(pendingKey, JSON.stringify(submission));
  result.textContent = "Creating durable task…";
  try {
    const ledgerEvent = await window.__TAURI__.core.invoke("create_task", {
      intent: submission,
    });
    localStorage.removeItem(pendingKey);
    localStorage.setItem(lastTaskKey, submission.task_id);
    await showProjection(submission.task_id);
    result.textContent = `Task recorded at revision ${ledgerEvent.stream_revision}`;
    form.reset();
  } catch (error) {
    result.textContent = `Outcome not confirmed; retry will use the same identity: ${error}`;
  }
}

form.addEventListener("submit", async (event) => {
  event.preventDefault();
  const pending = localStorage.getItem(pendingKey);
  await submit(pending ? JSON.parse(pending) : newSubmission(intentField.value));
});

const pending = localStorage.getItem(pendingKey);
if (pending) {
  const submission = JSON.parse(pending);
  intentField.value = submission.intent;
  submit(submission);
} else {
  const lastTaskId = localStorage.getItem(lastTaskKey);
  if (lastTaskId) {
    showProjection(lastTaskId).catch((error) => {
      result.textContent = `Unable to read durable task: ${error}`;
    });
  }
}
