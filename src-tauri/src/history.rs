use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StoredChatMessage {
    pub(crate) role: String,
    pub(crate) content: String,
}

pub(crate) struct HistoryState {
    connection: Mutex<Connection>,
}

impl HistoryState {
    pub(crate) fn open(path: PathBuf) -> Result<Self, String> {
        if path != PathBuf::from(":memory:") {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Unable to create AutoCoder data directory: {e}"))?;
            }
        }
        let connection = Connection::open(path)
            .map_err(|e| format!("Unable to open AutoCoder history database: {e}"))?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS chat_messages (id INTEGER PRIMARY KEY AUTOINCREMENT, project_path TEXT NOT NULL, role TEXT NOT NULL CHECK(role IN ('user','assistant')), content TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
             CREATE INDEX IF NOT EXISTS chat_messages_project ON chat_messages(project_path,id);
             CREATE TABLE IF NOT EXISTS terminal_commands (id INTEGER PRIMARY KEY AUTOINCREMENT, project_path TEXT NOT NULL, command TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
             CREATE INDEX IF NOT EXISTS terminal_commands_project ON terminal_commands(project_path,id);"
        ).map_err(history_error)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub(crate) fn chat_messages(&self, project: &Path) -> Result<Vec<StoredChatMessage>, String> {
        let connection = self.connection.lock().map_err(|_| lock_error())?;
        let mut statement = connection
            .prepare("SELECT role,content FROM chat_messages WHERE project_path=?1 ORDER BY id")
            .map_err(history_error)?;
        let messages = statement
            .query_map([key(project)], |row| {
                Ok(StoredChatMessage {
                    role: row.get(0)?,
                    content: row.get(1)?,
                })
            })
            .map_err(history_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(history_error)?;
        Ok(messages)
    }

    pub(crate) fn append_chat_exchange(
        &self,
        project: &Path,
        user: &str,
        assistant: &str,
    ) -> Result<(), String> {
        let mut connection = self.connection.lock().map_err(|_| lock_error())?;
        let transaction = connection.transaction().map_err(history_error)?;
        let project = key(project);
        transaction
            .execute(
                "INSERT INTO chat_messages(project_path,role,content) VALUES (?1,'user',?2)",
                params![project, user],
            )
            .map_err(history_error)?;
        transaction
            .execute(
                "INSERT INTO chat_messages(project_path,role,content) VALUES (?1,'assistant',?2)",
                params![project, assistant],
            )
            .map_err(history_error)?;
        transaction.commit().map_err(history_error)
    }

    pub(crate) fn terminal_commands(&self, project: &Path) -> Result<Vec<String>, String> {
        let connection = self.connection.lock().map_err(|_| lock_error())?;
        let mut statement = connection
            .prepare("SELECT command FROM terminal_commands WHERE project_path=?1 ORDER BY id")
            .map_err(history_error)?;
        let commands = statement
            .query_map([key(project)], |row| row.get(0))
            .map_err(history_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(history_error)?;
        Ok(commands)
    }

    pub(crate) fn append_terminal_command(
        &self,
        project: &Path,
        command: &str,
    ) -> Result<(), String> {
        let connection = self.connection.lock().map_err(|_| lock_error())?;
        let project = key(project);
        let previous: Option<String> = connection.query_row("SELECT command FROM terminal_commands WHERE project_path=?1 ORDER BY id DESC LIMIT 1", [&project], |row| row.get(0)).optional().map_err(history_error)?;
        if previous.as_deref() != Some(command) {
            connection
                .execute(
                    "INSERT INTO terminal_commands(project_path,command) VALUES (?1,?2)",
                    params![project, command],
                )
                .map_err(history_error)?;
        }
        Ok(())
    }

    pub(crate) fn clear(&self, project: &Path, kind: &str) -> Result<(), String> {
        let table = match kind {
            "chat" => "chat_messages",
            "terminal" => "terminal_commands",
            _ => return Err("Unknown history kind.".into()),
        };
        self.connection
            .lock()
            .map_err(|_| lock_error())?
            .execute(
                &format!("DELETE FROM {table} WHERE project_path=?1"),
                [key(project)],
            )
            .map_err(history_error)?;
        Ok(())
    }
}

fn key(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
fn history_error(error: rusqlite::Error) -> String {
    format!("Unable to access project history: {error}")
}
fn lock_error() -> String {
    "Unable to access project history.".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> HistoryState {
        HistoryState::open(PathBuf::from(":memory:")).unwrap()
    }

    #[test]
    fn histories_are_persistent_deduplicated_and_project_scoped() {
        let history = state();
        history
            .append_chat_exchange(Path::new("one"), "question", "answer")
            .unwrap();
        history
            .append_terminal_command(Path::new("one"), "cargo test")
            .unwrap();
        history
            .append_terminal_command(Path::new("one"), "cargo test")
            .unwrap();
        assert_eq!(history.chat_messages(Path::new("one")).unwrap().len(), 2);
        assert!(history.chat_messages(Path::new("two")).unwrap().is_empty());
        assert_eq!(
            history.terminal_commands(Path::new("one")).unwrap(),
            ["cargo test"]
        );
    }

    #[test]
    fn histories_clear_independently() {
        let history = state();
        history
            .append_chat_exchange(Path::new("one"), "q", "a")
            .unwrap();
        history
            .append_terminal_command(Path::new("one"), "test")
            .unwrap();
        history.clear(Path::new("one"), "chat").unwrap();
        assert!(history.chat_messages(Path::new("one")).unwrap().is_empty());
        assert_eq!(
            history.terminal_commands(Path::new("one")).unwrap(),
            ["test"]
        );
        assert!(history.clear(Path::new("one"), "invalid").is_err());
    }
}
