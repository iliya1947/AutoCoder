use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::{ChatMessage, TerminalResult};

const CHAT_LIMIT: i64 = 200;
const TERMINAL_LIMIT: i64 = 100;

pub struct HistoryStore(Mutex<Connection>);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredTerminalRun {
    command: String,
    result: TerminalResult,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectHistory {
    chat_messages: Vec<ChatMessage>,
    terminal_runs: Vec<StoredTerminalRun>,
}

impl HistoryStore {
    pub fn open(path: PathBuf) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let connection = Connection::open(path).map_err(|e| e.to_string())?;
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
            CREATE TABLE IF NOT EXISTS chat_messages(project TEXT NOT NULL, position INTEGER NOT NULL, role TEXT NOT NULL, content TEXT NOT NULL, PRIMARY KEY(project, position));
            CREATE TABLE IF NOT EXISTS terminal_runs(id INTEGER PRIMARY KEY, project TEXT NOT NULL, command TEXT NOT NULL, exit_code INTEGER, stdout TEXT NOT NULL, stderr TEXT NOT NULL, cancelled INTEGER NOT NULL, created_at INTEGER NOT NULL DEFAULT(unixepoch()));
            CREATE INDEX IF NOT EXISTS terminal_project_id ON terminal_runs(project, id);")
            .map_err(|e| e.to_string())?;
        Ok(Self(Mutex::new(connection)))
    }

    fn key(root: &Path) -> String {
        root.to_string_lossy().into_owned()
    }

    pub fn load(&self, root: &Path) -> Result<ProjectHistory, String> {
        let connection = self
            .0
            .lock()
            .map_err(|_| "Unable to access history database.".to_string())?;
        let key = Self::key(root);
        let mut chat_statement = connection.prepare("SELECT role, content FROM chat_messages WHERE project=?1 ORDER BY position DESC LIMIT ?2").map_err(|e| e.to_string())?;
        let mut chat_messages: Vec<_> = chat_statement
            .query_map(params![key, CHAT_LIMIT], |row| {
                Ok(ChatMessage {
                    role: row.get(0)?,
                    content: row.get(1)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        chat_messages.reverse();
        let mut terminal_statement = connection.prepare("SELECT command, exit_code, stdout, stderr, cancelled FROM terminal_runs WHERE project=?1 ORDER BY id DESC LIMIT ?2").map_err(|e| e.to_string())?;
        let mut terminal_runs: Vec<_> = terminal_statement
            .query_map(params![key, TERMINAL_LIMIT], |row| {
                Ok(StoredTerminalRun {
                    command: row.get(0)?,
                    result: TerminalResult {
                        exit_code: row.get(1)?,
                        stdout: row.get(2)?,
                        stderr: row.get(3)?,
                        cancelled: row.get::<_, i64>(4)? != 0,
                    },
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        terminal_runs.reverse();
        Ok(ProjectHistory {
            chat_messages,
            terminal_runs,
        })
    }

    pub fn replace_chat(&self, root: &Path, messages: &[ChatMessage]) -> Result<(), String> {
        let mut connection = self
            .0
            .lock()
            .map_err(|_| "Unable to access history database.".to_string())?;
        let transaction = connection.transaction().map_err(|e| e.to_string())?;
        let key = Self::key(root);
        transaction
            .execute("DELETE FROM chat_messages WHERE project=?1", [&key])
            .map_err(|e| e.to_string())?;
        let start = messages.len().saturating_sub(CHAT_LIMIT as usize);
        for (position, message) in messages[start..].iter().enumerate() {
            transaction.execute("INSERT INTO chat_messages(project, position, role, content) VALUES(?1, ?2, ?3, ?4)", params![key, position, message.role, message.content]).map_err(|e| e.to_string())?;
        }
        transaction.commit().map_err(|e| e.to_string())
    }

    pub fn append_terminal(
        &self,
        root: &Path,
        command: &str,
        result: &TerminalResult,
    ) -> Result<(), String> {
        let connection = self
            .0
            .lock()
            .map_err(|_| "Unable to access history database.".to_string())?;
        let key = Self::key(root);
        connection.execute("INSERT INTO terminal_runs(project, command, exit_code, stdout, stderr, cancelled) VALUES(?1, ?2, ?3, ?4, ?5, ?6)", params![key, command, result.exit_code, result.stdout, result.stderr, result.cancelled]).map_err(|e| e.to_string())?;
        connection.execute("DELETE FROM terminal_runs WHERE project=?1 AND id NOT IN (SELECT id FROM terminal_runs WHERE project=?1 ORDER BY id DESC LIMIT ?2)", params![key, TERMINAL_LIMIT]).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn clear(&self, root: &Path, kind: &str) -> Result<(), String> {
        let table = match kind {
            "chat" => "chat_messages",
            "terminal" => "terminal_runs",
            _ => return Err("Unknown history kind.".into()),
        };
        self.0
            .lock()
            .map_err(|_| "Unable to access history database.".to_string())?
            .execute(
                &format!("DELETE FROM {table} WHERE project=?1"),
                [Self::key(root)],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn persists_isolates_and_clears_project_history() {
        let directory = TempDir::new().unwrap();
        let db = directory.path().join("history.db");
        let first = directory.path().join("first");
        let second = directory.path().join("second");
        let store = HistoryStore::open(db.clone()).unwrap();
        store
            .replace_chat(
                &first,
                &[ChatMessage {
                    role: "user".into(),
                    content: "hello".into(),
                }],
            )
            .unwrap();
        store
            .append_terminal(
                &first,
                "cargo test",
                &TerminalResult {
                    exit_code: Some(0),
                    stdout: "ok".into(),
                    stderr: "".into(),
                    cancelled: false,
                },
            )
            .unwrap();
        assert_eq!(store.load(&second).unwrap().chat_messages.len(), 0);
        drop(store);
        let reopened = HistoryStore::open(db).unwrap();
        let loaded = reopened.load(&first).unwrap();
        assert_eq!(loaded.chat_messages[0].content, "hello");
        assert_eq!(loaded.terminal_runs[0].command, "cargo test");
        reopened.clear(&first, "chat").unwrap();
        assert!(reopened.load(&first).unwrap().chat_messages.is_empty());
        assert_eq!(reopened.load(&first).unwrap().terminal_runs.len(), 1);
    }
}
