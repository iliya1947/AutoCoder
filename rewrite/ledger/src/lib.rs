use autocoder_contracts::{IdempotencyKey, LedgerEvent, TaskId};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LedgerError {
    #[error("revision conflict: expected {expected}, actual {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("event id already belongs to a different append")]
    EventIdConflict,
    #[error("ledger storage error: {0}")]
    Storage(String),
}

pub trait ExecutionLedger: Send + Sync {
    fn append(
        &self,
        task_id: &TaskId,
        expected_revision: u64,
        idempotency_key: &IdempotencyKey,
        event: LedgerEvent,
    ) -> Result<LedgerEvent, LedgerError>;
    fn events(&self, task_id: &TaskId) -> Result<Vec<LedgerEvent>, LedgerError>;
}
