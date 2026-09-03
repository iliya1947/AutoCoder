use autocoder_application::ApplicationShell;
use autocoder_contracts::{CreateTaskIntent, LedgerEvent};
use std::sync::Mutex;

struct RuntimeState(Mutex<ApplicationShell>);

fn dispatch_create_task(
    shell: &ApplicationShell,
    intent: CreateTaskIntent,
) -> Result<LedgerEvent, String> {
    shell.create_task(intent).map_err(|error| error.to_string())
}

#[tauri::command]
fn create_task(
    state: tauri::State<'_, RuntimeState>,
    intent: CreateTaskIntent,
) -> Result<LedgerEvent, String> {
    let shell = state.0.lock().map_err(|_| "runtime state poisoned")?;
    dispatch_create_task(&shell, intent)
}

fn main() {
    let data_dir = std::env::current_dir()
        .expect("current directory")
        .join("autocoder-clean-runtime.sqlite");
    let shell = ApplicationShell::open(data_dir).expect("open execution ledger");
    tauri::Builder::default()
        .manage(RuntimeState(Mutex::new(shell)))
        .invoke_handler(tauri::generate_handler![create_task])
        .run(tauri::generate_context!())
        .expect("run AutoCoder clean desktop runtime");
}

#[cfg(test)]
mod tests {
    use super::*;
    use autocoder_contracts::*;

    #[test]
    fn desktop_command_dispatches_versioned_ui_intent() {
        let shell = ApplicationShell::open(":memory:").unwrap();
        let event = dispatch_create_task(
            &shell,
            CreateTaskIntent {
                contract_version: CONTRACT_VERSION,
                workspace_id: WorkspaceId::parse("workspace-desktop").unwrap(),
                task_id: TaskId::parse("task-desktop").unwrap(),
                intent: "Start clean runtime".into(),
                event_id: EventId::parse("event-desktop").unwrap(),
                idempotency_key: IdempotencyKey::parse("request-desktop").unwrap(),
                expected_revision: 0,
            },
        )
        .unwrap();
        assert_eq!(event.stream_revision, 1);
    }
}
