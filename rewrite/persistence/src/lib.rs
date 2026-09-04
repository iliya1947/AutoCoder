use autocoder_contracts::{LedgerEvent, TaskId, CONTRACT_VERSION};
use autocoder_ledger::{ExecutionLedger, LedgerError};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::{path::Path, sync::Mutex};

pub struct SqliteLedger {
    connection: Mutex<Connection>,
}

impl SqliteLedger {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, LedgerError> {
        let connection = Connection::open(path).map_err(storage)?;
        connection.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS execution_events (task_id TEXT NOT NULL, revision INTEGER NOT NULL, event_id TEXT NOT NULL UNIQUE, idempotency_key TEXT NOT NULL UNIQUE, body TEXT NOT NULL, PRIMARY KEY(task_id, revision));").map_err(storage)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }
}

fn storage(error: rusqlite::Error) -> LedgerError {
    LedgerError::Storage(error.to_string())
}

impl ExecutionLedger for SqliteLedger {
    fn append(
        &self,
        expected_revision: u64,
        event: LedgerEvent,
    ) -> Result<LedgerEvent, LedgerError> {
        if event.schema_version != CONTRACT_VERSION {
            return Err(LedgerError::InvalidEnvelope(format!(
                "unsupported schema version {}",
                event.schema_version
            )));
        }
        let next_revision = expected_revision.checked_add(1).ok_or_else(|| {
            LedgerError::InvalidEnvelope("expected revision cannot be incremented".into())
        })?;
        if event.stream_revision != next_revision {
            return Err(LedgerError::InvalidEnvelope(format!(
                "event revision {} does not follow expected revision {}",
                event.stream_revision, expected_revision
            )));
        }
        let task_id = &event.task_id;
        let key = &event.idempotency_key;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| LedgerError::Storage("ledger mutex poisoned".into()))?;
        // IMMEDIATE serializes competing writers before checking stream revision.
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        let by_event_id: Option<String> = transaction
            .query_row(
                "SELECT body FROM execution_events WHERE event_id=?1",
                [event.event_id.as_str()],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage)?;
        let by_key: Option<String> = transaction
            .query_row(
                "SELECT body FROM execution_events WHERE idempotency_key=?1",
                [key.as_str()],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage)?;
        let matches_retry = |body: &str| -> Result<bool, LedgerError> {
            let stored: LedgerEvent = serde_json::from_str(body)
                .map_err(|error| LedgerError::Storage(error.to_string()))?;
            Ok(stored == event && expected_revision + 1 == stored.stream_revision)
        };
        match (by_event_id.as_deref(), by_key.as_deref()) {
            (Some(event_body), Some(key_body))
                if event_body == key_body && matches_retry(event_body)? =>
            {
                return Ok(event);
            }
            (Some(_), _) | (_, Some(_)) => return Err(LedgerError::IdentityConflict),
            (None, None) => {}
        }
        let actual: u64 = transaction
            .query_row(
                "SELECT COALESCE(MAX(revision), 0) FROM execution_events WHERE task_id=?1",
                [task_id.as_str()],
                |row| row.get(0),
            )
            .map_err(storage)?;
        if actual != expected_revision {
            return Err(LedgerError::RevisionConflict {
                expected: expected_revision,
                actual,
            });
        }
        let body =
            serde_json::to_string(&event).map_err(|e| LedgerError::Storage(e.to_string()))?;
        transaction.execute("INSERT INTO execution_events(task_id, revision, event_id, idempotency_key, body) VALUES (?1, ?2, ?3, ?4, ?5)", params![task_id.as_str(), event.stream_revision, event.event_id.as_str(), key.as_str(), body]).map_err(storage)?;
        transaction.commit().map_err(storage)?;
        Ok(event)
    }

