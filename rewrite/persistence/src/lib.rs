use autocoder_contracts::{IdempotencyKey, LedgerEvent, TaskId};
use autocoder_ledger::{ExecutionLedger, LedgerError};
use rusqlite::{params, Connection, OptionalExtension};
use std::{path::Path, sync::Mutex};

pub struct SqliteLedger {
    connection: Mutex<Connection>,
}

impl SqliteLedger {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, LedgerError> {
        let connection = Connection::open(path).map_err(storage)?;
        connection.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS execution_events (task_id TEXT NOT NULL, revision INTEGER NOT NULL, event_id TEXT NOT NULL UNIQUE, idempotency_key TEXT NOT NULL, body TEXT NOT NULL, PRIMARY KEY(task_id, revision), UNIQUE(task_id, idempotency_key));").map_err(storage)?;
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
        task_id: &TaskId,
        expected_revision: u64,
        key: &IdempotencyKey,
        event: LedgerEvent,
    ) -> Result<LedgerEvent, LedgerError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| LedgerError::Storage("ledger mutex poisoned".into()))?;
        let transaction = connection.transaction().map_err(storage)?;
        let existing: Option<String> = transaction
            .query_row(
                "SELECT body FROM execution_events WHERE task_id=?1 AND idempotency_key=?2",
                params![task_id.as_str(), key.as_str()],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage)?;
        if let Some(body) = existing {
            return serde_json::from_str(&body).map_err(|e| LedgerError::Storage(e.to_string()));
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
        transaction.execute("INSERT INTO execution_events(task_id, revision, event_id, idempotency_key, body) VALUES (?1, ?2, ?3, ?4, ?5)", params![task_id.as_str(), event.stream_revision, event.event_id.as_str(), key.as_str(), body]).map_err(|error| if error.to_string().contains("event_id") { LedgerError::EventIdConflict } else { storage(error) })?;
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
