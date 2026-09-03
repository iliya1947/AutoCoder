use autocoder_contracts::{CreateTaskIntent, LedgerEvent};
use autocoder_orchestration::{OrchestrationCore, OrchestrationError};
use autocoder_persistence::SqliteLedger;
use std::path::Path;

/// UI-facing application shell. It translates an intent but owns no transitions.
pub struct ApplicationShell {
    core: OrchestrationCore<SqliteLedger>,
}

impl ApplicationShell {
    pub fn open(ledger_path: impl AsRef<Path>) -> Result<Self, autocoder_ledger::LedgerError> {
        Ok(Self {
            core: OrchestrationCore::new(SqliteLedger::open(ledger_path)?),
        })
    }
    pub fn create_task(&self, intent: CreateTaskIntent) -> Result<LedgerEvent, OrchestrationError> {
        self.core.create_task(intent)
    }
}

// Reserved ownership boundaries in the composition root. Implementations arrive as
// independent slices and must not acquire task-transition or persistence ownership.
pub mod workspace {
    pub trait Workspace {}
}
pub mod provider_runtime {
    pub trait ProviderRuntime {}
}
pub mod process_supervisor {
    pub trait ProcessSupervisor {}
}
pub mod diagnostics {
    pub trait Diagnostics {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use autocoder_contracts::*;
    use autocoder_ledger::ExecutionLedger;

    fn intent(key: &str) -> CreateTaskIntent {
        CreateTaskIntent {
            contract_version: CONTRACT_VERSION,
            workspace_id: WorkspaceId::parse("workspace-1").unwrap(),
            task_id: TaskId::parse("task-1").unwrap(),
            intent: "Create the first task".into(),
            event_id: EventId::parse("event-1").unwrap(),
            idempotency_key: IdempotencyKey::parse(key).unwrap(),
            expected_revision: 0,
        }
    }

    #[test]
    fn ui_intent_is_transitioned_by_core_and_durably_replayed() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ledger.sqlite");
        let shell = ApplicationShell::open(&path).unwrap();
        let created = shell.create_task(intent("ui-request-1")).unwrap();
        assert_eq!(created.stream_revision, 1);
        drop(shell);

        let ledger = SqliteLedger::open(&path).unwrap();
        let events = ledger.events(&TaskId::parse("task-1").unwrap()).unwrap();
        assert_eq!(events, vec![created]);
    }

    #[test]
    fn repeated_ui_intent_is_idempotent() {
        let shell = ApplicationShell::open(":memory:").unwrap();
        let first = shell.create_task(intent("ui-request-1")).unwrap();
        let repeated = shell.create_task(intent("ui-request-1")).unwrap();
        assert_eq!(first, repeated);
    }

    #[test]
    fn competing_append_is_fenced_by_stream_revision() {
        let shell = ApplicationShell::open(":memory:").unwrap();
        shell.create_task(intent("ui-request-1")).unwrap();
        let mut competing = intent("ui-request-2");
        competing.event_id = EventId::parse("event-2").unwrap();
        let error = shell.create_task(competing).unwrap_err();
        assert!(error.to_string().contains("expected 0, actual 1"));
    }

    #[test]
    fn create_cannot_be_reissued_at_a_nonzero_current_revision() {
        let shell = ApplicationShell::open(":memory:").unwrap();
        shell.create_task(intent("ui-request-1")).unwrap();
        let mut second_create = intent("ui-request-2");
        second_create.expected_revision = 1;
        let error = shell.create_task(second_create).unwrap_err();
        assert!(error.to_string().contains("requires expected revision 0"));
    }
}