    fn events(&self, task_id: &TaskId) -> Result<Vec<LedgerEvent>, LedgerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| LedgerError::Storage("ledger mutex poisoned".into()))?;
        let mut statement = connection
            .prepare("SELECT body FROM execution_events WHERE task_id=?1 ORDER BY revision")
            .map_err(storage)?;
        let rows = statement
            .query_map([task_id.as_str()], |row| row.get::<_, String>(0))
            .map_err(storage)?;
        rows.map(|row| {
            row.map_err(storage).and_then(|body| {
                serde_json::from_str(&body).map_err(|e| LedgerError::Storage(e.to_string()))
            })
        })
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use autocoder_contracts::*;

    fn event(event_id: &str, key: &str) -> LedgerEvent {
        LedgerEvent {
            schema_version: CONTRACT_VERSION,
            task_id: TaskId::parse("task-1").unwrap(),
            event_id: EventId::parse(event_id).unwrap(),
            stream_revision: 1,
            idempotency_key: IdempotencyKey::parse(key).unwrap(),
            payload: TaskEventPayload::TaskCreated {
                workspace_id: WorkspaceId::parse("workspace-1").unwrap(),
                intent: "intent".into(),
                input_revision: InputRevision::parse("input-1").unwrap(),
            },
        }
    }

    #[test]
    fn exact_logical_append_retry_returns_the_committed_event() {
        let ledger = SqliteLedger::open(":memory:").unwrap();
        let append = event("event-1", "request-1");
        assert_eq!(ledger.append(0, append.clone()).unwrap(), append);
        assert_eq!(ledger.append(0, append.clone()).unwrap(), append);
        assert_eq!(ledger.events(&append.task_id).unwrap().len(), 1);
    }

    #[test]
    fn conflicting_event_id_or_idempotency_key_is_rejected() {
        let ledger = SqliteLedger::open(":memory:").unwrap();
        ledger.append(0, event("event-1", "request-1")).unwrap();

        let mut changed_payload = event("event-1", "request-1");
        changed_payload.payload = TaskEventPayload::TaskCreated {
            workspace_id: WorkspaceId::parse("workspace-1").unwrap(),
            intent: "different intent".into(),
            input_revision: InputRevision::parse("input-1").unwrap(),
        };
        assert!(matches!(
            ledger.append(0, changed_payload),
            Err(LedgerError::IdentityConflict)
        ));
        assert!(matches!(
            ledger.append(0, event("event-2", "request-1")),
            Err(LedgerError::IdentityConflict)
        ));
        assert!(matches!(
            ledger.append(0, event("event-1", "request-2")),
            Err(LedgerError::IdentityConflict)
        ));
    }

    #[test]
    fn stale_append_and_inconsistent_envelope_are_rejected() {
        let ledger = SqliteLedger::open(":memory:").unwrap();
        ledger.append(0, event("event-1", "request-1")).unwrap();
        assert!(matches!(
            ledger.append(0, event("event-2", "request-2")),
            Err(LedgerError::RevisionConflict {
                expected: 0,
                actual: 1
            })
        ));

        let mut wrong_revision = event("event-3", "request-3");
        wrong_revision.stream_revision = 2;
        assert!(matches!(
            ledger.append(0, wrong_revision),
            Err(LedgerError::InvalidEnvelope(_))
        ));
        let mut wrong_version = event("event-4", "request-4");
        wrong_version.schema_version = CONTRACT_VERSION + 1;
        assert!(matches!(
            ledger.append(0, wrong_version),
            Err(LedgerError::InvalidEnvelope(_))
        ));
    }

    #[test]
    fn competing_ledger_instances_observe_durable_stream_revision() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ledger.sqlite");
        let first = SqliteLedger::open(&path).unwrap();
        let second = SqliteLedger::open(&path).unwrap();
        first.append(0, event("event-1", "request-1")).unwrap();

        assert!(matches!(
            second.append(0, event("event-2", "request-2")),
            Err(LedgerError::RevisionConflict {
                expected: 0,
                actual: 1
            })
        ));
    }
}
