use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;
use thiserror::Error;

pub const CONTRACT_VERSION: u16 = 1;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ContractError {
    #[error("identifier must not be empty")]
    EmptyIdentifier,
    #[error("unsupported contract version {0}")]
    UnsupportedVersion(u16),
    #[error("verification provenance field {0} must not be empty")]
    EmptyVerificationField(&'static str),
}

macro_rules! identifier {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
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
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::parse(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_validated_during_deserialization() {
        let error = serde_json::from_str::<TaskId>(r#""   ""#).unwrap_err();
        assert!(error.to_string().contains("identifier must not be empty"));
    }

    #[test]
    fn legacy_create_input_revision_is_stable_and_event_scoped() {
        let event_id = EventId::parse("event-from-main").unwrap();
        assert_eq!(
            legacy_create_v1_input_revision(&event_id).as_str(),
            "autocoder:legacy-create-v1:event:event-from-main"
        );
    }
}

identifier!(WorkspaceId);
identifier!(TaskId);
identifier!(EventId);
identifier!(IdempotencyKey);
identifier!(EvidenceId);
identifier!(InputRevision);

const LEGACY_CREATE_V1_INPUT_REVISION_PREFIX: &str = "autocoder:legacy-create-v1:event:";

/// Deterministic input-basis reference for a v1 create written before
/// `input_revision` became part of the contract. The event identity is shared by
/// the durable event and the UI's pending request, so both upgrade paths derive
/// exactly the same value without consulting current workspace state.
pub fn legacy_create_v1_input_revision(event_id: &EventId) -> InputRevision {
    InputRevision::parse(format!(
        "{LEGACY_CREATE_V1_INPUT_REVISION_PREFIX}{}",
        event_id.as_str()
    ))
    .expect("the non-empty prefix makes the derived input revision valid")
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CreateTaskIntent {
    pub contract_version: u16,
    pub workspace_id: WorkspaceId,
    pub task_id: TaskId,
    pub intent: String,
    pub input_revision: InputRevision,
    pub event_id: EventId,
    pub idempotency_key: IdempotencyKey,
    pub expected_revision: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Created,
    Ready,
    Blocked,
    Completed,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct TransitionTaskIntent {
    pub contract_version: u16,
    pub task_id: TaskId,
    pub target_state: TaskState,
    pub event_id: EventId,
    pub idempotency_key: IdempotencyKey,
    pub expected_revision: u64,
}

impl TransitionTaskIntent {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.contract_version != CONTRACT_VERSION {
            return Err(ContractError::UnsupportedVersion(self.contract_version));
        }
        Ok(())
    }
}

/// Read model rebuilt from the task's event stream. It is never stored separately.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct TaskProjection {
    pub schema_version: u16,
    pub task_id: TaskId,
    pub workspace_id: WorkspaceId,
    pub intent: String,
    pub input_basis: VerificationBasis,
    pub state: TaskState,
    pub stream_revision: u64,
    pub completion_evidence_id: Option<EvidenceId>,
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
        input_revision: InputRevision,
    },
    TaskReady,
    TaskBlocked,
    SemanticVerificationRecorded {
        evidence: SemanticVerificationEvidence,
    },
    TaskCompleted {
        evidence_id: EvidenceId,
        basis: VerificationBasis,
    },
}

/// Minimal AutoCoder-owned reference to the relevant task/workspace input.
/// `input_revision` is opaque to orchestration; a future Workspace subsystem can
/// bind it to a real revision/hash without changing verification semantics.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct VerificationBasis {
    pub schema_version: u16,
    pub task_id: TaskId,
    pub task_created_event_id: EventId,
    pub workspace_id: WorkspaceId,
    pub input_revision: InputRevision,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationOutcome {
    Verified,
    Failed,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct VerificationProvenance {
    pub verifier: String,
    pub verifier_version: String,
    pub method: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SemanticVerificationEvidence {
    pub schema_version: u16,
    pub evidence_id: EvidenceId,
    pub basis: VerificationBasis,
    pub outcome: VerificationOutcome,
    pub provenance: VerificationProvenance,
    pub summary: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct RecordVerificationIntent {
    pub contract_version: u16,
    pub task_id: TaskId,
    pub evidence: SemanticVerificationEvidence,
    pub event_id: EventId,
    pub idempotency_key: IdempotencyKey,
    pub expected_revision: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CompleteTaskIntent {
    pub contract_version: u16,
    pub task_id: TaskId,
    pub evidence_id: EvidenceId,
    pub basis: VerificationBasis,
    pub event_id: EventId,
    pub idempotency_key: IdempotencyKey,
    pub expected_revision: u64,
}

impl RecordVerificationIntent {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_version(self.contract_version)?;
        validate_version(self.evidence.schema_version)?;
        validate_version(self.evidence.basis.schema_version)?;
        for (name, value) in [
            ("verifier", self.evidence.provenance.verifier.as_str()),
            (
                "verifier_version",
                self.evidence.provenance.verifier_version.as_str(),
            ),
            ("method", self.evidence.provenance.method.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(ContractError::EmptyVerificationField(name));
            }
        }
        Ok(())
    }
}

impl CompleteTaskIntent {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_version(self.contract_version)?;
        validate_version(self.basis.schema_version)
    }
}

fn validate_version(version: u16) -> Result<(), ContractError> {
    if version != CONTRACT_VERSION {
        return Err(ContractError::UnsupportedVersion(version));
    }
    Ok(())
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
