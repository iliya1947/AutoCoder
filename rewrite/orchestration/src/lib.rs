use autocoder_contracts::{CreateTaskIntent, LedgerEvent, TaskEventPayload, CONTRACT_VERSION};
use autocoder_ledger::{ExecutionLedger, LedgerError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OrchestrationError {
    #[error(transparent)]
    Contract(#[from] autocoder_contracts::ContractError),
    #[error(transparent)]
    Ledger(#[from] LedgerError),
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
        Ok(self.ledger.append(
            &intent.task_id,
            intent.expected_revision,
            &intent.idempotency_key,
            event,
        )?)
    }
}
