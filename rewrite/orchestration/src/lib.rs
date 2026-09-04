use autocoder_contracts::{
    CreateTaskIntent, LedgerEvent, TaskEventPayload, TaskId, TaskProjection, TaskState,
    TransitionTaskIntent, CONTRACT_VERSION,
};
use autocoder_ledger::{ExecutionLedger, LedgerError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OrchestrationError {
    #[error(transparent)]
    Contract(#[from] autocoder_contracts::ContractError),
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error("task creation requires expected revision 0, got {0}")]
    InvalidCreateRevision(u64),
    #[error("task {0} does not exist")]
    TaskNotFound(TaskId),
    #[error("incompatible task history: {0}")]
    IncompatibleHistory(String),
    #[error("invalid task transition from {from:?} to {to:?}")]
    InvalidTransition { from: TaskState, to: TaskState },
    #[error("task completion requires durable verified semantic-completion evidence")]
    CompletionRequiresVerifiedEvidence,
}

pub struct OrchestrationCore<L> {
    ledger: L,
}

impl<L: ExecutionLedger> OrchestrationCore<L> {
    pub fn new(ledger: L) -> Self {
        Self { ledger }
    }

    pub fn create_task(&self, intent: CreateTaskIntent) -> Result<LedgerEvent, OrchestrationError> {
        intent.validate()?;
        if intent.expected_revision != 0 {
            return Err(OrchestrationError::InvalidCreateRevision(
                intent.expected_revision,
            ));
        }
        let event = LedgerEvent {
            schema_version: CONTRACT_VERSION,
            task_id: intent.task_id.clone(),
            event_id: intent.event_id,
            stream_revision: intent.expected_revision + 1,
            idempotency_key: intent.idempotency_key.clone(),
            payload: TaskEventPayload::TaskCreated {
                workspace_id: intent.workspace_id,
                intent: intent.intent,
            },
        };
        Ok(self.ledger.append(intent.expected_revision, event)?)
    }

    /// Rebuilds the read model exclusively from the durable event stream.
    pub fn task(&self, task_id: &TaskId) -> Result<TaskProjection, OrchestrationError> {
        let events = self.ledger.events(task_id)?;
        project(task_id, &events)
    }

    pub fn transition_task(
        &self,
        intent: TransitionTaskIntent,
    ) -> Result<LedgerEvent, OrchestrationError> {
        intent.validate()?;
        let events = self.ledger.events(&intent.task_id)?;
        let actual = events.len() as u64;
        if intent.expected_revision > actual {
            return Err(LedgerError::RevisionConflict {
                expected: intent.expected_revision,
                actual,
            }
            .into());
        }
        // Validate the history at the caller's expected revision. This permits an
        // exact append retry to reach the Ledger while stale conflicting work is
        // still fenced by its stable identity or expected stream revision.
        let current = project(
            &intent.task_id,
            &events[..intent.expected_revision as usize],
        )?;
        let payload = match (current.state, intent.target_state) {
            // TaskCompleted remains part of the replay contract, but this generic
            // lifecycle command cannot produce it. A later orchestration-owned
            // completion path must first append/validate durable verification.
            (_, TaskState::Completed) => {
                return Err(OrchestrationError::CompletionRequiresVerifiedEvidence)
            }
            (TaskState::Created, TaskState::Ready) | (TaskState::Blocked, TaskState::Ready) => {
                TaskEventPayload::TaskReady
            }
            (TaskState::Ready, TaskState::Blocked) => TaskEventPayload::TaskBlocked,
            (from, to) => return Err(OrchestrationError::InvalidTransition { from, to }),
        };
        let event = LedgerEvent {
            schema_version: CONTRACT_VERSION,
            task_id: intent.task_id,
            event_id: intent.event_id,
            stream_revision: intent.expected_revision.checked_add(1).ok_or_else(|| {
                OrchestrationError::IncompatibleHistory("stream revision overflow".into())
            })?,
            idempotency_key: intent.idempotency_key,
            payload,
        };
        Ok(self.ledger.append(intent.expected_revision, event)?)
    }
}

fn project(task_id: &TaskId, events: &[LedgerEvent]) -> Result<TaskProjection, OrchestrationError> {
    let first = events
        .first()
        .ok_or_else(|| OrchestrationError::TaskNotFound(task_id.clone()))?;
    let (workspace_id, intent) = match &first.payload {
        TaskEventPayload::TaskCreated {
            workspace_id,
            intent,
        } => (workspace_id.clone(), intent.clone()),
        _ => {
            return Err(OrchestrationError::IncompatibleHistory(
                "first event is not task_created".into(),
            ))
        }
    };
    let mut state = TaskState::Created;
    for (index, event) in events.iter().enumerate() {
        let revision = (index as u64) + 1;
        if event.schema_version != CONTRACT_VERSION {
            return Err(OrchestrationError::IncompatibleHistory(format!(
                "unsupported event schema version {} at revision {revision}",
                event.schema_version
            )));
        }
        if &event.task_id != task_id || event.stream_revision != revision {
            return Err(OrchestrationError::IncompatibleHistory(format!(
                "invalid envelope at revision {revision}"
            )));
        }
        if index == 0 {
            continue;
        }
        let next = match &event.payload {
            TaskEventPayload::TaskReady => TaskState::Ready,
            TaskEventPayload::TaskBlocked => TaskState::Blocked,
            TaskEventPayload::TaskCompleted => {
                return Err(OrchestrationError::IncompatibleHistory(
                    "task_completed has no durable semantic-verification evidence".into(),
                ))
            }
            TaskEventPayload::TaskCreated { .. } => {
                return Err(OrchestrationError::IncompatibleHistory(
                    "task_created occurs more than once".into(),
                ))
            }
        };
        let valid = matches!(
            (state, next),
            (TaskState::Created, TaskState::Ready)
                | (TaskState::Blocked, TaskState::Ready)
                | (TaskState::Ready, TaskState::Blocked)
                | (TaskState::Ready, TaskState::Completed)
                | (TaskState::Blocked, TaskState::Completed)
        );
        if !valid {
            return Err(OrchestrationError::IncompatibleHistory(format!(
                "invalid transition from {state:?} to {next:?} at revision {revision}"
            )));
        }
        state = next;
    }
    Ok(TaskProjection {
        schema_version: CONTRACT_VERSION,
        task_id: task_id.clone(),
        workspace_id,
        intent,
        state,
        stream_revision: events.len() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use autocoder_contracts::{EventId, IdempotencyKey, WorkspaceId};

    struct ReplayLedger(Vec<LedgerEvent>);

    impl ExecutionLedger for ReplayLedger {
        fn append(&self, _: u64, _: LedgerEvent) -> Result<LedgerEvent, LedgerError> {
            unreachable!()
        }

        fn events(&self, _: &TaskId) -> Result<Vec<LedgerEvent>, LedgerError> {
            Ok(self.0.clone())
        }
    }

    fn event(revision: u64, payload: TaskEventPayload) -> LedgerEvent {
        LedgerEvent {
            schema_version: CONTRACT_VERSION,
            task_id: TaskId::parse("task-1").unwrap(),
            event_id: EventId::parse(format!("event-{revision}")).unwrap(),
            stream_revision: revision,
            idempotency_key: IdempotencyKey::parse(format!("request-{revision}")).unwrap(),
            payload,
        }
    }

    fn created() -> LedgerEvent {
        event(
            1,
            TaskEventPayload::TaskCreated {
                workspace_id: WorkspaceId::parse("workspace-1").unwrap(),
                intent: "intent".into(),
            },
        )
    }

    #[test]
    fn replay_rejects_invalid_lifecycle_history() {
        let core = OrchestrationCore::new(ReplayLedger(vec![
            created(),
            event(2, TaskEventPayload::TaskCompleted),
        ]));
        assert!(matches!(
            core.task(&TaskId::parse("task-1").unwrap()),
            Err(OrchestrationError::IncompatibleHistory(_))
        ));
    }

    #[test]
    fn replay_rejects_completed_event_without_durable_verification_evidence() {
        let core = OrchestrationCore::new(ReplayLedger(vec![
            created(),
            event(2, TaskEventPayload::TaskReady),
            event(3, TaskEventPayload::TaskCompleted),
        ]));
        let error = core.task(&TaskId::parse("task-1").unwrap()).unwrap_err();
        assert!(error
            .to_string()
            .contains("no durable semantic-verification evidence"));
    }

    #[test]
    fn replay_rejects_incompatible_event_version() {
        let mut incompatible = created();
        incompatible.schema_version = CONTRACT_VERSION + 1;
        let core = OrchestrationCore::new(ReplayLedger(vec![incompatible]));
        let error = core.task(&TaskId::parse("task-1").unwrap()).unwrap_err();
        assert!(error
            .to_string()
            .contains("unsupported event schema version"));
    }

    #[test]
    fn replay_rejects_missing_and_noncontiguous_history() {
        let missing = OrchestrationCore::new(ReplayLedger(vec![]));
        assert!(matches!(
            missing.task(&TaskId::parse("task-1").unwrap()),
            Err(OrchestrationError::TaskNotFound(_))
        ));

        let core = OrchestrationCore::new(ReplayLedger(vec![
            created(),
            event(3, TaskEventPayload::TaskReady),
        ]));
        assert!(matches!(
            core.task(&TaskId::parse("task-1").unwrap()),
            Err(OrchestrationError::IncompatibleHistory(_))
        ));
    }
}
