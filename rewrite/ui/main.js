const form = document.querySelector("#task-form");
const result = document.querySelector("#result");
const intentField = document.querySelector("#intent");
const pendingKey = "autocoder.pending-create-task.v1";

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
}
