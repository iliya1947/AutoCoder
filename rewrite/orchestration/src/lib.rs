use autocoder_contracts::{
    CompleteTaskIntent, CreateTaskIntent, EvidenceId, LedgerEvent, RecordVerificationIntent,
    SemanticVerificationEvidence, TaskEventPayload, TaskId, TaskProjection, TaskState,
    TransitionTaskIntent, VerificationBasis, VerificationOutcome, CONTRACT_VERSION,
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
    #[error("task completion is owned by the verified completion path")]
    CompletionRequiresVerifiedEvidence,
    #[error("verification evidence {0} is absent")]
    EvidenceNotFound(EvidenceId),
    #[error("verification evidence {0} did not verify semantic completion")]
    EvidenceFailed(EvidenceId),
    #[error("verification evidence is not applicable to the task's current input basis")]
    EvidenceBasisMismatch,
    #[error("verification evidence identity {0} is already present in task history")]
    EvidenceIdentityConflict(EvidenceId),
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
            stream_revision: 1,
            idempotency_key: intent.idempotency_key,
            payload: TaskEventPayload::TaskCreated {
                workspace_id: intent.workspace_id,
                intent: intent.intent,
                input_revision: intent.input_revision,
            },
        };
        Ok(self.ledger.append(0, event)?)
    }

    /// Rebuilds the read model exclusively from durable history. Replay performs
    /// no verification and consults no clock, filesystem, provider, or network.
    pub fn task(&self, task_id: &TaskId) -> Result<TaskProjection, OrchestrationError> {
        project(task_id, &self.ledger.events(task_id)?)
    }

    pub fn transition_task(
        &self,
        intent: TransitionTaskIntent,
    ) -> Result<LedgerEvent, OrchestrationError> {
        intent.validate()?;
        let events = self.ledger.events(&intent.task_id)?;
        let current = project_at_expected(&intent.task_id, &events, intent.expected_revision)?;
        let payload = match (current.projection.state, intent.target_state) {
            (_, TaskState::Completed) => {
                return Err(OrchestrationError::CompletionRequiresVerifiedEvidence)
            }
            (TaskState::Created, TaskState::Ready) | (TaskState::Blocked, TaskState::Ready) => {
                TaskEventPayload::TaskReady
            }
            (TaskState::Ready, TaskState::Blocked) => TaskEventPayload::TaskBlocked,
            (from, to) => return Err(OrchestrationError::InvalidTransition { from, to }),
        };
        self.append(
            intent.task_id,
            intent.event_id,
            intent.idempotency_key,
            intent.expected_revision,
            payload,
        )
    }

    /// Persists the verifier's historical result without treating success as
    /// completion. Applicability is deliberately evaluated by completion replay.
    pub fn record_verification(
        &self,
        intent: RecordVerificationIntent,
    ) -> Result<LedgerEvent, OrchestrationError> {
        intent.validate()?;
        let events = self.ledger.events(&intent.task_id)?;
        let current = project_at_expected(&intent.task_id, &events, intent.expected_revision)?;
        if current.projection.state == TaskState::Completed {
            return Err(OrchestrationError::InvalidTransition {
                from: TaskState::Completed,
                to: TaskState::Completed,
            });
        }
        if current
            .evidence
            .iter()
            .any(|item| item.evidence_id == intent.evidence.evidence_id)
        {
            return Err(OrchestrationError::EvidenceIdentityConflict(
                intent.evidence.evidence_id,
            ));
        }
        self.append(
            intent.task_id,
            intent.event_id,
            intent.idempotency_key,
            intent.expected_revision,
            TaskEventPayload::SemanticVerificationRecorded {
                evidence: intent.evidence,
            },
        )
    }

    /// The sole production path to TaskCompleted. It decides exclusively from
    /// the replayed prefix at `expected_revision`, then durably appends completion.
    pub fn complete_task(
        &self,
        intent: CompleteTaskIntent,
    ) -> Result<LedgerEvent, OrchestrationError> {
        intent.validate()?;
        let events = self.ledger.events(&intent.task_id)?;
        let current = project_at_expected(&intent.task_id, &events, intent.expected_revision)?;
        if intent.basis != current.projection.input_basis {
            return Err(OrchestrationError::EvidenceBasisMismatch);
        }
        let evidence = current
            .evidence
            .iter()
            .find(|e| e.evidence_id == intent.evidence_id)
            .ok_or_else(|| OrchestrationError::EvidenceNotFound(intent.evidence_id.clone()))?;
        if evidence.basis != current.projection.input_basis {
            return Err(OrchestrationError::EvidenceBasisMismatch);
        }
        if evidence.outcome != VerificationOutcome::Verified {
            return Err(OrchestrationError::EvidenceFailed(intent.evidence_id));
        }
        if !matches!(
            current.projection.state,
            TaskState::Ready | TaskState::Blocked
        ) {
            return Err(OrchestrationError::InvalidTransition {
                from: current.projection.state,
                to: TaskState::Completed,
            });
        }
        self.append(
            intent.task_id,
            intent.event_id,
            intent.idempotency_key,
            intent.expected_revision,
            TaskEventPayload::TaskCompleted {
                evidence_id: evidence.evidence_id.clone(),
                basis: intent.basis,
            },
        )
    }

    fn append(
        &self,
        task_id: TaskId,
        event_id: autocoder_contracts::EventId,
        idempotency_key: autocoder_contracts::IdempotencyKey,
        expected_revision: u64,
        payload: TaskEventPayload,
    ) -> Result<LedgerEvent, OrchestrationError> {
        let stream_revision = expected_revision.checked_add(1).ok_or_else(|| {
            OrchestrationError::IncompatibleHistory("stream revision overflow".into())
        })?;
        Ok(self.ledger.append(
            expected_revision,
            LedgerEvent {
                schema_version: CONTRACT_VERSION,
                task_id,
                event_id,
                stream_revision,
                idempotency_key,
                payload,
            },
        )?)
    }
}

