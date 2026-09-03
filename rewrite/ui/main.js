const form = document.querySelector("#task-form");
const result = document.querySelector("#result");

form.addEventListener("submit", async (event) => {
  event.preventDefault();
  const id = crypto.randomUUID();
  result.textContent = "Creating durable task…";
  try {
    const ledgerEvent = await window.__TAURI__.core.invoke("create_task", {
      intent: {
        contract_version: 1,
        workspace_id: "workspace-default",
        task_id: `task-${id}`,
        intent: document.querySelector("#intent").value,
        event_id: `event-${id}`,
        idempotency_key: `ui-${id}`,
        expected_revision: 0,
      },
    });
    result.textContent = `Task recorded at revision ${ledgerEvent.stream_revision}`;
    form.reset();
  } catch (error) {
    result.textContent = `Task was not created: ${error}`;
  }
});

