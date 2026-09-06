use autocoder_contracts::{
    CompleteTaskIntent, CreateTaskIntent, DefineStepIntent, LedgerEvent, ReconcileAttemptIntent,
    RecordAttemptObservationIntent, RecordAttemptOutcomeIntent, RecordVerificationIntent,
    StartAttemptIntent, TaskId, TaskProjection, TransitionTaskIntent,
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
    pub fn define_step(&self, intent: DefineStepIntent) -> Result<LedgerEvent, OrchestrationError> {
        self.core.define_step(intent)
    }
    pub fn start_attempt(
        &self,
        intent: StartAttemptIntent,
    ) -> Result<LedgerEvent, OrchestrationError> {
        self.core.start_attempt(intent)
    }
    pub fn record_attempt_outcome(
        &self,
        intent: RecordAttemptOutcomeIntent,
    ) -> Result<LedgerEvent, OrchestrationError> {
        self.core.record_attempt_outcome(intent)
    }
    pub fn record_attempt_observation(
        &self,
        intent: RecordAttemptObservationIntent,
    ) -> Result<LedgerEvent, OrchestrationError> {
        self.core.record_attempt_observation(intent)
    }
    pub fn reconcile_attempt(
        &self,
        intent: ReconcileAttemptIntent,
    ) -> Result<LedgerEvent, OrchestrationError> {
        self.core.reconcile_attempt(intent)
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

    fn define_step(revision: u64) -> DefineStepIntent {
        DefineStepIntent {
            contract_version: CONTRACT_VERSION,
            task_id: TaskId::parse("task-1").unwrap(),
            step_id: StepId::parse("step-1").unwrap(),
            description: "future side effect".into(),
            event_id: EventId::parse("define-step").unwrap(),
            idempotency_key: IdempotencyKey::parse("define-step-request").unwrap(),
            expected_revision: revision,
        }
    }

    fn start_attempt(id: &str, revision: u64) -> StartAttemptIntent {
        StartAttemptIntent {
            contract_version: CONTRACT_VERSION,
            task_id: TaskId::parse("task-1").unwrap(),
            step_id: StepId::parse("step-1").unwrap(),
            attempt_id: AttemptId::parse(id).unwrap(),
            event_id: EventId::parse(format!("start-{id}")).unwrap(),
            idempotency_key: IdempotencyKey::parse(format!("start-{id}-request")).unwrap(),
            expected_revision: revision,
        }
    }

    fn attempt_outcome(
        id: &str,
        generation: u64,
        outcome: AttemptOutcome,
        revision: u64,
    ) -> RecordAttemptOutcomeIntent {
        RecordAttemptOutcomeIntent {
            contract_version: CONTRACT_VERSION,
            task_id: TaskId::parse("task-1").unwrap(),
            step_id: StepId::parse("step-1").unwrap(),
            attempt_id: AttemptId::parse(id).unwrap(),
            authority_generation: generation,
            outcome,
            event_id: EventId::parse(format!("outcome-{id}")).unwrap(),
            idempotency_key: IdempotencyKey::parse(format!("outcome-{id}-request")).unwrap(),
            expected_revision: revision,
        }
    }

    fn scope(id: &str, generation: u64) -> AttemptScope {
        AttemptScope {
            task_id: TaskId::parse("task-1").unwrap(),
            step_id: StepId::parse("step-1").unwrap(),
            attempt_id: AttemptId::parse(id).unwrap(),
            authority_generation: generation,
        }
    }

    fn observation(id: &str, generation: u64, revision: u64) -> RecordAttemptObservationIntent {
        observation_named(id, id, generation, revision)
    }

    fn observation_named(
        name: &str,
        attempt_id: &str,
        generation: u64,
        revision: u64,
    ) -> RecordAttemptObservationIntent {
        RecordAttemptObservationIntent {
            contract_version: CONTRACT_VERSION,
            task_id: TaskId::parse("task-1").unwrap(),
            observation: AttemptObservation {
                schema_version: CONTRACT_VERSION,
                observation_id: ObservationId::parse(format!("observation-{name}")).unwrap(),
                scope: scope(attempt_id, generation),
                kind: AttemptObservationKind::OutcomeStillUnknown {
                    reason: "worker disconnected after dispatch".into(),
                },
                provenance: ObservationProvenance {
                    source: "autocoder.recovery-probe".into(),
                    source_version: "1.0.0".into(),
                    method: "durable-receipt-inspection".into(),
                    detail: "dispatch receipt exists; terminal receipt absent".into(),
                },
            },
            event_id: EventId::parse(format!("observe-{name}")).unwrap(),
            idempotency_key: IdempotencyKey::parse(format!("observe-{name}-request")).unwrap(),
            expected_revision: revision,
        }
    }

    fn reconciliation(
        id: &str,
        generation: u64,
        conclusion: ReconciliationConclusion,
        revision: u64,
    ) -> ReconcileAttemptIntent {
        reconciliation_named(id, id, id, generation, conclusion, revision)
    }

    fn reconciliation_named(
        name: &str,
        observation_name: &str,
        attempt_id: &str,
        generation: u64,
        conclusion: ReconciliationConclusion,
        revision: u64,
    ) -> ReconcileAttemptIntent {
        ReconcileAttemptIntent {
            contract_version: CONTRACT_VERSION,
            task_id: TaskId::parse("task-1").unwrap(),
            reconciliation: AttemptReconciliation {
                schema_version: CONTRACT_VERSION,
                reconciliation_id: ReconciliationId::parse(format!("reconciliation-{name}"))
                    .unwrap(),
                scope: scope(attempt_id, generation),
                observation_ids: vec![ObservationId::parse(format!(
                    "observation-{observation_name}"
                ))
                .unwrap()],
                conclusion,
                rationale: "orchestration policy evaluated the durable observation".into(),
            },
            event_id: EventId::parse(format!("reconcile-{name}")).unwrap(),
            idempotency_key: IdempotencyKey::parse(format!("reconcile-{name}-request")).unwrap(),
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
    fn main_create_history_retries_and_completes_through_durable_replay() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ledger.sqlite");
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection.execute_batch("CREATE TABLE execution_events (task_id TEXT NOT NULL, revision INTEGER NOT NULL, event_id TEXT NOT NULL UNIQUE, idempotency_key TEXT NOT NULL UNIQUE, body TEXT NOT NULL, PRIMARY KEY(task_id, revision));").unwrap();
        let main_body = r#"{"schema_version":1,"task_id":"task-1","event_id":"event-1","stream_revision":1,"idempotency_key":"ui-request-1","payload":{"type":"task_created","workspace_id":"workspace-1","intent":"Create the first task"}}"#;
        connection.execute(
            "INSERT INTO execution_events(task_id, revision, event_id, idempotency_key, body) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params!["task-1", 1, "event-1", "ui-request-1", main_body],
        ).unwrap();
        drop(connection);

        let shell = ApplicationShell::open(&path).unwrap();
        let legacy_input = legacy_create_v1_input_revision(&EventId::parse("event-1").unwrap());
        let mut retried_create = intent("ui-request-1");
        retried_create.input_revision = legacy_input.clone();
        assert_eq!(
            shell.create_task(retried_create).unwrap().stream_revision,
            1
        );
        shell
            .transition_task(transition(
                TaskState::Ready,
                1,
                "ready-event",
                "ready-request",
            ))
            .unwrap();

        let legacy_basis = basis("task-1", "event-1", legacy_input.as_str());
        let mut record = verification(VerificationOutcome::Verified, 2);
        record.evidence.basis = legacy_basis.clone();
        shell.record_verification(record).unwrap();
        let mut complete = completion(3);
        complete.basis = legacy_basis.clone();
        shell.complete_task(complete).unwrap();
        drop(shell);

        let reopened = ApplicationShell::open(&path).unwrap();
        let projection = reopened.task(&TaskId::parse("task-1").unwrap()).unwrap();
        assert_eq!(projection.input_basis, legacy_basis);
        assert_eq!(projection.state, TaskState::Completed);
        assert_eq!(projection.stream_revision, 4);
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

    #[test]
    fn started_attempt_reopens_with_explicitly_unknown_outcome_and_preserved_authority() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ledger.sqlite");
        let shell = ApplicationShell::open(&path).unwrap();
        shell.create_task(intent("create-request")).unwrap();
        shell.define_step(define_step(1)).unwrap();
        shell.start_attempt(start_attempt("attempt-1", 2)).unwrap();
        drop(shell);

        let projection = ApplicationShell::open(&path)
            .unwrap()
            .task(&TaskId::parse("task-1").unwrap())
            .unwrap();
        let step = &projection.steps[0];
        assert_eq!(step.authority_generation, 1);
        assert_eq!(
            step.current_attempt_id,
            Some(AttemptId::parse("attempt-1").unwrap())
        );
        assert_eq!(step.attempts[0].outcome, None);
        assert_eq!(step.attempts[0].authority, AttemptAuthority::Current);
    }

    #[test]
    fn new_attempt_supersedes_old_authority_and_late_result_is_only_history() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ledger.sqlite");
        let shell = ApplicationShell::open(&path).unwrap();
        shell.create_task(intent("create-request")).unwrap();
        shell.define_step(define_step(1)).unwrap();
        shell.start_attempt(start_attempt("attempt-1", 2)).unwrap();
        shell
            .record_attempt_observation(observation("attempt-1", 1, 3))
            .unwrap();
        shell
            .reconcile_attempt(reconciliation(
                "attempt-1",
                1,
                ReconciliationConclusion::RetryAuthorized,
                4,
            ))
            .unwrap();
        shell.start_attempt(start_attempt("attempt-2", 5)).unwrap();
        shell
            .record_attempt_outcome(attempt_outcome(
                "attempt-1",
                1,
                AttemptOutcome::Succeeded,
                6,
            ))
            .unwrap();

        drop(shell);
        let projection = ApplicationShell::open(&path)
            .unwrap()
            .task(&TaskId::parse("task-1").unwrap())
            .unwrap();
        let step = &projection.steps[0];
        assert_eq!(step.authority_generation, 2);
        assert_eq!(
            step.current_attempt_id,
            Some(AttemptId::parse("attempt-2").unwrap())
        );
        assert_eq!(step.attempts[0].outcome, Some(AttemptOutcome::Succeeded));
        assert_eq!(step.attempts[0].authority, AttemptAuthority::Superseded);
        assert_eq!(step.attempts[1].outcome, None);
        assert_eq!(projection.state, TaskState::Created);
    }

    #[test]
    fn attempt_append_exact_retry_is_idempotent_and_stale_writer_is_fenced() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ledger.sqlite");
        let first = ApplicationShell::open(&path).unwrap();
        let second = ApplicationShell::open(&path).unwrap();
        first.create_task(intent("create-request")).unwrap();
        first.define_step(define_step(1)).unwrap();
        let start = start_attempt("attempt-1", 2);
        assert_eq!(
            first.start_attempt(start.clone()).unwrap(),
            second.start_attempt(start).unwrap()
        );
        assert_eq!(
            SqliteLedger::open(&path)
                .unwrap()
                .events(&TaskId::parse("task-1").unwrap())
                .unwrap()
                .len(),
            3
        );
        let stale = start_attempt("attempt-2", 2);
        assert!(matches!(
            second.start_attempt(stale),
            Err(OrchestrationError::Ledger(
                autocoder_ledger::LedgerError::RevisionConflict {
                    expected: 2,
                    actual: 3
                }
            ))
        ));
    }

    #[test]
    fn technical_attempt_success_does_not_complete_task() {
        let shell = ApplicationShell::open(":memory:").unwrap();
        ready(&shell);
        shell.define_step(define_step(2)).unwrap();
        shell.start_attempt(start_attempt("attempt-1", 3)).unwrap();
        shell
            .record_attempt_outcome(attempt_outcome(
                "attempt-1",
                1,
                AttemptOutcome::Succeeded,
                4,
            ))
            .unwrap();
        let projection = shell.task(&TaskId::parse("task-1").unwrap()).unwrap();
        assert_eq!(projection.state, TaskState::Ready);
        assert_eq!(projection.completion_evidence_id, None);
    }

    fn complete_with_started_attempt(shell: &ApplicationShell) {
        ready(shell);
        shell.define_step(define_step(2)).unwrap();
        shell.start_attempt(start_attempt("attempt-1", 3)).unwrap();
        shell
            .record_verification(verification(VerificationOutcome::Verified, 4))
            .unwrap();
        shell.complete_task(completion(5)).unwrap();
    }

    #[test]
    fn completed_task_rejects_new_steps_and_attempts_but_accepts_late_history() {
        let shell = ApplicationShell::open(":memory:").unwrap();
        complete_with_started_attempt(&shell);

        let mut new_step = define_step(6);
        new_step.step_id = StepId::parse("step-after-completion").unwrap();
        new_step.event_id = EventId::parse("define-after-completion").unwrap();
        new_step.idempotency_key =
            IdempotencyKey::parse("define-after-completion-request").unwrap();
        assert!(matches!(
            shell.define_step(new_step),
            Err(OrchestrationError::TaskClosed(_))
        ));
        assert!(matches!(
            shell.start_attempt(start_attempt("attempt-after-completion", 6)),
            Err(OrchestrationError::TaskClosed(_))
        ));

        shell
            .record_attempt_outcome(attempt_outcome(
                "attempt-1",
                1,
                AttemptOutcome::Succeeded,
                6,
            ))
            .unwrap();
        let projection = shell.task(&TaskId::parse("task-1").unwrap()).unwrap();
        assert_eq!(projection.state, TaskState::Completed);
        assert_eq!(
            projection.steps[0].attempts[0].outcome,
            Some(AttemptOutcome::Succeeded)
        );
        assert_eq!(
            projection.steps[0].attempts[0].authority,
            AttemptAuthority::Revoked
        );
        assert!(!projection
            .steps
            .iter()
            .flat_map(|step| &step.attempts)
            .any(|attempt| attempt.authority == AttemptAuthority::Current));
    }

    #[test]
    fn replay_rejects_step_or_attempt_started_after_completion() {
        for payload in [
            TaskEventPayload::StepDefined {
                step_id: StepId::parse("forged-step").unwrap(),
                description: "forged post-completion work".into(),
            },
            TaskEventPayload::AttemptStarted {
                step_id: StepId::parse("step-1").unwrap(),
                attempt_id: AttemptId::parse("forged-attempt").unwrap(),
                generation: 2,
            },
        ] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("ledger.sqlite");
            let shell = ApplicationShell::open(&path).unwrap();
            complete_with_started_attempt(&shell);
            drop(shell);
            SqliteLedger::open(&path)
                .unwrap()
                .append(
                    6,
                    LedgerEvent {
                        schema_version: CONTRACT_VERSION,
                        task_id: TaskId::parse("task-1").unwrap(),
                        event_id: EventId::parse("forged-event").unwrap(),
                        stream_revision: 7,
                        idempotency_key: IdempotencyKey::parse("forged-request").unwrap(),
                        payload,
                    },
                )
                .unwrap();
            let error = ApplicationShell::open(&path)
                .unwrap()
                .task(&TaskId::parse("task-1").unwrap())
                .unwrap_err();
            assert!(error.to_string().contains("after task completion"));
        }
    }

    #[test]
    fn confirmed_interruption_and_unknown_outcome_remain_distinct_after_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ledger.sqlite");
        let shell = ApplicationShell::open(&path).unwrap();
        shell.create_task(intent("create-request")).unwrap();
        shell.define_step(define_step(1)).unwrap();
        shell.start_attempt(start_attempt("attempt-1", 2)).unwrap();
        shell
            .record_attempt_outcome(attempt_outcome(
                "attempt-1",
                1,
                AttemptOutcome::Interrupted,
                3,
            ))
            .unwrap();
        shell.start_attempt(start_attempt("attempt-2", 4)).unwrap();
        drop(shell);

        let projection = ApplicationShell::open(&path)
            .unwrap()
            .task(&TaskId::parse("task-1").unwrap())
            .unwrap();
        let attempts = &projection.steps[0].attempts;
        assert_eq!(attempts[0].outcome, Some(AttemptOutcome::Interrupted));
        assert_eq!(attempts[0].authority, AttemptAuthority::Superseded);
        assert_eq!(attempts[1].outcome, None);
        assert_eq!(attempts[1].authority, AttemptAuthority::Current);
    }

    #[test]
    fn unknown_retry_requires_a_separate_durable_authorizing_decision() {
        let shell = ApplicationShell::open(":memory:").unwrap();
        shell.create_task(intent("create-request")).unwrap();
        shell.define_step(define_step(1)).unwrap();
        shell.start_attempt(start_attempt("attempt-1", 2)).unwrap();
        assert!(matches!(
            shell.start_attempt(start_attempt("attempt-2", 3)),
            Err(OrchestrationError::UnknownAttemptRequiresReconciliation)
        ));

        shell
            .record_attempt_observation(observation("attempt-1", 1, 3))
            .unwrap();
        assert!(matches!(
            shell.start_attempt(start_attempt("attempt-2", 4)),
            Err(OrchestrationError::UnknownAttemptRequiresReconciliation)
        ));
        shell
            .reconcile_attempt(reconciliation(
                "attempt-1",
                1,
                ReconciliationConclusion::Unresolved {
                    reason: "insufficient durable evidence".into(),
                },
                4,
            ))
            .unwrap();
        assert!(matches!(
            shell.start_attempt(start_attempt("attempt-2", 5)),
            Err(OrchestrationError::UnknownAttemptRequiresReconciliation)
        ));
        assert_eq!(
            shell.task(&TaskId::parse("task-1").unwrap()).unwrap().state,
            TaskState::Created
        );
    }

    #[test]
    fn pre_reconciliation_v1_unknown_supersede_history_remains_replayable() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ledger.sqlite");
        let shell = ApplicationShell::open(&path).unwrap();
        shell.create_task(intent("create-request")).unwrap();
        shell.define_step(define_step(1)).unwrap();
        shell.start_attempt(start_attempt("attempt-1", 2)).unwrap();
        drop(shell);

        SqliteLedger::open(&path)
            .unwrap()
            .append(
                3,
                LedgerEvent {
                    schema_version: CONTRACT_VERSION,
                    task_id: TaskId::parse("task-1").unwrap(),
                    event_id: EventId::parse("legacy-start-attempt-2").unwrap(),
                    stream_revision: 4,
                    idempotency_key: IdempotencyKey::parse("legacy-start-attempt-2-request")
                        .unwrap(),
                    payload: TaskEventPayload::AttemptStarted {
                        step_id: StepId::parse("step-1").unwrap(),
                        attempt_id: AttemptId::parse("attempt-2").unwrap(),
                        generation: 2,
                    },
                },
            )
            .unwrap();

        let projection = ApplicationShell::open(&path)
            .unwrap()
            .task(&TaskId::parse("task-1").unwrap())
            .unwrap();
        let step = &projection.steps[0];
        assert_eq!(step.authority_generation, 2);
        assert_eq!(step.attempts[0].outcome, None);
        assert_eq!(step.attempts[0].authority, AttemptAuthority::Superseded);
        assert_eq!(step.attempts[1].outcome, None);
        assert_eq!(step.attempts[1].authority, AttemptAuthority::Current);
    }

    #[test]
    fn unresolved_can_advance_to_retry_only_after_new_durable_evidence() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ledger.sqlite");
        let shell = ApplicationShell::open(&path).unwrap();
        shell.create_task(intent("create-request")).unwrap();
        shell.define_step(define_step(1)).unwrap();
        shell.start_attempt(start_attempt("attempt-1", 2)).unwrap();
        shell
            .record_attempt_observation(observation_named("first", "attempt-1", 1, 3))
            .unwrap();
        shell
            .reconcile_attempt(reconciliation_named(
                "unresolved",
                "first",
                "attempt-1",
                1,
                ReconciliationConclusion::Unresolved {
                    reason: "terminal receipt remains absent".into(),
                },
                4,
            ))
            .unwrap();

        assert!(matches!(
            shell.reconcile_attempt(reconciliation_named(
                "same-evidence",
                "first",
                "attempt-1",
                1,
                ReconciliationConclusion::RetryAuthorized,
                5,
            )),
            Err(OrchestrationError::ReconciliationEvidenceNotAdvanced)
        ));
        shell
            .record_attempt_observation(observation_named("second", "attempt-1", 1, 5))
            .unwrap();
        let retry = reconciliation_named(
            "retry",
            "second",
            "attempt-1",
            1,
            ReconciliationConclusion::RetryAuthorized,
            6,
        );
        assert_eq!(
            shell.reconcile_attempt(retry.clone()).unwrap(),
            shell.reconcile_attempt(retry).unwrap()
        );
        drop(shell);

        let reopened = ApplicationShell::open(&path).unwrap();
        let projection = reopened.task(&TaskId::parse("task-1").unwrap()).unwrap();
        let attempt = &projection.steps[0].attempts[0];
        assert_eq!(attempt.reconciliations.len(), 2);
        assert!(matches!(
            attempt.reconciliations[0].conclusion,
            ReconciliationConclusion::Unresolved { .. }
        ));
        assert!(matches!(
            attempt.reconciliations[1].conclusion,
            ReconciliationConclusion::RetryAuthorized
        ));
        assert_eq!(
            attempt.effective_reconciliation_id,
            Some(ReconciliationId::parse("reconciliation-retry").unwrap())
        );
        reopened
            .start_attempt(start_attempt("attempt-2", 7))
            .unwrap();
        let retried = reopened.task(&TaskId::parse("task-1").unwrap()).unwrap();
        assert_eq!(retried.steps[0].authority_generation, 2);
        assert_eq!(retried.state, TaskState::Created);
    }

    #[test]
    fn confirmed_failure_can_later_receive_explicit_retry_authorization() {
        let shell = ApplicationShell::open(":memory:").unwrap();
        shell.create_task(intent("create-request")).unwrap();
        shell.define_step(define_step(1)).unwrap();
        shell.start_attempt(start_attempt("attempt-1", 2)).unwrap();
        shell
            .record_attempt_observation(observation_named("failed", "attempt-1", 1, 3))
            .unwrap();
        shell
            .reconcile_attempt(reconciliation_named(
                "failed",
                "failed",
                "attempt-1",
                1,
                ReconciliationConclusion::ConfirmedOutcome {
                    outcome: AttemptOutcome::Failed,
                },
                4,
            ))
            .unwrap();
        assert!(matches!(
            shell.start_attempt(start_attempt("attempt-2", 5)),
            Err(OrchestrationError::UnknownAttemptRequiresReconciliation)
        ));
        shell
            .record_attempt_observation(observation_named("retry", "attempt-1", 1, 5))
            .unwrap();
        shell
            .reconcile_attempt(reconciliation_named(
                "retry",
                "retry",
                "attempt-1",
                1,
                ReconciliationConclusion::RetryAuthorized,
                6,
            ))
            .unwrap();
        shell.start_attempt(start_attempt("attempt-2", 7)).unwrap();
        assert_eq!(
            shell.task(&TaskId::parse("task-1").unwrap()).unwrap().steps[0].attempts[0]
                .reconciliations
                .len(),
            2
        );
    }

    #[test]
    fn late_raw_terminal_outcome_overrides_unknownness_but_preserves_unresolved_history() {
        let shell = ApplicationShell::open(":memory:").unwrap();
        shell.create_task(intent("create-request")).unwrap();
        shell.define_step(define_step(1)).unwrap();
        shell.start_attempt(start_attempt("attempt-1", 2)).unwrap();
        shell
            .record_attempt_observation(observation("attempt-1", 1, 3))
            .unwrap();
        shell
            .reconcile_attempt(reconciliation(
                "attempt-1",
                1,
                ReconciliationConclusion::Unresolved {
                    reason: "no terminal fact yet".into(),
                },
                4,
            ))
            .unwrap();
        shell
            .record_attempt_outcome(attempt_outcome(
                "attempt-1",
                1,
                AttemptOutcome::Interrupted,
                5,
            ))
            .unwrap();
        shell.start_attempt(start_attempt("attempt-2", 6)).unwrap();

        let projection = shell.task(&TaskId::parse("task-1").unwrap()).unwrap();
        let first = &projection.steps[0].attempts[0];
        assert_eq!(first.outcome, Some(AttemptOutcome::Interrupted));
        assert_eq!(first.reconciliations.len(), 1);
        assert!(matches!(
            first.reconciliations[0].conclusion,
            ReconciliationConclusion::Unresolved { .. }
        ));
        assert_eq!(projection.steps[0].authority_generation, 2);
        assert_eq!(projection.state, TaskState::Created);
    }

    #[test]
    fn retry_reconciliation_is_durable_idempotent_and_advances_authority() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ledger.sqlite");
        let first = ApplicationShell::open(&path).unwrap();
        let second = ApplicationShell::open(&path).unwrap();
        first.create_task(intent("create-request")).unwrap();
        first.define_step(define_step(1)).unwrap();
        first.start_attempt(start_attempt("attempt-1", 2)).unwrap();
        let recorded_observation = observation("attempt-1", 1, 3);
        assert_eq!(
            first
                .record_attempt_observation(recorded_observation.clone())
                .unwrap(),
            second
                .record_attempt_observation(recorded_observation)
                .unwrap()
        );
        let decision = reconciliation("attempt-1", 1, ReconciliationConclusion::RetryAuthorized, 4);
        assert_eq!(
            first.reconcile_attempt(decision.clone()).unwrap(),
            second.reconcile_attempt(decision).unwrap()
        );
        drop(first);
        drop(second);

        let reopened = ApplicationShell::open(&path).unwrap();
        let before = reopened.task(&TaskId::parse("task-1").unwrap()).unwrap();
        assert!(matches!(
            before.steps[0].attempts[0]
                .reconciliations
                .last()
                .map(|item| &item.conclusion),
            Some(ReconciliationConclusion::RetryAuthorized)
        ));
        reopened
            .start_attempt(start_attempt("attempt-2", 5))
            .unwrap();
        let after = reopened.task(&TaskId::parse("task-1").unwrap()).unwrap();
        assert_eq!(after.steps[0].authority_generation, 2);
        assert_eq!(after.state, TaskState::Created);

        let mut stale = observation("attempt-2", 2, 5);
        stale.event_id = EventId::parse("stale-observation").unwrap();
        stale.idempotency_key = IdempotencyKey::parse("stale-observation-request").unwrap();
        assert!(matches!(
            reopened.record_attempt_observation(stale),
            Err(OrchestrationError::Ledger(
                autocoder_ledger::LedgerError::RevisionConflict {
                    expected: 5,
                    actual: 6
                }
            ))
        ));
    }

    #[test]
    fn confirmed_reconciliation_stays_distinct_and_late_contradiction_is_preserved() {
        let shell = ApplicationShell::open(":memory:").unwrap();
        shell.create_task(intent("create-request")).unwrap();
        shell.define_step(define_step(1)).unwrap();
        shell.start_attempt(start_attempt("attempt-1", 2)).unwrap();
        shell
            .record_attempt_observation(observation("attempt-1", 1, 3))
            .unwrap();
        shell
            .reconcile_attempt(reconciliation(
                "attempt-1",
                1,
                ReconciliationConclusion::ConfirmedOutcome {
                    outcome: AttemptOutcome::Succeeded,
                },
                4,
            ))
            .unwrap();
        let reconciled = shell.task(&TaskId::parse("task-1").unwrap()).unwrap();
        assert_eq!(reconciled.steps[0].attempts[0].outcome, None);
        assert!(matches!(
            reconciled.steps[0].attempts[0]
                .reconciliations
                .last()
                .map(|item| &item.conclusion),
            Some(ReconciliationConclusion::ConfirmedOutcome {
                outcome: AttemptOutcome::Succeeded
            })
        ));

        shell
            .record_attempt_outcome(attempt_outcome("attempt-1", 1, AttemptOutcome::Failed, 5))
            .unwrap();
        let late = shell.task(&TaskId::parse("task-1").unwrap()).unwrap();
        let attempt = &late.steps[0].attempts[0];
        assert_eq!(attempt.outcome, Some(AttemptOutcome::Failed));
        assert_eq!(attempt.contradictions.len(), 1);
        assert_eq!(attempt.observations.len(), 1);
        assert_eq!(attempt.reconciliations.len(), 1);
        assert_eq!(late.state, TaskState::Created);
        shell.start_attempt(start_attempt("attempt-2", 6)).unwrap();
        assert_eq!(
            shell.task(&TaskId::parse("task-1").unwrap()).unwrap().steps[0].authority_generation,
            2
        );
    }

    #[test]
    fn replay_rejects_incompatible_observation_and_reconciliation_versions() {
        for incompatible_observation in [true, false] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("ledger.sqlite");
            let durable = ApplicationShell::open(&path).unwrap();
            durable.create_task(intent("create-request")).unwrap();
            durable.define_step(define_step(1)).unwrap();
            durable
                .start_attempt(start_attempt("attempt-1", 2))
                .unwrap();
            drop(durable);
            let ledger = SqliteLedger::open(&path).unwrap();
            let payload = if incompatible_observation {
                let mut item = observation("attempt-1", 1, 3).observation;
                item.schema_version = CONTRACT_VERSION + 1;
                TaskEventPayload::AttemptObservationRecorded { observation: item }
            } else {
                let item = observation("attempt-1", 1, 3).observation;
                ledger
                    .append(
                        3,
                        LedgerEvent {
                            schema_version: CONTRACT_VERSION,
                            task_id: TaskId::parse("task-1").unwrap(),
                            event_id: EventId::parse("observation-event").unwrap(),
                            stream_revision: 4,
                            idempotency_key: IdempotencyKey::parse("observation-request").unwrap(),
                            payload: TaskEventPayload::AttemptObservationRecorded {
                                observation: item.clone(),
                            },
                        },
                    )
                    .unwrap();
                let mut decision =
                    reconciliation("attempt-1", 1, ReconciliationConclusion::RetryAuthorized, 4)
                        .reconciliation;
                decision.schema_version = CONTRACT_VERSION + 1;
                TaskEventPayload::AttemptReconciled {
                    reconciliation: decision,
                }
            };
            let revision = if incompatible_observation { 4 } else { 5 };
            ledger
                .append(
                    revision - 1,
                    LedgerEvent {
                        schema_version: CONTRACT_VERSION,
                        task_id: TaskId::parse("task-1").unwrap(),
                        event_id: EventId::parse("incompatible-event").unwrap(),
                        stream_revision: revision,
                        idempotency_key: IdempotencyKey::parse("incompatible-request").unwrap(),
                        payload,
                    },
                )
                .unwrap();
            let error = ApplicationShell::open(&path)
                .unwrap()
                .task(&TaskId::parse("task-1").unwrap())
                .unwrap_err();
            assert!(error.to_string().contains("unsupported attempt"));
        }
    }
}