struct ReplayState {
    projection: TaskProjection,
    evidence: Vec<SemanticVerificationEvidence>,
}

fn project_at_expected(
    task_id: &TaskId,
    events: &[LedgerEvent],
    expected: u64,
) -> Result<ReplayState, OrchestrationError> {
    let actual = events.len() as u64;
    if expected > actual {
        return Err(LedgerError::RevisionConflict { expected, actual }.into());
    }
    project_state(task_id, &events[..expected as usize])
}

fn project(task_id: &TaskId, events: &[LedgerEvent]) -> Result<TaskProjection, OrchestrationError> {
    Ok(project_state(task_id, events)?.projection)
}

fn project_state(
    task_id: &TaskId,
    events: &[LedgerEvent],
) -> Result<ReplayState, OrchestrationError> {
    let first = events
        .first()
        .ok_or_else(|| OrchestrationError::TaskNotFound(task_id.clone()))?;
    validate_envelope(task_id, first, 1)?;
    let (workspace_id, intent, input_revision) = match &first.payload {
        TaskEventPayload::TaskCreated {
            workspace_id,
            intent,
            input_revision,
        } => (workspace_id.clone(), intent.clone(), input_revision.clone()),
        _ => {
            return Err(OrchestrationError::IncompatibleHistory(
                "first event is not task_created".into(),
            ))
        }
    };
    let input_basis = VerificationBasis {
        schema_version: CONTRACT_VERSION,
        task_id: task_id.clone(),
        task_created_event_id: first.event_id.clone(),
        workspace_id: workspace_id.clone(),
        input_revision,
    };
    let mut state = TaskState::Created;
    let mut evidence: Vec<SemanticVerificationEvidence> = Vec::new();
    let mut completion_evidence_id = None;
    for (index, event) in events.iter().enumerate().skip(1) {
        let revision = index as u64 + 1;
        validate_envelope(task_id, event, revision)?;
        match &event.payload {
            TaskEventPayload::TaskReady
                if matches!(state, TaskState::Created | TaskState::Blocked) =>
            {
                state = TaskState::Ready
            }
            TaskEventPayload::TaskBlocked if state == TaskState::Ready => {
                state = TaskState::Blocked
            }
            TaskEventPayload::SemanticVerificationRecorded { evidence: item } => {
                if item.schema_version != CONTRACT_VERSION
                    || item.basis.schema_version != CONTRACT_VERSION
                {
                    return Err(OrchestrationError::IncompatibleHistory(format!(
                        "unsupported verification evidence/basis version at revision {revision}"
                    )));
                }
                if evidence
                    .iter()
                    .any(|existing| existing.evidence_id == item.evidence_id)
                {
                    return Err(OrchestrationError::IncompatibleHistory(format!(
                        "conflicting or duplicate evidence identity {}",
                        item.evidence_id
                    )));
                }
                evidence.push(item.clone());
            }
            TaskEventPayload::TaskCompleted { evidence_id, basis } => {
                if basis.schema_version != CONTRACT_VERSION {
                    return Err(OrchestrationError::IncompatibleHistory(format!(
                        "unsupported completion basis version at revision {revision}"
                    )));
                }
                let proof = evidence
                    .iter()
                    .find(|item| &item.evidence_id == evidence_id)
                    .ok_or_else(|| {
                        OrchestrationError::IncompatibleHistory(format!(
                            "completion references absent evidence {evidence_id}"
                        ))
                    })?;
                if basis != &input_basis
                    || &proof.basis != basis
                    || proof.outcome != VerificationOutcome::Verified
                {
                    return Err(OrchestrationError::IncompatibleHistory(format!("completion evidence is failed, stale, or inapplicable at revision {revision}")));
                }
                if !matches!(state, TaskState::Ready | TaskState::Blocked) {
                    return Err(OrchestrationError::IncompatibleHistory(format!(
                        "invalid transition from {state:?} to Completed at revision {revision}"
                    )));
                }
                state = TaskState::Completed;
                completion_evidence_id = Some(evidence_id.clone());
            }
            TaskEventPayload::TaskCreated { .. } => {
                return Err(OrchestrationError::IncompatibleHistory(
                    "task_created occurs more than once".into(),
                ))
            }
            _ => {
                return Err(OrchestrationError::IncompatibleHistory(format!(
                    "invalid lifecycle event at revision {revision}"
                )))
            }
        }
    }
    Ok(ReplayState {
        projection: TaskProjection {
            schema_version: CONTRACT_VERSION,
            task_id: task_id.clone(),
            workspace_id,
            intent,
            input_basis,
            state,
            stream_revision: events.len() as u64,
            completion_evidence_id,
        },
        evidence,
    })
}

fn validate_envelope(
    task_id: &TaskId,
    event: &LedgerEvent,
    revision: u64,
) -> Result<(), OrchestrationError> {
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
    Ok(())
}
