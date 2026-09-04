use autocoder_contracts::{LedgerEvent, TaskId};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LedgerError {
    #[error("revision conflict: expected {expected}, actual {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("stable append identity was reused with a conflicting event")]
    IdentityConflict,
    #[error("inconsistent event envelope: {0}")]
    InvalidEnvelope(String),
    #[error("ledger storage error: {0}")]
    Storage(String),
}

pub trait ExecutionLedger: Send + Sync {
    fn append(
        &self,
        expected_revision: u64,
        event: LedgerEvent,
    ) -> Result<LedgerEvent, LedgerError>;
    fn events(&self, task_id: &TaskId) -> Result<Vec<LedgerEvent>, LedgerError>;
}
