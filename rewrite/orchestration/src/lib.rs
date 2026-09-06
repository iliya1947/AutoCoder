use autocoder_contracts::{
    AttemptAuthority, AttemptOutcomeContradiction, AttemptProjection, CompleteTaskIntent,
    CreateTaskIntent, DefineStepIntent, DurableStepProjection, EvidenceId, LedgerEvent,
    ReconcileAttemptIntent, ReconciliationConclusion, RecordAttemptObservationIntent,
    RecordAttemptOutcomeIntent, RecordVerificationIntent, SemanticVerificationEvidence,
    StartAttemptIntent, StepId, TaskEventPayload, TaskId, TaskProjection, TaskState,
    TransitionTaskIntent, VerificationBasis, VerificationOutcome, CONTRACT_VERSION,
};
use autocoder_ledger::{ExecutionLedger, LedgerError};
use std::collections::HashMap;
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
    #[error("step {0} does not exist")]
    StepNotFound(StepId),
    #[error("step identity {0} is already present in task history")]
    StepIdentityConflict(StepId),
    #[error("attempt identity is already present in task history")]
    AttemptIdentityConflict,
    #[error("attempt or its execution-authority token does not match durable history")]
    AttemptAuthorityMismatch,
    #[error("attempt already has a terminal outcome")]
    AttemptAlreadyTerminal,
    #[error("attempt outcome is unknown; a durable retry-authorizing reconciliation is required")]
    UnknownAttemptRequiresReconciliation,
    #[error("observation identity is already present in task history")]
    ObservationIdentityConflict,
    #[error("observation or reconciliation scope does not match durable attempt history")]
    ReconciliationScopeMismatch,
    #[error("reconciliation references an absent or differently scoped observation")]
    ReconciliationEvidenceMismatch,
    #[error("reconciliation identity is already present in task history")]
    AttemptAlreadyReconciled,
    #[error("reconciliation does not reference any new durable observation")]
    ReconciliationEvidenceNotAdvanced,
    #[error("task {0} is completed and cannot create new execution work")]
    TaskClosed(TaskId),
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

    pub fn define_step(&self, intent: DefineStepIntent) -> Result<LedgerEvent, OrchestrationError> {
        intent.validate()?;
        let current = project_at_expected(
            &intent.task_id,
            &self.ledger.events(&intent.task_id)?,
            intent.expected_revision,
        )?;
        if current.projection.state == TaskState::Completed {
            return Err(OrchestrationError::TaskClosed(intent.task_id));
        }
        if current
            .projection
            .steps
            .iter()
            .any(|step| step.step_id == intent.step_id)
        {
            return Err(OrchestrationError::StepIdentityConflict(intent.step_id));
        }
        self.append(
            intent.task_id,
            intent.event_id,
            intent.idempotency_key,
            intent.expected_revision,
            TaskEventPayload::StepDefined {
                step_id: intent.step_id,
                description: intent.description,
            },
        )
    }

    pub fn start_attempt(
        &self,
        intent: StartAttemptIntent,
    ) -> Result<LedgerEvent, OrchestrationError> {
        intent.validate()?;
        let current = project_at_expected(
            &intent.task_id,
            &self.ledger.events(&intent.task_id)?,
            intent.expected_revision,
        )?;
        if current.projection.state == TaskState::Completed {
            return Err(OrchestrationError::TaskClosed(intent.task_id));
        }
        if current
            .projection
            .steps
            .iter()
            .flat_map(|step| &step.attempts)
            .any(|attempt| attempt.attempt_id == intent.attempt_id)
        {
            return Err(OrchestrationError::AttemptIdentityConflict);
        }
        let step = current
            .projection
            .steps
            .iter()
            .find(|step| step.step_id == intent.step_id)
            .ok_or_else(|| OrchestrationError::StepNotFound(intent.step_id.clone()))?;
        if let Some(current_id) = &step.current_attempt_id {
            let current_attempt = step
                .attempts
                .iter()
                .find(|attempt| &attempt.attempt_id == current_id)
                .expect("replay keeps current attempt identity consistent");
            let retry_authorized = matches!(
                current_attempt
                    .reconciliations
                    .last()
                    .map(|item| &item.conclusion),
                Some(ReconciliationConclusion::RetryAuthorized)
            );
            if current_attempt.outcome.is_none() && !retry_authorized {
                return Err(OrchestrationError::UnknownAttemptRequiresReconciliation);
            }
        }
        let generation = step.authority_generation.checked_add(1).ok_or_else(|| {
            OrchestrationError::IncompatibleHistory(
                "execution authority generation overflow".into(),
            )
        })?;
        self.append(
            intent.task_id,
            intent.event_id,
            intent.idempotency_key,
            intent.expected_revision,
            TaskEventPayload::AttemptStarted {
                step_id: intent.step_id,
                attempt_id: intent.attempt_id,
                generation,
            },
        )
    }

    pub fn record_attempt_outcome(
        &self,
        intent: RecordAttemptOutcomeIntent,
    ) -> Result<LedgerEvent, OrchestrationError> {
        intent.validate()?;
        let current = project_at_expected(
            &intent.task_id,
            &self.ledger.events(&intent.task_id)?,
            intent.expected_revision,
        )?;
        let step = current
            .projection
            .steps
            .iter()
            .find(|step| step.step_id == intent.step_id)
            .ok_or_else(|| OrchestrationError::StepNotFound(intent.step_id.clone()))?;
        let attempt = step
            .attempts
            .iter()
            .find(|attempt| attempt.attempt_id == intent.attempt_id)
            .ok_or(OrchestrationError::AttemptAuthorityMismatch)?;
        if attempt.generation != intent.authority_generation {
            return Err(OrchestrationError::AttemptAuthorityMismatch);
        }
        if attempt.outcome.is_some() {
            return Err(OrchestrationError::AttemptAlreadyTerminal);
        }
        self.append(
            intent.task_id,
            intent.event_id,
            intent.idempotency_key,
            intent.expected_revision,
            TaskEventPayload::AttemptOutcomeRecorded {
                step_id: intent.step_id,
                attempt_id: intent.attempt_id,
                authority_generation: intent.authority_generation,
                outcome: intent.outcome,
            },
        )
    }

    pub fn record_attempt_observation(
        &self,
        intent: RecordAttemptObservationIntent,
    ) -> Result<LedgerEvent, OrchestrationError> {
        intent.validate()?;
        let events = self.ledger.events(&intent.task_id)?;
        // Let Ledger distinguish an exact retry from a stale writer before
        // scope validation against the writer's older prefix.
        if (events.len() as u64) > intent.expected_revision {
            return self.append(
                intent.task_id,
                intent.event_id,
                intent.idempotency_key,
                intent.expected_revision,
                TaskEventPayload::AttemptObservationRecorded {
                    observation: intent.observation,
                },
            );
        }
        let current = project_at_expected(&intent.task_id, &events, intent.expected_revision)?;
        if current
            .projection
            .steps
            .iter()
            .flat_map(|step| &step.attempts)
            .flat_map(|attempt| &attempt.observations)
            .any(|item| item.observation_id == intent.observation.observation_id)
        {
            return Err(OrchestrationError::ObservationIdentityConflict);
        }
        find_scoped_attempt(&current.projection, &intent.observation.scope)?;
        self.append(
            intent.task_id,
            intent.event_id,
            intent.idempotency_key,
            intent.expected_revision,
            TaskEventPayload::AttemptObservationRecorded {
                observation: intent.observation,
            },
        )
    }

    /// Orchestration owns this decision and derives it only from the durable
    /// prefix named by `expected_revision`; no live source is consulted here.
    pub fn reconcile_attempt(
        &self,
        intent: ReconcileAttemptIntent,
    ) -> Result<LedgerEvent, OrchestrationError> {
        intent.validate()?;
        let events = self.ledger.events(&intent.task_id)?;
        if (events.len() as u64) > intent.expected_revision {
            return self.append(
                intent.task_id,
                intent.event_id,
                intent.idempotency_key,
                intent.expected_revision,
                TaskEventPayload::AttemptReconciled {
                    reconciliation: intent.reconciliation,
                },
            );
        }
        let current = project_at_expected(&intent.task_id, &events, intent.expected_revision)?;
        if current
            .projection
            .steps
            .iter()
            .flat_map(|step| &step.attempts)
            .flat_map(|attempt| &attempt.reconciliations)
            .any(|item| item.reconciliation_id == intent.reconciliation.reconciliation_id)
        {
            return Err(OrchestrationError::AttemptAlreadyReconciled);
        }
        let attempt = find_scoped_attempt(&current.projection, &intent.reconciliation.scope)?;
        if attempt.outcome.is_some() {
            return Err(OrchestrationError::AttemptAlreadyTerminal);
        }
        if intent.reconciliation.observation_ids.iter().any(|id| {
            !attempt
                .observations
                .iter()
                .any(|observation| &observation.observation_id == id)
        }) {
            return Err(OrchestrationError::ReconciliationEvidenceMismatch);
        }
        let prior_decision_revision = attempt
            .effective_reconciliation_id
            .as_ref()
            .and_then(|id| current.reconciliation_revisions.get(id))
            .copied()
            .unwrap_or(0);
        let advances_evidence = intent.reconciliation.observation_ids.iter().any(|id| {
            current
                .observation_revisions
                .get(id)
                .is_some_and(|revision| *revision > prior_decision_revision)
        });
        if !advances_evidence {
            return Err(OrchestrationError::ReconciliationEvidenceNotAdvanced);
        }
        self.append(
            intent.task_id,
            intent.event_id,
            intent.idempotency_key,
            intent.expected_revision,
            TaskEventPayload::AttemptReconciled {
                reconciliation: intent.reconciliation,
            },
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
    observation_revisions: HashMap<autocoder_contracts::ObservationId, u64>,
    reconciliation_revisions: HashMap<autocoder_contracts::ReconciliationId, u64>,
}

fn find_scoped_attempt<'a>(
    projection: &'a TaskProjection,
    scope: &autocoder_contracts::AttemptScope,
) -> Result<&'a AttemptProjection, OrchestrationError> {
    if scope.task_id != projection.task_id {
        return Err(OrchestrationError::ReconciliationScopeMismatch);
    }
    projection
        .steps
        .iter()
        .find(|step| step.step_id == scope.step_id)
        .and_then(|step| {
            step.attempts.iter().find(|attempt| {
                attempt.attempt_id == scope.attempt_id
                    && attempt.generation == scope.authority_generation
            })
        })
        .ok_or(OrchestrationError::ReconciliationScopeMismatch)
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
    let mut steps: Vec<DurableStepProjection> = Vec::new();
    let mut observation_revisions = HashMap::new();
    let mut reconciliation_revisions = HashMap::new();
    let mut reconciliation_era_seen = false;
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
                for attempt in steps.iter_mut().flat_map(|step| &mut step.attempts) {
                    if attempt.authority == AttemptAuthority::Current {
                        attempt.authority = AttemptAuthority::Revoked;
                    }
                }
            }
            TaskEventPayload::StepDefined {
                step_id,
                description,
            } => {
                if state == TaskState::Completed {
                    return Err(OrchestrationError::IncompatibleHistory(format!(
                        "step defined after task completion at revision {revision}"
                    )));
                }
                if steps.iter().any(|step| &step.step_id == step_id) {
                    return Err(OrchestrationError::IncompatibleHistory(format!(
                        "duplicate step identity {step_id}"
                    )));
                }
                steps.push(DurableStepProjection {
                    step_id: step_id.clone(),
                    description: description.clone(),
                    authority_generation: 0,
                    current_attempt_id: None,
                    attempts: Vec::new(),
                });
            }
            TaskEventPayload::AttemptStarted {
                step_id,
                attempt_id,
                generation,
            } => {
                if state == TaskState::Completed {
                    return Err(OrchestrationError::IncompatibleHistory(format!(
                        "attempt started after task completion at revision {revision}"
                    )));
                }
                if steps
                    .iter()
                    .flat_map(|step| &step.attempts)
                    .any(|attempt| &attempt.attempt_id == attempt_id)
                {
                    return Err(OrchestrationError::IncompatibleHistory(
                        "duplicate attempt identity".into(),
                    ));
                }
                let step = steps
                    .iter_mut()
                    .find(|step| &step.step_id == step_id)
                    .ok_or_else(|| {
                        OrchestrationError::IncompatibleHistory(format!(
                            "attempt references absent step {step_id}"
                        ))
                    })?;
                if let Some(current_id) = &step.current_attempt_id {
                    let current = step
                        .attempts
                        .iter()
                        .find(|attempt| &attempt.attempt_id == current_id)
                        .ok_or_else(|| {
                            OrchestrationError::IncompatibleHistory(
                                "current attempt identity is absent".into(),
                            )
                        })?;
                    let retry_authorized = matches!(
                        current.reconciliations.last().map(|item| &item.conclusion),
                        Some(ReconciliationConclusion::RetryAuthorized)
                    );
                    // Only the stream prefix before the first reconciliation-
                    // era fact can be grandfathered as legacy V1 history.
                    if current.outcome.is_none() && reconciliation_era_seen && !retry_authorized {
                        return Err(OrchestrationError::IncompatibleHistory(format!(
                            "unknown reconciled attempt superseded without retry authorization at revision {revision}"
                        )));
                    }
                }
                if *generation
                    != step.authority_generation.checked_add(1).ok_or_else(|| {
                        OrchestrationError::IncompatibleHistory(
                            "execution authority generation overflow".into(),
                        )
                    })?
                {
                    return Err(OrchestrationError::IncompatibleHistory(format!(
                        "non-monotonic authority generation at revision {revision}"
                    )));
                }
                for attempt in &mut step.attempts {
                    attempt.authority = AttemptAuthority::Superseded;
                }
                step.authority_generation = *generation;
                step.current_attempt_id = Some(attempt_id.clone());
                step.attempts.push(AttemptProjection {
                    attempt_id: attempt_id.clone(),
                    generation: *generation,
                    outcome: None,
                    authority: AttemptAuthority::Current,
                    observations: Vec::new(),
                    reconciliations: Vec::new(),
                    effective_reconciliation_id: None,
                    contradictions: Vec::new(),
                });
            }
            TaskEventPayload::AttemptOutcomeRecorded {
                step_id,
                attempt_id,
                authority_generation,
                outcome,
            } => {
                let step = steps
                    .iter_mut()
                    .find(|step| &step.step_id == step_id)
                    .ok_or_else(|| {
                        OrchestrationError::IncompatibleHistory(format!(
                            "outcome references absent step {step_id}"
                        ))
                    })?;
                let attempt = step
                    .attempts
                    .iter_mut()
                    .find(|attempt| &attempt.attempt_id == attempt_id)
                    .ok_or_else(|| {
                        OrchestrationError::IncompatibleHistory(
                            "outcome references absent attempt".into(),
                        )
                    })?;
                if attempt.generation != *authority_generation || attempt.outcome.is_some() {
                    return Err(OrchestrationError::IncompatibleHistory(format!(
                        "invalid attempt outcome authority at revision {revision}"
                    )));
                }
                for reconciliation in &attempt.reconciliations {
                    if let ReconciliationConclusion::ConfirmedOutcome {
                        outcome: reconciled_outcome,
                    } = reconciliation.conclusion
                    {
                        if reconciled_outcome != *outcome {
                            attempt.contradictions.push(AttemptOutcomeContradiction {
                                reconciliation_id: reconciliation.reconciliation_id.clone(),
                                outcome_event_id: event.event_id.clone(),
                                reconciled_outcome,
                                late_outcome: *outcome,
                            });
                        }
                    }
                }
                attempt.outcome = Some(*outcome);
            }
            TaskEventPayload::AttemptObservationRecorded { observation } => {
                reconciliation_era_seen = true;
                if observation.validate().is_err() {
                    return Err(OrchestrationError::IncompatibleHistory(format!(
                        "unsupported attempt observation version at revision {revision}"
                    )));
                }
                if observation.scope.task_id != *task_id
                    || steps
                        .iter()
                        .flat_map(|step| &step.attempts)
                        .flat_map(|attempt| &attempt.observations)
                        .any(|existing| existing.observation_id == observation.observation_id)
                {
                    return Err(OrchestrationError::IncompatibleHistory(format!(
                        "invalid or duplicate observation identity at revision {revision}"
                    )));
                }
                let attempt = steps
                    .iter_mut()
                    .find(|step| step.step_id == observation.scope.step_id)
                    .and_then(|step| {
                        step.attempts.iter_mut().find(|attempt| {
                            attempt.attempt_id == observation.scope.attempt_id
                                && attempt.generation == observation.scope.authority_generation
                        })
                    })
                    .ok_or_else(|| {
                        OrchestrationError::IncompatibleHistory(format!(
                            "observation has invalid attempt scope at revision {revision}"
                        ))
                    })?;
                attempt.observations.push(observation.clone());
                observation_revisions.insert(observation.observation_id.clone(), revision);
            }
            TaskEventPayload::AttemptReconciled { reconciliation } => {
                reconciliation_era_seen = true;
                if reconciliation.validate().is_err() {
                    return Err(OrchestrationError::IncompatibleHistory(format!(
                        "unsupported attempt reconciliation version at revision {revision}"
                    )));
                }
                if reconciliation.scope.task_id != *task_id {
                    return Err(OrchestrationError::IncompatibleHistory(format!(
                        "reconciliation has invalid task scope at revision {revision}"
                    )));
                }
                if steps
                    .iter()
                    .flat_map(|step| &step.attempts)
                    .flat_map(|attempt| &attempt.reconciliations)
                    .any(|existing| existing.reconciliation_id == reconciliation.reconciliation_id)
                {
                    return Err(OrchestrationError::IncompatibleHistory(format!(
                        "duplicate reconciliation identity at revision {revision}"
                    )));
                }
                let attempt = steps
                    .iter_mut()
                    .find(|step| step.step_id == reconciliation.scope.step_id)
                    .and_then(|step| {
                        step.attempts.iter_mut().find(|attempt| {
                            attempt.attempt_id == reconciliation.scope.attempt_id
                                && attempt.generation == reconciliation.scope.authority_generation
                        })
                    })
                    .ok_or_else(|| {
                        OrchestrationError::IncompatibleHistory(format!(
                            "reconciliation has invalid attempt scope at revision {revision}"
                        ))
                    })?;
                if attempt.outcome.is_some()
                    || reconciliation.observation_ids.is_empty()
                    || reconciliation.observation_ids.iter().any(|id| {
                        !attempt
                            .observations
                            .iter()
                            .any(|observation| &observation.observation_id == id)
                    })
                {
                    return Err(OrchestrationError::IncompatibleHistory(format!(
                        "invalid reconciliation evidence or state at revision {revision}"
                    )));
                }
                let prior_decision_revision = attempt
                    .effective_reconciliation_id
                    .as_ref()
                    .and_then(|id| reconciliation_revisions.get(id))
                    .copied()
                    .unwrap_or(0);
                if !reconciliation.observation_ids.iter().any(|id| {
                    observation_revisions
                        .get(id)
                        .is_some_and(|recorded| *recorded > prior_decision_revision)
                }) {
                    return Err(OrchestrationError::IncompatibleHistory(format!(
                        "reconciliation does not advance durable evidence at revision {revision}"
                    )));
                }
                attempt.reconciliations.push(reconciliation.clone());
                attempt.effective_reconciliation_id =
                    Some(reconciliation.reconciliation_id.clone());
                reconciliation_revisions.insert(reconciliation.reconciliation_id.clone(), revision);
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
            steps,
        },
        evidence,
        observation_revisions,
        reconciliation_revisions,
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
