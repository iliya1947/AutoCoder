use autocoder_contracts::{
    CompleteTaskIntent, CreateTaskIntent, LedgerEvent, RecordVerificationIntent, TaskId,
    TaskProjection, TransitionTaskIntent,
};
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
    pub fn task(&self, task_id: &TaskId) -> Result<TaskProjection, OrchestrationError> {
        self.core.task(task_id)
    }
    pub fn transition_task(
        &self,
        intent: TransitionTaskIntent,
    ) -> Result<LedgerEvent, OrchestrationError> {
        self.core.transition_task(intent)
    }
    pub fn record_verification(
        &self,
        intent: RecordVerificationIntent,
    ) -> Result<LedgerEvent, OrchestrationError> {
        self.core.record_verification(intent)
    }
    pub fn complete_task(
        &self,
        intent: CompleteTaskIntent,
    ) -> Result<LedgerEvent, OrchestrationError> {
        self.core.complete_task(intent)
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
            input_revision: InputRevision::parse("workspace-snapshot-1").unwrap(),
            event_id: EventId::parse("event-1").unwrap(),
            idempotency_key: IdempotencyKey::parse(key).unwrap(),
            expected_revision: 0,
        }
    }

    fn transition(state: TaskState, revision: u64, event: &str, key: &str) -> TransitionTaskIntent {
        TransitionTaskIntent {
            contract_version: CONTRACT_VERSION,
            task_id: TaskId::parse("task-1").unwrap(),
            target_state: state,
            event_id: EventId::parse(event).unwrap(),
            idempotency_key: IdempotencyKey::parse(key).unwrap(),
            expected_revision: revision,
        }
    }

    fn basis(task: &str, created_event: &str, input: &str) -> VerificationBasis {
        VerificationBasis {
            schema_version: CONTRACT_VERSION,
            task_id: TaskId::parse(task).unwrap(),
            task_created_event_id: EventId::parse(created_event).unwrap(),
            workspace_id: WorkspaceId::parse("workspace-1").unwrap(),
            input_revision: InputRevision::parse(input).unwrap(),
        }
    }

    fn verification(outcome: VerificationOutcome, revision: u64) -> RecordVerificationIntent {
        RecordVerificationIntent {
            contract_version: CONTRACT_VERSION,
            task_id: TaskId::parse("task-1").unwrap(),
            evidence: SemanticVerificationEvidence {
                schema_version: CONTRACT_VERSION,
                evidence_id: EvidenceId::parse("evidence-1").unwrap(),
                basis: basis("task-1", "event-1", "workspace-snapshot-1"),
                outcome,
                provenance: VerificationProvenance {
                    verifier: "autocoder.semantic-verifier".into(),
                    verifier_version: "1.0.0".into(),
                    method: "acceptance-contract".into(),
                },
                summary: "requirements satisfied".into(),
            },
            event_id: EventId::parse("verification-event").unwrap(),
            idempotency_key: IdempotencyKey::parse("verification-request").unwrap(),
            expected_revision: revision,
        }
    }

    fn completion(revision: u64) -> CompleteTaskIntent {
        CompleteTaskIntent {
            contract_version: CONTRACT_VERSION,
            task_id: TaskId::parse("task-1").unwrap(),
            evidence_id: EvidenceId::parse("evidence-1").unwrap(),
            basis: basis("task-1", "event-1", "workspace-snapshot-1"),
            event_id: EventId::parse("completion-event").unwrap(),
            idempotency_key: IdempotencyKey::parse("completion-request").unwrap(),
            expected_revision: revision,
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

    #[test]
    fn lifecycle_projection_is_rebuilt_after_store_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ledger.sqlite");
        let shell = ApplicationShell::open(&path).unwrap();
        shell.create_task(intent("create-request")).unwrap();
        shell
            .transition_task(transition(TaskState::Ready, 1, "event-2", "ready-request"))
            .unwrap();
        shell
            .transition_task(transition(
                TaskState::Blocked,
                2,
                "event-3",
                "blocked-request",
            ))
            .unwrap();
        drop(shell);

        let reopened = ApplicationShell::open(&path).unwrap();
        let projection = reopened.task(&TaskId::parse("task-1").unwrap()).unwrap();
        assert_eq!(projection.state, TaskState::Blocked);
        assert_eq!(projection.stream_revision, 3);
        assert_eq!(projection.intent, "Create the first task");
    }

    #[test]
    fn core_accepts_defined_transitions_and_rejects_invalid_ones() {
        let shell = ApplicationShell::open(":memory:").unwrap();
        shell.create_task(intent("create-request")).unwrap();
        let invalid = shell
            .transition_task(transition(
                TaskState::Completed,
                1,
                "event-2",
                "complete-request",
            ))
            .unwrap_err();
        assert!(matches!(
            invalid,
            OrchestrationError::CompletionRequiresVerifiedEvidence
        ));

        shell
            .transition_task(transition(TaskState::Ready, 1, "event-3", "ready-request"))
            .unwrap();
        shell
            .transition_task(transition(
                TaskState::Blocked,
                2,
                "event-4",
                "blocked-request",
            ))
            .unwrap();
        shell
            .transition_task(transition(TaskState::Ready, 3, "event-5", "resume-request"))
            .unwrap();
        shell
            .transition_task(transition(
                TaskState::Blocked,
                4,
                "event-6",
                "blocked-request-2",
            ))
            .unwrap();
        let unverified_from_blocked = shell
            .transition_task(transition(
                TaskState::Completed,
                5,
                "event-7",
                "complete-request-2",
            ))
            .unwrap_err();
        assert!(matches!(
            unverified_from_blocked,
            OrchestrationError::CompletionRequiresVerifiedEvidence
        ));

        let terminal = shell
            .transition_task(transition(
                TaskState::Created,
                5,
                "event-8",
                "invalid-request",
            ))
            .unwrap_err();
        assert!(matches!(
            terminal,
            OrchestrationError::InvalidTransition { .. }
        ));
    }

    #[test]
    fn lifecycle_append_retry_is_idempotent_and_stale_writer_is_fenced() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ledger.sqlite");
        let first = ApplicationShell::open(&path).unwrap();
        let second = ApplicationShell::open(&path).unwrap();
        first.create_task(intent("create-request")).unwrap();
        let ready = transition(TaskState::Ready, 1, "event-2", "ready-request");
        let committed = first.transition_task(ready.clone()).unwrap();
        assert_eq!(second.transition_task(ready).unwrap(), committed);

        let stale = second
            .transition_task(transition(TaskState::Ready, 1, "event-3", "stale-request"))
            .unwrap_err();
        assert!(matches!(
            stale,
            OrchestrationError::Ledger(autocoder_ledger::LedgerError::RevisionConflict {
                expected: 1,
                actual: 2
            })
        ));
    }

    fn ready(shell: &ApplicationShell) {
        shell.create_task(intent("create-request")).unwrap();
        shell
            .transition_task(transition(TaskState::Ready, 1, "event-2", "ready-request"))
            .unwrap();
    }

    #[test]
    fn verified_evidence_is_durable_before_completion_and_replays_after_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ledger.sqlite");
        let shell = ApplicationShell::open(&path).unwrap();
        ready(&shell);
        let evidence = shell
            .record_verification(verification(VerificationOutcome::Verified, 2))
            .unwrap();
        assert!(matches!(
            evidence.payload,
            TaskEventPayload::SemanticVerificationRecorded { .. }
        ));
        assert_eq!(
            shell.task(&TaskId::parse("task-1").unwrap()).unwrap().state,
            TaskState::Ready
        );
        shell.complete_task(completion(3)).unwrap();
        drop(shell);

        let reopened = ApplicationShell::open(&path).unwrap();
        let projection = reopened.task(&TaskId::parse("task-1").unwrap()).unwrap();
        assert_eq!(projection.state, TaskState::Completed);
        assert_eq!(projection.stream_revision, 4);
        assert_eq!(
            projection.completion_evidence_id,
            Some(EvidenceId::parse("evidence-1").unwrap())
        );
    }

    #[test]
    fn absent_or_failed_evidence_cannot_complete() {
        let shell = ApplicationShell::open(":memory:").unwrap();
        ready(&shell);
        assert!(matches!(
            shell.complete_task(completion(2)),
            Err(OrchestrationError::EvidenceNotFound(_))
        ));
        shell
            .record_verification(verification(VerificationOutcome::Failed, 2))
            .unwrap();
        assert!(matches!(
            shell.complete_task(completion(3)),
            Err(OrchestrationError::EvidenceFailed(_))
        ));
        assert_eq!(
            shell.task(&TaskId::parse("task-1").unwrap()).unwrap().state,
            TaskState::Ready
        );
    }

    #[test]
    fn evidence_for_another_task_or_input_basis_is_inapplicable_and_stale() {
        for invalid_basis in [
            basis("another-task", "event-1", "workspace-snapshot-1"),
            basis("task-1", "event-1", "older-workspace-snapshot"),
        ] {
            let shell = ApplicationShell::open(":memory:").unwrap();
            ready(&shell);
            let mut record = verification(VerificationOutcome::Verified, 2);
            record.evidence.basis = invalid_basis;
            shell.record_verification(record).unwrap();
            assert!(matches!(
                shell.complete_task(completion(3)),
                Err(OrchestrationError::EvidenceBasisMismatch)
            ));
        }
    }

    #[test]
    fn exact_verification_and_completion_retries_do_not_duplicate_and_stale_writer_is_fenced() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ledger.sqlite");
        let first = ApplicationShell::open(&path).unwrap();
        let second = ApplicationShell::open(&path).unwrap();
        ready(&first);
        let record = verification(VerificationOutcome::Verified, 2);
        assert_eq!(
            first.record_verification(record.clone()).unwrap(),
            second.record_verification(record).unwrap()
        );
        let complete = completion(3);
        assert_eq!(
            first.complete_task(complete.clone()).unwrap(),
            second.complete_task(complete).unwrap()
        );
        let events = SqliteLedger::open(&path)
            .unwrap()
            .events(&TaskId::parse("task-1").unwrap())
            .unwrap();
        assert_eq!(events.len(), 4);

        let mut stale = verification(VerificationOutcome::Verified, 2);
        stale.event_id = EventId::parse("stale-verification").unwrap();
        stale.idempotency_key = IdempotencyKey::parse("stale-request").unwrap();
        assert!(matches!(
            second.record_verification(stale),
            Err(OrchestrationError::Ledger(
                autocoder_ledger::LedgerError::RevisionConflict {
                    expected: 2,
                    actual: 4
                }
            ))
        ));
    }

    #[test]
    fn incompatible_evidence_version_is_explicitly_rejected() {
        let shell = ApplicationShell::open(":memory:").unwrap();
        ready(&shell);
        let mut record = verification(VerificationOutcome::Verified, 2);
        record.evidence.schema_version = CONTRACT_VERSION + 1;
        let error = shell.record_verification(record).unwrap_err();
        assert!(error.to_string().contains("unsupported contract version"));
    }

    #[test]
    fn conflicting_evidence_identity_is_rejected_without_corrupting_history() {
        let shell = ApplicationShell::open(":memory:").unwrap();
        ready(&shell);
        shell
            .record_verification(verification(VerificationOutcome::Verified, 2))
            .unwrap();
        let mut conflict = verification(VerificationOutcome::Failed, 3);
        conflict.event_id = EventId::parse("conflicting-evidence-event").unwrap();
        conflict.idempotency_key = IdempotencyKey::parse("conflicting-evidence-request").unwrap();
        assert!(matches!(
            shell.record_verification(conflict),
            Err(OrchestrationError::EvidenceIdentityConflict(_))
        ));
        assert_eq!(
            shell
                .task(&TaskId::parse("task-1").unwrap())
                .unwrap()
                .stream_revision,
            3
        );
    }

    #[test]
    fn replay_explicitly_rejects_durable_incompatible_evidence_history() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ledger.sqlite");
        let shell = ApplicationShell::open(&path).unwrap();
        ready(&shell);
        drop(shell);
        let ledger = SqliteLedger::open(&path).unwrap();
        let mut evidence = verification(VerificationOutcome::Verified, 2).evidence;
        evidence.schema_version = CONTRACT_VERSION + 1;
        ledger
            .append(
                2,
                LedgerEvent {
                    schema_version: CONTRACT_VERSION,
                    task_id: TaskId::parse("task-1").unwrap(),
                    event_id: EventId::parse("incompatible-evidence-event").unwrap(),
                    stream_revision: 3,
                    idempotency_key: IdempotencyKey::parse("incompatible-evidence-request")
                        .unwrap(),
                    payload: TaskEventPayload::SemanticVerificationRecorded { evidence },
                },
            )
            .unwrap();
        let reopened = ApplicationShell::open(&path).unwrap();
        let error = reopened
            .task(&TaskId::parse("task-1").unwrap())
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("unsupported verification evidence/basis version"));
    }
}
