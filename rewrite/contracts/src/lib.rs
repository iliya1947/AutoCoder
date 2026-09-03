use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

pub const CONTRACT_VERSION: u16 = 1;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ContractError {
    #[error("identifier must not be empty")]
    EmptyIdentifier,
    #[error("unsupported contract version {0}")]
    UnsupportedVersion(u16),
}

macro_rules! identifier {
    ($name:ident) => {
        #[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn parse(value: impl Into<String>) -> Result<Self, ContractError> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(ContractError::EmptyIdentifier);
                }
                Ok(Self(value))
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

identifier!(WorkspaceId);
identifier!(TaskId);
identifier!(EventId);
identifier!(IdempotencyKey);

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CreateTaskIntent {
    pub contract_version: u16,
    pub workspace_id: WorkspaceId,
    pub task_id: TaskId,
    pub intent: String,
    pub event_id: EventId,
    pub idempotency_key: IdempotencyKey,
    pub expected_revision: u64,
}

impl CreateTaskIntent {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.contract_version != CONTRACT_VERSION {
            return Err(ContractError::UnsupportedVersion(self.contract_version));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskEventPayload {
    TaskCreated {
        workspace_id: WorkspaceId,
        intent: String,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct LedgerEvent {
    pub schema_version: u16,
    pub task_id: TaskId,
    pub event_id: EventId,
    pub stream_revision: u64,
    pub idempotency_key: IdempotencyKey,
    pub payload: TaskEventPayload,
}
