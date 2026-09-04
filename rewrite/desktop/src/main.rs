use autocoder_application::ApplicationShell;
use autocoder_contracts::{CreateTaskIntent, LedgerEvent};
use std::{fs, path::PathBuf, sync::Mutex};
use tauri::Manager;

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
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            fs::create_dir_all(&data_dir)?;
            let shell = ApplicationShell::open(ledger_path(data_dir))?;
            app.manage(RuntimeState(Mutex::new(shell)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![create_task])
        .run(tauri::generate_context!())
        .expect("run AutoCoder clean desktop runtime");
}

fn ledger_path(app_data_dir: PathBuf) -> PathBuf {
    app_data_dir.join("execution-ledger.sqlite")
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

    #[test]
    fn ledger_location_is_derived_only_from_tauri_app_data() {
        let app_data = std::path::PathBuf::from("stable-app-data");
        assert_eq!(
            ledger_path(app_data),
            std::path::PathBuf::from("stable-app-data/execution-ledger.sqlite")
        );
    }
}
