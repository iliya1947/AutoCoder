use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{ChatMessage, TerminalResult};

const CHAT_LIMIT: i64 = 200;
const TERMINAL_LIMIT: i64 = 100;

pub struct HistoryStore(Mutex<Connection>);

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct WorkspaceState {
    pub project_root: String,
    pub open_file: Option<String>,
}

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
    orchestration_task: Option<serde_json::Value>,
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
            CREATE INDEX IF NOT EXISTS terminal_project_id ON terminal_runs(project, id);
            CREATE TABLE IF NOT EXISTS workspace_state(id INTEGER PRIMARY KEY CHECK(id=1), project_root TEXT NOT NULL, open_file TEXT);
            CREATE TABLE IF NOT EXISTS orchestration_tasks(project TEXT PRIMARY KEY, task TEXT NOT NULL, updated_at INTEGER NOT NULL DEFAULT(unixepoch()));
            CREATE TABLE IF NOT EXISTS orchestration_task_history(project TEXT NOT NULL, task_id TEXT NOT NULL, task TEXT NOT NULL, updated_at INTEGER NOT NULL DEFAULT(unixepoch()), PRIMARY KEY(project, task_id));")
            .map_err(|e| e.to_string())?;
        Ok(Self(Mutex::new(connection)))
    }

    fn key(root: &Path) -> String {
        root.to_string_lossy().into_owned()
    }

    pub fn workspace(&self) -> Result<Option<WorkspaceState>, String> {
        let connection = self
            .0
            .lock()
            .map_err(|_| "Unable to access history database.".to_string())?;
        let mut statement = connection
            .prepare("SELECT project_root, open_file FROM workspace_state WHERE id=1")
            .map_err(|e| e.to_string())?;
        let mut rows = statement.query([]).map_err(|e| e.to_string())?;
        rows.next()
            .map_err(|e| e.to_string())?
            .map(|row| {
                Ok(WorkspaceState {
                    project_root: row.get(0).map_err(|e| e.to_string())?,
                    open_file: row.get(1).map_err(|e| e.to_string())?,
                })
            })
            .transpose()
    }

    pub fn remember_project(&self, root: &Path) -> Result<(), String> {
        self.0
            .lock()
            .map_err(|_| "Unable to access history database.".to_string())?
            .execute(
                "INSERT INTO workspace_state(id, project_root, open_file) VALUES(1, ?1, NULL) ON CONFLICT(id) DO UPDATE SET open_file=CASE WHEN workspace_state.project_root=excluded.project_root THEN workspace_state.open_file ELSE NULL END, project_root=excluded.project_root",
                [Self::key(root)],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn remember_file(&self, root: &Path, relative_path: &str) -> Result<(), String> {
        let updated = self
            .0
            .lock()
            .map_err(|_| "Unable to access history database.".to_string())?
            .execute(
                "UPDATE workspace_state SET open_file=?1 WHERE id=1 AND project_root=?2",
                params![relative_path, Self::key(root)],
            )
            .map_err(|e| e.to_string())?;
        if updated == 1 {
            Ok(())
        } else {
            Err("The file belongs to a previous project session.".into())
        }
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
        let orchestration_task = connection
            .query_row(
                "SELECT task FROM orchestration_tasks WHERE project=?1",
                [&key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .map(|task| serde_json::from_str(&task).map_err(|e| e.to_string()))
            .transpose()?;
        Ok(ProjectHistory {
            chat_messages,
            terminal_runs,
            orchestration_task,
        })
    }

    pub fn save_orchestration_task(
        &self,
        root: &Path,
        task: Option<&serde_json::Value>,
    ) -> Result<(), String> {
        let connection = self
            .0
            .lock()
            .map_err(|_| "Unable to access history database.".to_string())?;
        let key = Self::key(root);
        match task {
            Some(value) => {
                let json = serde_json::to_string(value).map_err(|e| e.to_string())?;
                let task_id = task_id(value)?;
                let transaction = connection
                    .unchecked_transaction()
                    .map_err(|e| e.to_string())?;
                if stopped_task_would_be_replaced(&transaction, &key, value)? {
                    return Ok(());
                }
                transaction.execute("INSERT INTO orchestration_task_history(project, task_id, task, updated_at) VALUES(?1, ?2, ?3, unixepoch()) ON CONFLICT(project, task_id) DO UPDATE SET task=excluded.task, updated_at=excluded.updated_at", params![key, task_id, json]).map_err(|e| e.to_string())?;
                transaction.execute("INSERT INTO orchestration_tasks(project, task, updated_at) VALUES(?1, ?2, unixepoch()) ON CONFLICT(project) DO UPDATE SET task=excluded.task, updated_at=excluded.updated_at", params![key, json]).map_err(|e| e.to_string())?;
                transaction.commit().map_err(|e| e.to_string())?;
            }
            None => {
                connection
                    .execute("DELETE FROM orchestration_tasks WHERE project=?1", [key])
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }

    pub fn append_chat_exchange(
        &self,
        root: &Path,
        user: &ChatMessage,
        assistant: &ChatMessage,
    ) -> Result<(), String> {
        self.append_chat_exchange_transaction(root, user, assistant, None, false)
    }

    pub fn append_chat_exchange_and_task(
        &self,
        root: &Path,
        user: &ChatMessage,
        assistant: &ChatMessage,
        task: Option<&serde_json::Value>,
    ) -> Result<(), String> {
        self.append_chat_exchange_transaction(root, user, assistant, task, true)
    }

    fn append_chat_exchange_transaction(
        &self,
        root: &Path,
        user: &ChatMessage,
        assistant: &ChatMessage,
        task: Option<&serde_json::Value>,
        update_task: bool,
    ) -> Result<(), String> {
        let mut connection = self
            .0
            .lock()
            .map_err(|_| "Unable to access history database.".to_string())?;
        let transaction = connection.transaction().map_err(|e| e.to_string())?;
        let key = Self::key(root);
        if user.role != "user" || assistant.role != "assistant" {
            return Err("A chat exchange must contain a user and assistant message.".into());
        }
        if let Some(value) = task {
            if stopped_task_would_be_replaced(&transaction, &key, value)? {
                return Ok(());
            }
        }
        let next_position: i64 = transaction
            .query_row(
                "SELECT COALESCE(MAX(position) + 1, 0) FROM chat_messages WHERE project=?1",
                [&key],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        for (offset, message) in [user, assistant].into_iter().enumerate() {
            transaction.execute("INSERT INTO chat_messages(project, position, role, content) VALUES(?1, ?2, ?3, ?4)", params![key, next_position + offset as i64, message.role, message.content]).map_err(|e| e.to_string())?;
        }
        transaction.execute("DELETE FROM chat_messages WHERE project=?1 AND position NOT IN (SELECT position FROM chat_messages WHERE project=?1 ORDER BY position DESC LIMIT ?2)", params![key, CHAT_LIMIT]).map_err(|e| e.to_string())?;
        match (update_task, task) {
            (true, Some(value)) => {
                let json = serde_json::to_string(value).map_err(|e| e.to_string())?;
                let task_id = task_id(value)?;
                transaction.execute("INSERT INTO orchestration_task_history(project, task_id, task, updated_at) VALUES(?1, ?2, ?3, unixepoch()) ON CONFLICT(project, task_id) DO UPDATE SET task=excluded.task, updated_at=excluded.updated_at", params![key, task_id, json]).map_err(|e| e.to_string())?;
                transaction.execute("INSERT INTO orchestration_tasks(project, task, updated_at) VALUES(?1, ?2, unixepoch()) ON CONFLICT(project) DO UPDATE SET task=excluded.task, updated_at=excluded.updated_at", params![key, json]).map_err(|e| e.to_string())?;
            }
            (true, None) => {
                transaction
                    .execute("DELETE FROM orchestration_tasks WHERE project=?1", [&key])
                    .map_err(|e| e.to_string())?;
            }
            (false, _) => {}
        }
        transaction.commit().map_err(|e| e.to_string())
    }

    pub fn append_terminal(
        &self,
        root: &Path,
        command: &str,
        result: &TerminalResult,
    ) -> Result<(), String> {
        let mut connection = self
            .0
            .lock()
            .map_err(|_| "Unable to access history database.".to_string())?;
        let key = Self::key(root);
        let transaction = connection.transaction().map_err(|e| e.to_string())?;
        transaction.execute("INSERT INTO terminal_runs(project, command, exit_code, stdout, stderr, cancelled) VALUES(?1, ?2, ?3, ?4, ?5, ?6)", params![key, command, result.exit_code, result.stdout, result.stderr, result.cancelled]).map_err(|e| e.to_string())?;
        transaction.execute("DELETE FROM terminal_runs WHERE project=?1 AND id NOT IN (SELECT id FROM terminal_runs WHERE project=?1 ORDER BY id DESC LIMIT ?2)", params![key, TERMINAL_LIMIT]).map_err(|e| e.to_string())?;
        transaction.commit().map_err(|e| e.to_string())
    }

    pub fn clear(&self, root: &Path, kind: &str) -> Result<(), String> {
        if kind == "orchestration" {
            let mut connection = self
                .0
                .lock()
                .map_err(|_| "Unable to access history database.".to_string())?;
            let transaction = connection.transaction().map_err(|e| e.to_string())?;
            let key = Self::key(root);
            transaction
                .execute("DELETE FROM orchestration_tasks WHERE project=?1", [&key])
                .map_err(|e| e.to_string())?;
            transaction
                .execute(
                    "DELETE FROM orchestration_task_history WHERE project=?1",
                    [&key],
                )
                .map_err(|e| e.to_string())?;
            return transaction.commit().map_err(|e| e.to_string());
        }
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

fn stopped_task_would_be_replaced(
    connection: &Connection,
    project: &str,
    incoming: &serde_json::Value,
) -> Result<bool, String> {
    let incoming_id = task_id(incoming)?;
    if incoming.get("status").and_then(serde_json::Value::as_str) == Some("stopped") {
        return Ok(false);
    }
    let current = connection
        .query_row(
            "SELECT task FROM orchestration_tasks WHERE project=?1",
            [project],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some(current) = current else {
        return Ok(false);
    };
    let current: serde_json::Value =
        serde_json::from_str(&current).map_err(|error| error.to_string())?;
    Ok(task_id(&current)? == incoming_id
        && current.get("status").and_then(serde_json::Value::as_str) == Some("stopped"))
}

fn task_id(task: &serde_json::Value) -> Result<&str, String> {
    task.get("id")
        .and_then(serde_json::Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| "An orchestration task must have a non-empty id.".to_string())
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
            .append_chat_exchange(
                &first,
                &ChatMessage {
                    role: "user".into(),
                    content: "hello".into(),
                },
                &ChatMessage {
                    role: "assistant".into(),
                    content: "hi".into(),
                },
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
        assert_eq!(loaded.chat_messages[1].content, "hi");
        assert_eq!(loaded.terminal_runs[0].command, "cargo test");
        reopened.clear(&first, "chat").unwrap();
        assert!(reopened.load(&first).unwrap().chat_messages.is_empty());
        assert_eq!(reopened.load(&first).unwrap().terminal_runs.len(), 1);
    }

    #[test]
    fn persists_one_workspace_and_resets_file_when_project_changes() {
        let directory = TempDir::new().unwrap();
        let db = directory.path().join("history.db");
        let first = directory.path().join("first");
        let second = directory.path().join("second");
        let store = HistoryStore::open(db.clone()).unwrap();

        assert_eq!(store.workspace().unwrap(), None);
        store.remember_project(&first).unwrap();
        store.remember_file(&first, "src/main.rs").unwrap();
        assert_eq!(
            store.remember_file(&second, "ignored.txt").unwrap_err(),
            "The file belongs to a previous project session."
        );
        store.remember_project(&first).unwrap();
        assert_eq!(
            store.workspace().unwrap(),
            Some(WorkspaceState {
                project_root: first.to_string_lossy().into_owned(),
                open_file: Some("src/main.rs".into()),
            })
        );

        drop(store);
        let reopened = HistoryStore::open(db).unwrap();
        assert_eq!(
            reopened.workspace().unwrap().unwrap().open_file.as_deref(),
            Some("src/main.rs")
        );
        reopened.remember_project(&second).unwrap();
        assert_eq!(reopened.workspace().unwrap().unwrap().open_file, None);
    }

    #[test]
    fn appends_chat_as_an_atomic_pair_and_enforces_message_limit() {
        let directory = TempDir::new().unwrap();
        let store = HistoryStore::open(directory.path().join("history.db")).unwrap();
        let project = directory.path().join("project");

        for index in 0..101 {
            store
                .append_chat_exchange(
                    &project,
                    &ChatMessage {
                        role: "user".into(),
                        content: format!("question {index}"),
                    },
                    &ChatMessage {
                        role: "assistant".into(),
                        content: format!("answer {index}"),
                    },
                )
                .unwrap();
        }

        let messages = store.load(&project).unwrap().chat_messages;
        assert_eq!(messages.len(), CHAT_LIMIT as usize);
        assert_eq!(messages.first().unwrap().content, "question 1");
        assert_eq!(messages.last().unwrap().content, "answer 100");
        assert!(store
            .append_chat_exchange(
                &project,
                &ChatMessage {
                    role: "assistant".into(),
                    content: "wrong".into(),
                },
                &ChatMessage {
                    role: "assistant".into(),
                    content: "answer".into(),
                },
            )
            .is_err());
        assert_eq!(store.load(&project).unwrap().chat_messages.len(), 200);
    }

    #[test]
    fn terminal_limit_keeps_complete_transcripts() {
        let directory = TempDir::new().unwrap();
        let store = HistoryStore::open(directory.path().join("history.db")).unwrap();
        let project = directory.path().join("project");
        for index in 0..101 {
            store
                .append_terminal(
                    &project,
                    &format!("command {index}"),
                    &TerminalResult {
                        exit_code: Some(index),
                        stdout: format!("stdout {index}"),
                        stderr: format!("stderr {index}"),
                        cancelled: index == 100,
                    },
                )
                .unwrap();
        }
        let runs = store.load(&project).unwrap().terminal_runs;
        assert_eq!(runs.len(), TERMINAL_LIMIT as usize);
        assert_eq!(runs.first().unwrap().command, "command 1");
        let latest = runs.last().unwrap();
        assert_eq!(latest.result.exit_code, Some(100));
        assert_eq!(latest.result.stdout, "stdout 100");
        assert_eq!(latest.result.stderr, "stderr 100");
        assert!(latest.result.cancelled);
    }

    #[test]
    fn persists_the_task_state_machine_per_project() {
        let directory = TempDir::new().unwrap();
        let db = directory.path().join("history.db");
        let first = directory.path().join("first");
        let second = directory.path().join("second");
        let store = HistoryStore::open(db.clone()).unwrap();
        let task = serde_json::json!({"id":"task-1","status":"awaiting_approval","goal":"fix it","nextSequence":2,"actions":[]});

        store
            .append_chat_exchange_and_task(
                &first,
                &ChatMessage {
                    role: "user".into(),
                    content: "fix".into(),
                },
                &ChatMessage {
                    role: "assistant".into(),
                    content: "review this".into(),
                },
                Some(&task),
            )
            .unwrap();
        assert_eq!(store.load(&first).unwrap().orchestration_task, Some(task));
        assert_eq!(store.load(&first).unwrap().chat_messages.len(), 2);
        assert_eq!(store.load(&second).unwrap().orchestration_task, None);
        let stopped = serde_json::json!({"id":"task-1","status":"stopped","goal":"fix it","nextSequence":2,"actions":[],"conclusion":{"outcome":"stopped","reason":"The task was stopped by the user."}});
        store
            .save_orchestration_task(&first, Some(&stopped))
            .unwrap();
        let late = serde_json::json!({"id":"task-1","status":"awaiting_approval","goal":"fix it","nextSequence":3,"actions":[]});
        store.save_orchestration_task(&first, Some(&late)).unwrap();
        store
            .append_chat_exchange_and_task(
                &first,
                &ChatMessage {
                    role: "user".into(),
                    content: "late result".into(),
                },
                &ChatMessage {
                    role: "assistant".into(),
                    content: "must be discarded".into(),
                },
                Some(&late),
            )
            .unwrap();
        assert_eq!(
            store.load(&first).unwrap().orchestration_task,
            Some(stopped.clone())
        );
        assert_eq!(store.load(&first).unwrap().chat_messages.len(), 2);
        let replacement = serde_json::json!({"id":"task-2","status":"thinking","goal":"new task","nextSequence":1,"actions":[]});
        store
            .save_orchestration_task(&first, Some(&replacement))
            .unwrap();
        let archived: String = store
            .0
            .lock()
            .unwrap()
            .query_row(
                "SELECT task FROM orchestration_task_history WHERE project=?1 AND task_id='task-1'",
                [HistoryStore::key(&first)],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&archived).unwrap(),
            stopped
        );
        assert_eq!(
            store.load(&first).unwrap().orchestration_task,
            Some(replacement)
        );
        drop(store);

        let reopened = HistoryStore::open(db).unwrap();
        assert!(reopened.load(&first).unwrap().orchestration_task.is_some());
        reopened.save_orchestration_task(&first, None).unwrap();
        assert!(reopened.load(&first).unwrap().orchestration_task.is_none());
    }
}
