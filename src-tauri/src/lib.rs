use std::{
    fs,
    fs::OpenOptions,
    io::{BufRead, BufReader, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

mod history;
mod process_lifecycle;
use history::{HistoryStore, ProjectHistory};
use process_lifecycle::{ChildIo, OwnedChild, ProcessLifecycle};

use serde::{Deserialize, Serialize};
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;

const EXCLUDED_DIRECTORY_NAMES: &[&str] = &[
    ".git",
    ".idea",
    ".venv",
    ".vscode",
    "dist",
    "node_modules",
    "target",
];
#[derive(Default)]
struct ProjectState {
    root: Mutex<Option<PathBuf>>,
}

#[derive(Default)]
struct TerminalState {
    project_transition: Mutex<()>,
    active_cancel: Mutex<Option<Arc<AtomicBool>>>,
}

impl TerminalState {
    fn begin_project_command(
        &self,
        project_state: &ProjectState,
    ) -> Result<(PathBuf, Arc<AtomicBool>), String> {
        let _transition = self
            .project_transition
            .lock()
            .map_err(|_| "Unable to access terminal command state.".to_string())?;
        let root = project_state
            .root
            .lock()
            .map_err(|_| "Unable to access the project state.".to_string())?
            .clone()
            .ok_or_else(|| "Open a project before running a terminal command.".to_string())?;
        let cancel = Arc::new(AtomicBool::new(false));
        let mut active = self
            .active_cancel
            .lock()
            .map_err(|_| "Unable to access terminal command state.".to_string())?;
        if active.is_some() {
            return Err("Another terminal command is already running.".to_string());
        }
        *active = Some(Arc::clone(&cancel));
        Ok((root, cancel))
    }

    fn switch_project(&self, project_state: &ProjectState, root: PathBuf) -> Result<bool, String> {
        let _transition = self
            .project_transition
            .lock()
            .map_err(|_| "Unable to access terminal command state.".to_string())?;
        let mut current_root = project_state
            .root
            .lock()
            .map_err(|_| "Unable to access the project state.".to_string())?;
        if current_root.as_ref() == Some(&root) {
            return Ok(false);
        }
        if self
            .active_cancel
            .lock()
            .map_err(|_| "Unable to access terminal command state.".to_string())?
            .is_some()
        {
            return Err(
                "Cancel or wait for the active terminal command before switching projects."
                    .to_string(),
            );
        }
        *current_root = Some(root);
        Ok(true)
    }
}

struct OllamaState {
    owned: Mutex<Option<OwnedChild>>,
    lifecycle: Arc<ProcessLifecycle>,
}

impl OllamaState {
    fn new(lifecycle: Arc<ProcessLifecycle>) -> Self {
        Self {
            owned: Mutex::new(None),
            lifecycle,
        }
    }
}

impl OllamaState {
    fn ensure_running(&self) -> Result<(), String> {
        match ollama_api_readiness("127.0.0.1:11434", std::time::Duration::from_millis(750)) {
            OllamaReadiness::Ready => return Ok(()),
            OllamaReadiness::HttpStatus(status) => {
                return Err(format!(
                    "Ollama readiness endpoint /api/version returned HTTP {status}."
                ));
            }
            OllamaReadiness::Unavailable => {}
        }
        self.ensure_running_with(
            ollama_is_ready,
            || launch_ollama(&self.lifecycle),
            || thread::sleep(std::time::Duration::from_millis(250)),
        )
    }

    fn ensure_running_with<R, L, S>(
        &self,
        mut ready: R,
        launch: L,
        mut sleep: S,
    ) -> Result<(), String>
    where
        R: FnMut() -> bool,
        L: FnOnce() -> Result<OwnedChild, String>,
        S: FnMut(),
    {
        let mut owned = self
            .owned
            .lock()
            .map_err(|_| "Unable to access Ollama state.".to_string())?;
        if ready() {
            return Ok(());
        }
        if let Some(process) = owned.as_mut() {
            if process
                .try_wait()
                .map_err(|error| error.to_string())?
                .is_none()
            {
                return Err(
                    "The AutoCoder-owned Ollama process is running, but its API is unavailable."
                        .into(),
                );
            }
            *owned = None;
        }

        let mut process = launch()?;
        for _ in 0..80 {
            if ready() {
                *owned = Some(process);
                return Ok(());
            }
            if let Some(status) = process.try_wait().map_err(|error| error.to_string())? {
                return Err(format!(
                    "Ollama exited before its API was ready ({status})."
                ));
            }
            sleep();
        }
        let _ = process.kill();
        let _ = process.wait();
        Err("Timed out after 20 seconds waiting for local Ollama.".into())
    }

    fn shutdown(&self) {
        let Ok(mut owned) = self.owned.lock() else {
            return;
        };
        if let Some(mut process) = owned.take() {
            match process.try_wait() {
                Ok(Some(_)) => {}
                Ok(None) => {
                    let _ = process.kill();
                    let _ = process.wait();
                }
                Err(error) => {
                    eprintln!("Unable to inspect the AutoCoder-owned Ollama process: {error}")
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Ord, PartialOrd, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum FileTreeNodeKind {
    Directory,
    File,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FileTreeNode {
    name: String,
    path: String,
    kind: FileTreeNodeKind,
    children: Vec<FileTreeNode>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectTree {
    name: String,
    children: Vec<FileTreeNode>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenProjectResult {
    project: ProjectTree,
    session_changed: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RefreshProjectResult {
    project: ProjectTree,
    open_file_content: Option<String>,
}

#[derive(Serialize)]
struct FileReadResult {
    content: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct TerminalResult {
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    cancelled: bool,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupMetadata {
    created_at_unix_ms: u128,
    original_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupEntry {
    id: String,
    created_at_unix_ms: u128,
    relative_path: String,
    content: String,
    current_content: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatMessage {
    role: String,
    content: String,
}

#[tauri::command]
fn load_project_history(
    project_state: State<'_, ProjectState>,
    history: State<'_, HistoryStore>,
) -> Result<ProjectHistory, String> {
    history.load(&project_root(&project_state)?)
}

#[tauri::command]
fn save_chat_exchange(
    project_key: String,
    user_message: ChatMessage,
    assistant_message: ChatMessage,
    project_state: State<'_, ProjectState>,
    history: State<'_, HistoryStore>,
) -> Result<(), String> {
    let current_root = project_root(&project_state)?;
    if current_root.to_string_lossy() != project_key {
        return Err("The chat response belongs to a previous project session.".into());
    }
    history.append_chat_exchange(&current_root, &user_message, &assistant_message)
}

#[tauri::command]
fn clear_project_history(
    kind: String,
    project_state: State<'_, ProjectState>,
    history: State<'_, HistoryStore>,
) -> Result<(), String> {
    history.clear(&project_root(&project_state)?, &kind)
}

#[derive(Deserialize, Serialize)]
struct ChatRequest {
    messages: Vec<ChatMessage>,
    context: Option<ChatContext>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatContext {
    open_file: Option<OpenFileContext>,
    selection: Option<SelectionContext>,
    project: Option<ProjectContext>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenFileContext {
    path: String,
    content: String,
    saved_content: String,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
enum SelectionContext {
    Active { path: String, content: String },
    None,
}

#[derive(Deserialize, Serialize)]
struct ProjectContext {
    name: String,
    entries: Vec<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatResponse {
    message: ChatMessage,
    proposal: Option<FileProposal>,
    command_proposal: Option<TerminalProposal>,
    #[serde(default)]
    project_key: String,
}

#[derive(Deserialize, Serialize)]
struct TerminalProposal {
    command: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct FileProposal {
    operation: String,
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    original_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_saved_content: Option<String>,
}

#[tauri::command]
async fn open_project(
    app: tauri::AppHandle,
    project_state: State<'_, ProjectState>,
    terminal_state: State<'_, TerminalState>,
) -> Result<Option<OpenProjectResult>, String> {
    let Some(root) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    let root = root.into_path().map_err(|error| error.to_string())?;
    let root = fs::canonicalize(root).map_err(|error| error.to_string())?;

    let metadata = fs::metadata(&root).map_err(|error| error.to_string())?;
    if !metadata.is_dir() {
        return Err("The selected path is not a directory.".to_string());
    }
    fs::read_dir(&root).map_err(|error| error.to_string())?;

    let project = project_tree(&root)?;

    // Starting a command and committing a project switch share one transition
    // lock, so neither can observe the other's half-completed state.
    let session_changed = terminal_state.switch_project(&project_state, root)?;

    Ok(Some(OpenProjectResult {
        project,
        session_changed,
    }))
}

#[tauri::command]
fn refresh_project(
    open_file_path: Option<String>,
    project_state: State<'_, ProjectState>,
) -> Result<RefreshProjectResult, String> {
    let root = project_root(&project_state)?;
    refresh_project_state(&root, open_file_path.as_deref())
}

fn refresh_project_state(
    root: &Path,
    open_file_path: Option<&str>,
) -> Result<RefreshProjectResult, String> {
    let project = project_tree(root)?;
    let open_file_content = match open_file_path {
        Some(path) if project_contains_file(&project.children, path) => {
            Some(read_file(root, path)?)
        }
        _ => None,
    };
    Ok(RefreshProjectResult {
        project,
        open_file_content,
    })
}

fn project_contains_file(nodes: &[FileTreeNode], path: &str) -> bool {
    nodes.iter().any(|node| {
        (node.kind == FileTreeNodeKind::File && node.path == path)
            || (node.kind == FileTreeNodeKind::Directory
                && project_contains_file(&node.children, path))
    })
}

#[tauri::command]
fn send_chat_message(
    app: tauri::AppHandle,
    ollama: State<'_, OllamaState>,
    project_state: State<'_, ProjectState>,
    request: ChatRequest,
) -> Result<ChatResponse, String> {
    if request.messages.is_empty()
        || request
            .messages
            .iter()
            .any(|message| message.content.trim().is_empty())
    {
        return Err("Chat messages cannot be empty.".to_string());
    }

    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|error| format!("Unable to locate the AutoCoder resources: {error}"))?;
    if uses_managed_local_ollama() {
        ollama.ensure_running()?;
    }
    let root = project_root(&project_state)?;
    let mut response = run_chat_backend(&resource_dir, &request, &ollama.lifecycle)?;
    response.project_key = root.to_string_lossy().into_owned();
    Ok(response)
}

fn uses_managed_local_ollama() -> bool {
    let url = std::env::var("AUTOCODER_OLLAMA_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:11434/api/chat".into());
    is_managed_local_ollama_url(&url)
}

fn is_managed_local_ollama_url(url: &str) -> bool {
    let Ok(url) = tauri::Url::parse(url) else {
        return false;
    };
    matches!(url.scheme(), "http" | "https")
        && matches!(
            url.host_str(),
            Some("127.0.0.1" | "localhost" | "::1" | "[::1]")
        )
}

fn ollama_is_ready() -> bool {
    ollama_api_is_ready("127.0.0.1:11434", std::time::Duration::from_millis(750))
}

#[derive(Debug, PartialEq, Eq)]
enum OllamaReadiness {
    Ready,
    HttpStatus(u16),
    Unavailable,
}

fn ollama_api_is_ready(address: &str, timeout: std::time::Duration) -> bool {
    ollama_api_readiness(address, timeout) == OllamaReadiness::Ready
}

fn ollama_api_readiness(address: &str, timeout: std::time::Duration) -> OllamaReadiness {
    let Ok(mut stream) = TcpStream::connect_timeout(
        &match address.parse() {
            Ok(value) => value,
            Err(_) => return OllamaReadiness::Unavailable,
        },
        timeout,
    ) else {
        return OllamaReadiness::Unavailable;
    };
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));
    if stream
        .write_all(b"GET /api/version HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .is_err()
    {
        return OllamaReadiness::Unavailable;
    }
    let mut response = Vec::new();
    if std::io::Read::read_to_end(&mut stream, &mut response).is_err() {
        return OllamaReadiness::Unavailable;
    }
    let Some(split) = response.windows(4).position(|bytes| bytes == b"\r\n\r\n") else {
        return OllamaReadiness::Unavailable;
    };
    let Ok(headers) = std::str::from_utf8(&response[..split]) else {
        return OllamaReadiness::Unavailable;
    };
    let status = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse::<u16>().ok());
    if status != Some(200) {
        return status
            .map(OllamaReadiness::HttpStatus)
            .unwrap_or(OllamaReadiness::Unavailable);
    }
    if serde_json::from_slice::<serde_json::Value>(&response[split + 4..])
        .ok()
        .is_some_and(|json| {
            json.get("version")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|version| !version.is_empty())
        })
    {
        OllamaReadiness::Ready
    } else {
        OllamaReadiness::Unavailable
    }
}

fn ollama_executable() -> Option<PathBuf> {
    if cfg!(windows) {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            let installed = PathBuf::from(local)
                .join("Programs")
                .join("Ollama")
                .join("ollama.exe");
            if installed.is_file() {
                return Some(installed);
            }
        }
    }
    let name = if cfg!(windows) {
        "ollama.exe"
    } else {
        "ollama"
    };
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|path| path.join(name))
            .find(|path| path.is_file())
    })
}

fn launch_ollama(lifecycle: &ProcessLifecycle) -> Result<OwnedChild, String> {
    let executable = ollama_executable().ok_or_else(|| {
        "Local Ollama was not found. Install Ollama; automatic downloads are disabled.".to_string()
    })?;
    let mut command = Command::new(&executable);
    command
        .arg("serve")
        .current_dir(executable.parent().unwrap_or(Path::new(".")))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = lifecycle
        .spawn(&mut command, ChildIo::Ollama)
        .map_err(|error| {
            format!(
                "Failed to start Ollama at '{}': {error}",
                executable.display()
            )
        })?;
    if let Some(stderr) = child.stderr.take() {
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                eprintln!("Ollama: {line}");
            }
        });
    }
    Ok(child)
}

#[cfg(any(windows, test))]
fn windows_creation_flags(production: bool, windows: bool, show_override: bool) -> u32 {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    if production && windows && !show_override {
        CREATE_NO_WINDOW
    } else {
        0
    }
}

fn configure_child_process(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let show_override =
            std::env::var_os("AUTOCODER_SHOW_CHILD_CONSOLES").is_some_and(|value| value != "0");
        command.creation_flags(windows_creation_flags(
            !cfg!(debug_assertions),
            true,
            show_override,
        ));
    }
    #[cfg(not(windows))]
    let _ = command;
}

fn backend_paths(resource_dir: &Path, python_override: Option<PathBuf>) -> (PathBuf, PathBuf) {
    let backend = resource_dir.join("backend").join("main.py");
    let python = python_override.unwrap_or_else(|| {
        if cfg!(windows) {
            resource_dir.join("python-runtime").join("python.exe")
        } else {
            PathBuf::from("python3")
        }
    });
    (python, backend)
}

fn run_chat_backend(
    resource_dir: &Path,
    request: &ChatRequest,
    lifecycle: &ProcessLifecycle,
) -> Result<ChatResponse, String> {
    let python_override = std::env::var_os("AUTOCODER_PYTHON")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let (python, backend) = backend_paths(resource_dir, python_override);
    if !backend.is_file() {
        return Err(format!(
            "The bundled AI backend script is missing: {}",
            backend.display()
        ));
    }
    if python.components().count() > 1 && !python.is_file() {
        return Err(format!(
            "The Python runtime for the AI backend is missing: {}. Reinstall AutoCoder or set AUTOCODER_PYTHON for development diagnostics.",
            python.display()
        ));
    }

    let mut command = Command::new(&python);
    command
        .arg(backend)
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if uses_managed_local_ollama() {
        command.env("AUTOCODER_OLLAMA_MANAGED", "1");
    }
    let mut child = lifecycle
        .spawn(&mut command, ChildIo::PythonBridge)
        .map_err(|error| {
            format!(
                "Unable to start the AI backend with Python runtime '{}': {error}",
                python.display()
            )
        })?;
    serde_json::to_writer(
        child
            .stdin
            .as_mut()
            .ok_or("Unable to open backend input.")?,
        request,
    )
    .map_err(|error| error.to_string())?;
    child
        .stdin
        .as_mut()
        .ok_or("Unable to open backend input.")?
        .flush()
        .map_err(|error| error.to_string())?;
    drop(child.stdin.take());

    let output = child
        .wait_with_output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let error = if error.is_empty() {
            "The AI backend failed.".to_string()
        } else {
            error
        };
        eprintln!(
            "AutoCoder chat backend failed with status {}: {}",
            output.status, error
        );
        return Err(error);
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("The AI backend returned an invalid response: {error}"))
}

#[tauri::command]
fn read_project_file(
    relative_path: String,
    project_state: State<'_, ProjectState>,
) -> Result<FileReadResult, String> {
    let root = project_root(&project_state)?;
    let content = read_file(&root, &relative_path)?;
    Ok(FileReadResult { content })
}

#[tauri::command]
fn save_project_file(
    app: tauri::AppHandle,
    relative_path: String,
    content: String,
    expected_content: String,
    project_state: State<'_, ProjectState>,
) -> Result<(), String> {
    let root = project_root(&project_state)?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let backup_root = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("backups");
    save_open_file(
        &root,
        &relative_path,
        &content,
        expected_content.as_bytes(),
        &backup_root,
        timestamp,
    )
}

#[tauri::command]
fn create_project_file(
    relative_path: String,
    content: String,
    project_state: State<'_, ProjectState>,
) -> Result<ProjectTree, String> {
    let root = project_root(&project_state)?;
    create_file(&root, &relative_path, &content)?;
    let name = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Project".to_string());
    Ok(ProjectTree {
        name,
        children: read_directory(&root, &root),
    })
}

#[tauri::command]
fn delete_project_file(
    app: tauri::AppHandle,
    relative_path: String,
    expected_content: String,
    project_state: State<'_, ProjectState>,
) -> Result<ProjectTree, String> {
    let root = project_root(&project_state)?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let backup_root = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("backups");
    delete_file(
        &root,
        &relative_path,
        expected_content.as_bytes(),
        &backup_root,
        timestamp,
    )?;
    let name = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Project".to_string());
    Ok(ProjectTree {
        name,
        children: read_directory(&root, &root),
    })
}

#[tauri::command]
fn list_project_backups(
    app: tauri::AppHandle,
    project_state: State<'_, ProjectState>,
) -> Result<Vec<BackupEntry>, String> {
    let root = project_root(&project_state)?;
    let backup_root = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("backups");
    list_backups(&root, &backup_root)
}

#[tauri::command]
fn restore_project_backup(
    app: tauri::AppHandle,
    backup_id: String,
    expected_current_content: Option<String>,
    project_state: State<'_, ProjectState>,
) -> Result<ProjectTree, String> {
    let root = project_root(&project_state)?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let backup_root = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("backups");
    restore_backup(
        &root,
        &backup_root,
        &backup_id,
        expected_current_content.as_deref(),
        timestamp,
    )?;
    let name = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Project".to_string());
    Ok(ProjectTree {
        name,
        children: read_directory(&root, &root),
    })
}

#[tauri::command]
async fn execute_project_command(
    command: String,
    project_state: State<'_, ProjectState>,
    lifecycle: State<'_, Arc<ProcessLifecycle>>,
    terminal_state: State<'_, TerminalState>,
    history: State<'_, HistoryStore>,
) -> Result<TerminalResult, String> {
    let (root, cancel) = terminal_state.begin_project_command(&project_state)?;
    let lifecycle = Arc::clone(lifecycle.inner());
    let task_root = root.clone();
    let task_command = command.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        run_project_command(&task_root, &task_command, &lifecycle, &cancel)
    })
    .await
    .map_err(|error| format!("Unable to join command task: {error}"));
    if let Ok(mut active) = terminal_state.active_cancel.lock() {
        *active = None;
    }
    let completed = result??;
    history.append_terminal(&root, &command, &completed)?;
    Ok(completed)
}

#[tauri::command]
fn cancel_project_command(terminal_state: State<'_, TerminalState>) -> Result<bool, String> {
    let active = terminal_state
        .active_cancel
        .lock()
        .map_err(|_| "Unable to access terminal command state.".to_string())?;
    if let Some(cancel) = active.as_ref() {
        cancel.store(true, Ordering::Release);
        Ok(true)
    } else {
        Ok(false)
    }
}

fn run_project_command(
    root: &Path,
    command: &str,
    lifecycle: &ProcessLifecycle,
    cancel: &AtomicBool,
) -> Result<TerminalResult, String> {
    let command = command.trim();
    if command.is_empty() {
        return Err("The command cannot be empty.".to_string());
    }
    let root = fs::canonicalize(root).map_err(|error| error.to_string())?;
    if !root.is_dir() {
        return Err("The project root is not a directory.".to_string());
    }

    #[cfg(windows)]
    let mut process = {
        let mut process = Command::new("cmd.exe");
        // /U makes cmd.exe's own redirected output UTF-16LE. External programs
        // still write their own bytes to the inherited pipe handles, so output
        // decoding also accepts UTF-8 below.
        process.args(["/D", "/U", "/S", "/C", command]);
        process
    };
    #[cfg(not(windows))]
    let mut process = {
        let mut process = Command::new("sh");
        process.args(["-c", command]);
        process
    };
    #[cfg(windows)]
    let shell_root = windows_shell_directory(&root);
    #[cfg(not(windows))]
    let shell_root = root.clone();
    process
        .current_dir(&shell_root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let child = lifecycle
        .spawn(&mut process, ChildIo::Terminal)
        .map_err(|error| format!("Unable to start owned command: {error}"))?;
    let (output, cancelled) = wait_for_terminal_output(child, cancel)
        .map_err(|error| format!("Unable to collect command output: {error}"))?;
    Ok(TerminalResult {
        exit_code: output.status.code(),
        stdout: decode_terminal_output(&output.stdout),
        stderr: decode_terminal_output(&output.stderr),
        cancelled,
    })
}

#[cfg(windows)]
fn wait_for_terminal_output(
    child: OwnedChild,
    cancel: &AtomicBool,
) -> std::io::Result<(std::process::Output, bool)> {
    child.wait_with_output_cancelled(cancel)
}

#[cfg(not(windows))]
fn wait_for_terminal_output(
    mut child: OwnedChild,
    cancel: &AtomicBool,
) -> std::io::Result<(std::process::Output, bool)> {
    let mut cancelled = false;
    loop {
        if cancel.load(Ordering::Acquire) && !cancelled {
            child.kill()?;
            cancelled = true;
        }
        if child.try_wait()?.is_some() {
            return child.wait_with_output().map(|output| (output, cancelled));
        }
        thread::sleep(std::time::Duration::from_millis(25));
    }
}

#[cfg(windows)]
fn windows_shell_directory(canonical_root: &Path) -> PathBuf {
    use std::path::Prefix;

    let mut components = canonical_root.components();
    match components.next() {
        // std::fs::canonicalize uses an extended-length path. cmd.exe treats a
        // local \\?\C:\... cwd like a UNC cwd, so remove only the VerbatimDisk
        // marker. VerbatimUNC is deliberately left unchanged.
        Some(std::path::Component::Prefix(prefix))
            if matches!(prefix.kind(), Prefix::VerbatimDisk(_)) =>
        {
            let drive = match prefix.kind() {
                Prefix::VerbatimDisk(drive) => drive as char,
                _ => unreachable!(),
            };
            let remainder = components.as_path();
            PathBuf::from(format!("{drive}:\\")).join(remainder)
        }
        _ => canonical_root.to_path_buf(),
    }
}

#[cfg(windows)]
fn decode_terminal_output(bytes: &[u8]) -> String {
    let has_utf16le_bom = bytes.starts_with(&[0xff, 0xfe]);
    let utf16_bytes = bytes.strip_prefix(&[0xff, 0xfe]).unwrap_or(bytes);
    let has_utf16le_text_marker = utf16_bytes.len() % 2 == 0
        && utf16_bytes.chunks_exact(2).any(|pair| {
            pair[1] == 0
                && (pair[0] == b'\t' || pair[0] == b'\r' || pair[0] == b'\n' || pair[0] >= b' ')
        });

    // External developer CLIs commonly emit ASCII/UTF-8 directly to inherited
    // redirected handles. Prefer valid UTF-8 unless the byte layout contains an
    // explicit UTF-16LE marker (BOM or an ASCII/control UTF-16 code unit).
    if !has_utf16le_bom && !has_utf16le_text_marker {
        if let Ok(text) = std::str::from_utf8(bytes) {
            return text.to_owned();
        }
    }

    if has_utf16le_bom || has_utf16le_text_marker {
        let units = utf16_bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        return String::from_utf16_lossy(&units);
    }

    // There is no encoding metadata on an anonymous pipe. Unknown legacy
    // encodings cannot be selected deterministically; preserve the previous
    // loss-tolerant behavior rather than misclassifying arbitrary bytes as UTF-16.
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(not(windows))]
fn decode_terminal_output(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn project_root(project_state: &State<'_, ProjectState>) -> Result<PathBuf, String> {
    project_state
        .root
        .lock()
        .map_err(|_| "Unable to access the project state.".to_string())?
        .clone()
        .ok_or_else(|| "Open a project first.".to_string())
}

fn project_tree(root: &Path) -> Result<ProjectTree, String> {
    let metadata = fs::metadata(root).map_err(|error| error.to_string())?;
    if !metadata.is_dir() {
        return Err("The open project path is no longer a directory.".to_string());
    }
    fs::read_dir(root).map_err(|error| error.to_string())?;
    let name = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Project".to_string());
    Ok(ProjectTree {
        name,
        children: read_directory(root, root),
    })
}

fn resolve_project_file(root: &Path, relative_path: &str) -> Result<PathBuf, String> {
    let root = fs::canonicalize(root).map_err(|error| error.to_string())?;
    let candidate =
        fs::canonicalize(root.join(relative_path)).map_err(|error| error.to_string())?;

    if candidate == root || !candidate.starts_with(&root) {
        return Err("The file must be strictly inside the project directory.".to_string());
    }
    if !fs::metadata(&candidate)
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("The selected path is not a file.".to_string());
    }
    Ok(candidate)
}

fn create_file(root: &Path, relative_path: &str, content: &str) -> Result<(), String> {
    let relative = Path::new(relative_path);
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative.components().any(|component| match component {
            std::path::Component::Normal(name) => !is_safe_windows_path_component(name),
            _ => true,
        })
    {
        return Err(
            "The new file path must use normalized, Windows-safe relative names.".to_string(),
        );
    }

    let canonical_root = fs::canonicalize(root).map_err(|error| error.to_string())?;
    let parent = relative.parent().unwrap_or_else(|| Path::new(""));
    let canonical_parent = fs::canonicalize(canonical_root.join(parent))
        .map_err(|error| format!("The destination directory does not exist: {error}"))?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err("The new file must be inside the project directory.".to_string());
    }
    let file_name = relative
        .file_name()
        .ok_or_else(|| "The new file needs a name.".to_string())?;
    let destination = canonical_parent.join(file_name);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
        .map_err(|error| format!("Unable to create file: {error}"))?;
    if let Err(error) = file
        .write_all(content.as_bytes())
        .and_then(|_| file.sync_all())
    {
        drop(file);
        let _ = fs::remove_file(&destination);
        return Err(format!("Unable to create file: {error}"));
    }
    Ok(())
}

fn is_safe_windows_path_component(name: &std::ffi::OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    if name.is_empty()
        || name.ends_with(['.', ' '])
        || name.chars().any(|character| {
            character <= '\u{1f}'
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '|' | '?' | '*' | '/' | '\\'
                )
        })
    {
        return false;
    }

    let device_name = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    !matches!(device_name.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !device_name.strip_prefix("COM").is_some_and(|number| {
            matches!(number, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        })
        && !device_name.strip_prefix("LPT").is_some_and(|number| {
            matches!(number, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        })
}

fn read_file(root: &Path, relative_path: &str) -> Result<String, String> {
    let path = resolve_project_file(root, relative_path)?;
    let content = fs::read(path).map_err(|error| format!("Unable to read this file: {error}"))?;
    if is_binary(&content) {
        return Err("This binary file cannot be opened as text.".to_string());
    }
    String::from_utf8(content).map_err(|error| format!("Unable to read this file as text: {error}"))
}

fn backup_entry(root: &Path, backup_root: &Path, id: &str) -> Result<BackupEntry, String> {
    if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("Invalid backup identifier.".to_string());
    }
    let canonical_root = fs::canonicalize(root).map_err(|error| error.to_string())?;
    let directory = backup_root.join(id);
    let metadata: BackupMetadata = serde_json::from_slice(
        &fs::read(directory.join("metadata.json"))
            .map_err(|error| format!("Unable to read backup metadata: {error}"))?,
    )
    .map_err(|error| format!("Invalid backup metadata: {error}"))?;
    let original = PathBuf::from(&metadata.original_path);
    let relative = original
        .strip_prefix(&canonical_root)
        .map_err(|_| "This backup belongs to another project.".to_string())?;
    if relative.as_os_str().is_empty() || relative.components().any(|component| {
        !matches!(component, std::path::Component::Normal(name) if is_safe_windows_path_component(name))
    }) {
        return Err("The backup path is unsafe.".to_string());
    }
    let content_bytes = fs::read(directory.join("content.bak"))
        .map_err(|error| format!("Unable to read backup content: {error}"))?;
    if is_binary(&content_bytes) {
        return Err("Binary backups cannot be restored in the text editor.".to_string());
    }
    let content = String::from_utf8(content_bytes)
        .map_err(|error| format!("Unable to read backup content as text: {error}"))?;
    let relative_path = relative.to_string_lossy().replace('\\', "/");
    let target = canonical_root.join(relative);
    let current_content = if target.exists() {
        Some(read_file(&canonical_root, &relative_path)?)
    } else {
        None
    };
    Ok(BackupEntry {
        id: id.to_string(),
        created_at_unix_ms: metadata.created_at_unix_ms,
        relative_path,
        content,
        current_content,
    })
}

fn list_backups(root: &Path, backup_root: &Path) -> Result<Vec<BackupEntry>, String> {
    let mut backups = Vec::new();
    let entries = match fs::read_dir(backup_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(backups),
        Err(error) => return Err(format!("Unable to list backups: {error}")),
    };
    for entry in entries.flatten() {
        let id = entry.file_name().to_string_lossy().into_owned();
        if let Ok(backup) = backup_entry(root, backup_root, &id) {
            backups.push(backup);
        }
    }
    backups.sort_by(|left, right| right.created_at_unix_ms.cmp(&left.created_at_unix_ms));
    Ok(backups)
}

fn restore_backup(
    root: &Path,
    backup_root: &Path,
    id: &str,
    expected_current_content: Option<&str>,
    timestamp: u128,
) -> Result<(), String> {
    let backup = backup_entry(root, backup_root, id)?;
    if backup.current_content.as_deref() != expected_current_content {
        return Err("The file changed on disk after the backup list was opened.".to_string());
    }
    if let Some(expected_current_content) = expected_current_content {
        save_file_checked(
            root,
            &backup.relative_path,
            &backup.content,
            expected_current_content.as_bytes(),
            backup_root,
            timestamp,
        )
    } else {
        create_file(root, &backup.relative_path, &backup.content)
    }
}

fn save_file(
    root: &Path,
    relative_path: &str,
    content: &str,
    backup_root: &Path,
    timestamp: u128,
) -> Result<(), String> {
    save_file_with_expected(
        root,
        relative_path,
        content,
        None,
        "",
        backup_root,
        timestamp,
        || {},
    )
}

fn save_open_file(
    root: &Path,
    relative_path: &str,
    content: &str,
    expected_disk_content: &[u8],
    backup_root: &Path,
    timestamp: u128,
) -> Result<(), String> {
    save_file_with_expected(
        root,
        relative_path,
        content,
        Some(expected_disk_content),
        "The file changed on disk after it was opened.",
        backup_root,
        timestamp,
        || {},
    )
}

fn save_file_checked(
    root: &Path,
    relative_path: &str,
    content: &str,
    expected_disk_content: &[u8],
    backup_root: &Path,
    timestamp: u128,
) -> Result<(), String> {
    save_file_with_expected(
        root,
        relative_path,
        content,
        Some(expected_disk_content),
        "The file changed on disk after the backup list was opened.",
        backup_root,
        timestamp,
        || {},
    )
}

fn save_file_with_expected<F>(
    root: &Path,
    relative_path: &str,
    content: &str,
    expected_disk_content: Option<&[u8]>,
    expected_mismatch_error: &str,
    backup_root: &Path,
    timestamp: u128,
    before_recheck: F,
) -> Result<(), String>
where
    F: FnOnce(),
{
    // Resolve immediately before backup and again before writing. This rejects
    // traversal and symlinks outside the project, including a path changed mid-save.
    let path = resolve_project_file(root, relative_path)?;
    let disk_content = fs::read(&path).map_err(|error| format!("Unable to read file: {error}"))?;
    if expected_disk_content.is_some_and(|expected| expected != disk_content) {
        return Err(expected_mismatch_error.to_string());
    }
    let backup_dir = backup_root.join(timestamp.to_string());
    fs::create_dir_all(&backup_dir).map_err(|error| error.to_string())?;
    fs::write(backup_dir.join("content.bak"), &disk_content)
        .map_err(|error| format!("Unable to create backup: {error}"))?;

    let metadata = BackupMetadata {
        created_at_unix_ms: timestamp / 1_000_000,
        original_path: path.to_string_lossy().into_owned(),
    };
    let metadata_json = serde_json::to_vec_pretty(&metadata).map_err(|error| error.to_string())?;
    fs::write(backup_dir.join("metadata.json"), metadata_json)
        .map_err(|error| format!("Unable to save backup metadata: {error}"))?;

    before_recheck();
    let rechecked_path = resolve_project_file(root, relative_path)?;
    if rechecked_path != path {
        return Err("The file path changed while it was being saved.".to_string());
    }
    let rechecked_content = fs::read(&rechecked_path)
        .map_err(|error| format!("Unable to recheck file before saving: {error}"))?;
    if rechecked_content != disk_content {
        return Err("The file changed on disk while it was being backed up.".to_string());
    }
    atomic_replace(&rechecked_path, content.as_bytes(), timestamp)
        .map_err(|error| format!("Unable to save file: {error}"))
}

fn delete_file(
    root: &Path,
    relative_path: &str,
    expected_content: &[u8],
    backup_root: &Path,
    timestamp: u128,
) -> Result<(), String> {
    let path = resolve_project_file(root, relative_path)?;
    let disk_content = fs::read(&path).map_err(|error| format!("Unable to read file: {error}"))?;
    if disk_content != expected_content {
        return Err("The file changed on disk after the deletion was proposed.".to_string());
    }
    let backup_dir = backup_root.join(timestamp.to_string());
    fs::create_dir_all(&backup_dir).map_err(|error| error.to_string())?;
    fs::write(backup_dir.join("content.bak"), &disk_content)
        .map_err(|error| format!("Unable to create backup: {error}"))?;
    let metadata = BackupMetadata {
        created_at_unix_ms: timestamp / 1_000_000,
        original_path: path.to_string_lossy().into_owned(),
    };
    let metadata_json = serde_json::to_vec_pretty(&metadata).map_err(|error| error.to_string())?;
    fs::write(backup_dir.join("metadata.json"), metadata_json)
        .map_err(|error| format!("Unable to save backup metadata: {error}"))?;

    let rechecked_path = resolve_project_file(root, relative_path)?;
    if rechecked_path != path {
        return Err("The file path changed while it was being deleted.".to_string());
    }
    let rechecked_content = fs::read(&rechecked_path)
        .map_err(|error| format!("Unable to recheck file before deletion: {error}"))?;
    if rechecked_content != disk_content {
        return Err("The file changed on disk while it was being backed up.".to_string());
    }
    fs::remove_file(rechecked_path).map_err(|error| format!("Unable to delete file: {error}"))
}

fn atomic_replace(path: &Path, content: &[u8], timestamp: u128) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "The file has no parent directory",
        )
    })?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    let temporary = parent.join(format!(".{file_name}.autocoder-{timestamp}.tmp"));

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = (|| {
        file.set_permissions(fs::metadata(path)?.permissions())?;
        file.write_all(content)?;
        file.sync_all()?;
        drop(file);
        replace_file(&temporary, path)
    })();
    result
}

#[cfg(not(windows))]
fn replace_file(replacement: &Path, destination: &Path) -> std::io::Result<()> {
    let result = fs::rename(replacement, destination);
    if result.is_err() {
        let _ = fs::remove_file(replacement);
    }
    result
}

#[derive(Debug, Eq, PartialEq)]
enum WindowsReplaceFailure {
    OriginalNamesRetained,
    RestoreSafetyBackup,
}

fn classify_windows_replace_failure(error_code: i32) -> WindowsReplaceFailure {
    // Microsoft documents 1175 and 1176 (when lpBackupFileName is supplied)
    // as retaining the original names. 1177 moves the old destination to the
    // supplied backup name and therefore requires restoration before cleanup.
    match error_code {
        1177 => WindowsReplaceFailure::RestoreSafetyBackup,
        1175 | 1176 => WindowsReplaceFailure::OriginalNamesRetained,
        _ => WindowsReplaceFailure::OriginalNamesRetained,
    }
}

#[cfg(windows)]
fn replace_file(replacement: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;

    let mut safety_name = replacement.as_os_str().to_os_string();
    safety_name.push(".replaced.bak");
    let safety_backup = PathBuf::from(safety_name);
    let destination_wide: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let replacement_wide: Vec<u16> = replacement
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let safety_backup_wide: Vec<u16> = safety_backup
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let succeeded = unsafe {
        ReplaceFileW(
            destination_wide.as_ptr(),
            replacement_wide.as_ptr(),
            safety_backup_wide.as_ptr(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if succeeded != 0 {
        let _ = fs::remove_file(safety_backup);
        return Ok(());
    }

    let error = std::io::Error::last_os_error();
    let disposition = classify_windows_replace_failure(error.raw_os_error().unwrap_or_default());
    if disposition == WindowsReplaceFailure::RestoreSafetyBackup || !destination.is_file() {
        restore_windows_safety_backup(&safety_backup, destination).map_err(|restore_error| {
            std::io::Error::new(
                restore_error.kind(),
                format!(
                    "ReplaceFileW failed ({error}); restoring '{}' to '{}' also failed ({restore_error}). Recovery copies were preserved.",
                    safety_backup.display(),
                    destination.display()
                ),
            )
        })?;
    }
    if destination.is_file() {
        // Only discard the proposed new content after the old destination has
        // been verified or restored. The app-data backup remains independent.
        let _ = fs::remove_file(replacement);
    }
    Err(error)
}

#[cfg(windows)]
fn restore_windows_safety_backup(safety_backup: &Path, destination: &Path) -> std::io::Result<()> {
    if destination.is_file() {
        return Ok(());
    }
    fs::rename(safety_backup, destination).or_else(|rename_error| {
        if destination.exists() {
            return Err(rename_error);
        }
        let mut source = fs::File::open(safety_backup)?;
        let mut restored = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)?;
        std::io::copy(&mut source, &mut restored)?;
        restored.sync_all()
    })
}

fn is_excluded_directory(name: &str) -> bool {
    name.starts_with('.') || EXCLUDED_DIRECTORY_NAMES.contains(&name.to_lowercase().as_str())
}

fn is_binary(content: &[u8]) -> bool {
    if std::str::from_utf8(content).is_err() || content.contains(&0) {
        return true;
    }

    let suspicious_controls = content
        .iter()
        .filter(|byte| **byte < 0x20 && !matches!(**byte, b'\n' | b'\r' | b'\t' | 0x0c | 0x08))
        .count();
    !content.is_empty() && suspicious_controls * 100 / content.len() > 1
}

fn read_directory(root: &Path, path: &Path) -> Vec<FileTreeNode> {
    let mut nodes = Vec::new();
    let Ok(entries) = fs::read_dir(path) else {
        return nodes;
    };

    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if should_skip_entry(&entry, file_type) {
            continue;
        }

        let name = entry.file_name().to_string_lossy().into_owned();
        let entry_path = entry.path();
        let relative_path = entry_path
            .strip_prefix(root)
            .unwrap_or(&entry_path)
            .to_string_lossy()
            .into_owned();

        if file_type.is_dir() {
            if is_excluded_directory(&name) {
                continue;
            }
            nodes.push(FileTreeNode {
                name,
                path: relative_path,
                kind: FileTreeNodeKind::Directory,
                children: read_directory(root, &entry_path),
            });
        } else if file_type.is_file() {
            nodes.push(FileTreeNode {
                name,
                path: relative_path,
                kind: FileTreeNodeKind::File,
                children: Vec::new(),
            });
        }
    }

    nodes.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    nodes
}

fn should_skip_entry(entry: &fs::DirEntry, file_type: fs::FileType) -> bool {
    if file_type.is_symlink() {
        return true;
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        if entry
            .metadata()
            .is_ok_and(|metadata| metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
        {
            return true;
        }

        // Shell shortcuts are ordinary .lnk files to std::fs, not symlinks.
        // They are navigation objects rather than project file contents.
        if file_type.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("lnk"))
        {
            return true;
        }
    }

    #[cfg(not(windows))]
    let _ = entry;

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::Barrier;
    use tempfile::TempDir;

    fn project() -> (TempDir, PathBuf) {
        let directory = TempDir::new().expect("temporary project");
        let file = directory.path().join("notes.txt");
        fs::write(&file, "before").expect("fixture file");
        (directory, file)
    }

    #[test]
    fn terminal_start_and_project_switch_are_atomic() {
        for _ in 0..100 {
            let project_state = Arc::new(ProjectState {
                root: Mutex::new(Some(PathBuf::from("project-a"))),
            });
            let terminal_state = Arc::new(TerminalState::default());
            let barrier = Arc::new(Barrier::new(3));

            let command_project = Arc::clone(&project_state);
            let command_terminal = Arc::clone(&terminal_state);
            let command_barrier = Arc::clone(&barrier);
            let command = thread::spawn(move || {
                command_barrier.wait();
                command_terminal
                    .begin_project_command(&command_project)
                    .unwrap()
                    .0
            });

            let switch_project = Arc::clone(&project_state);
            let switch_terminal = Arc::clone(&terminal_state);
            let switch_barrier = Arc::clone(&barrier);
            let project_switch = thread::spawn(move || {
                switch_barrier.wait();
                switch_terminal.switch_project(&switch_project, PathBuf::from("project-b"))
            });

            barrier.wait();
            let command_root = command.join().unwrap();
            let switch_result = project_switch.join().unwrap();
            if switch_result.is_ok() {
                assert_eq!(command_root, PathBuf::from("project-b"));
            } else {
                assert_eq!(command_root, PathBuf::from("project-a"));
                assert_eq!(
                    project_state.root.lock().unwrap().as_deref(),
                    Some(Path::new("project-a"))
                );
            }
        }
    }

    #[test]
    fn reopening_the_current_project_is_not_a_switch_and_is_allowed_during_a_command() {
        let root = PathBuf::from("project-a");
        let project_state = ProjectState {
            root: Mutex::new(Some(root.clone())),
        };
        let terminal_state = TerminalState::default();
        terminal_state
            .begin_project_command(&project_state)
            .unwrap();

        assert!(!terminal_state.switch_project(&project_state, root).unwrap());
        assert!(terminal_state.active_cancel.lock().unwrap().is_some());
    }

    #[test]
    fn rebuilding_project_tree_discovers_external_file_changes() {
        let (directory, _) = project();
        let initial = project_tree(directory.path()).unwrap();
        assert!(initial.children.iter().any(|node| node.name == "notes.txt"));
        assert!(!initial.children.iter().any(|node| node.name == "added.rs"));

        fs::write(directory.path().join("added.rs"), "fn main() {}\n").unwrap();
        let refreshed = project_tree(directory.path()).unwrap();
        assert!(refreshed
            .children
            .iter()
            .any(|node| node.name == "added.rs"));
        assert_eq!(
            refreshed.name,
            directory.path().file_name().unwrap().to_string_lossy()
        );
    }

    #[test]
    fn refreshing_project_reloads_or_closes_the_open_file() {
        let (directory, file) = project();
        fs::write(&file, "changed externally").unwrap();

        let refreshed = refresh_project_state(directory.path(), Some("notes.txt")).unwrap();
        assert_eq!(
            refreshed.open_file_content.as_deref(),
            Some("changed externally")
        );

        fs::remove_file(file).unwrap();
        let refreshed = refresh_project_state(directory.path(), Some("notes.txt")).unwrap();
        assert!(refreshed.open_file_content.is_none());
        assert!(!refreshed
            .project
            .children
            .iter()
            .any(|node| node.name == "notes.txt"));
    }

    #[test]
    fn terminal_command_runs_in_project_and_captures_output() {
        let (directory, _) = project();
        let lifecycle = ProcessLifecycle::new().unwrap();
        #[cfg(windows)]
        let command = "echo output & echo problem 1>&2 & exit /b 7";
        #[cfg(not(windows))]
        let command = "printf output; printf problem >&2; exit 7";

        let result = run_project_command(
            directory.path(),
            command,
            &lifecycle,
            &AtomicBool::new(false),
        )
        .unwrap();

        assert_eq!(result.exit_code, Some(7));
        assert!(result.stdout.contains("output"));
        assert!(result.stderr.contains("problem"));
    }

    #[test]
    fn terminal_ascii_stdout_is_readable() {
        let (directory, _) = project();
        let lifecycle = ProcessLifecycle::new().unwrap();
        let result = run_project_command(
            directory.path(),
            "echo AUTOCODER_TERMINAL_OK",
            &lifecycle,
            &AtomicBool::new(false),
        )
        .unwrap();

        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.contains("AUTOCODER_TERMINAL_OK"));
    }

    #[cfg(windows)]
    #[test]
    fn terminal_uses_unicode_project_directory_without_verbatim_cwd() {
        let parent = TempDir::new().expect("temporary parent");
        let directory = parent.path().join("AutoCoder_Тест");
        fs::create_dir(&directory).unwrap();
        let canonical = fs::canonicalize(&directory).unwrap();
        let shell_directory = windows_shell_directory(&canonical);

        assert_eq!(shell_directory, directory);
        assert!(!shell_directory.to_string_lossy().starts_with(r"\\?\"));

        let lifecycle = ProcessLifecycle::new().unwrap();
        let result =
            run_project_command(&directory, "cd", &lifecycle, &AtomicBool::new(false)).unwrap();
        assert_eq!(result.exit_code, Some(0));
        assert_eq!(result.stdout.trim(), directory.to_string_lossy());
        assert!(!result.stderr.contains("UNC"));
    }

    #[cfg(windows)]
    #[test]
    fn terminal_preserves_real_unc_directory_representation() {
        let unc = Path::new(r"\\?\UNC\server\share\AutoCoder_Тест");
        assert_eq!(windows_shell_directory(unc), unc);
    }

    #[cfg(windows)]
    #[test]
    fn terminal_decodes_unicode_stdout_and_stderr() {
        let (directory, _) = project();
        let lifecycle = ProcessLifecycle::new().unwrap();
        let result = run_project_command(
            directory.path(),
            "echo Русский stdout & echo Русский stderr 1>&2 & exit /b 9",
            &lifecycle,
            &AtomicBool::new(false),
        )
        .unwrap();

        assert_eq!(result.exit_code, Some(9));
        assert!(result.stdout.contains("Русский stdout"));
        assert!(result.stderr.contains("Русский stderr"));
        assert!(!result.stdout.contains('�'));
        assert!(!result.stderr.contains('�'));
    }

    #[cfg(windows)]
    #[test]
    fn terminal_decodes_external_ascii_stdout_and_stderr() {
        let (directory, _) = project();
        let lifecycle = ProcessLifecycle::new().unwrap();
        let result = run_project_command(
            directory.path(),
            "powershell.exe -NoLogo -NoProfile -NonInteractive -Command \"$o=[Console]::OpenStandardOutput();$e=[Console]::OpenStandardError();$a=[Text.Encoding]::ASCII.GetBytes('EXTERNAL_ASCII_STDOUT');$b=[Text.Encoding]::ASCII.GetBytes('EXTERNAL_ASCII_STDERR');$o.Write($a,0,$a.Length);$e.Write($b,0,$b.Length)\"",
            &lifecycle,
            &AtomicBool::new(false),
        )
        .unwrap();

        assert_eq!(result.exit_code, Some(0));
        assert_eq!(result.stdout, "EXTERNAL_ASCII_STDOUT");
        assert_eq!(result.stderr, "EXTERNAL_ASCII_STDERR");
    }

    #[cfg(windows)]
    #[test]
    fn terminal_decodes_external_utf8_stdout_stderr_and_exit_code() {
        let (directory, _) = project();
        let lifecycle = ProcessLifecycle::new().unwrap();
        let result = run_project_command(
            directory.path(),
            "powershell.exe -NoLogo -NoProfile -NonInteractive -Command \"$o=[Console]::OpenStandardOutput();$e=[Console]::OpenStandardError();$a=[Text.Encoding]::UTF8.GetBytes('Внешний stdout');$b=[Text.Encoding]::UTF8.GetBytes('Внешний stderr');$o.Write($a,0,$a.Length);$e.Write($b,0,$b.Length);exit 13\"",
            &lifecycle,
            &AtomicBool::new(false),
        )
        .unwrap();

        assert_eq!(result.exit_code, Some(13));
        assert_eq!(result.stdout, "Внешний stdout");
        assert_eq!(result.stderr, "Внешний stderr");
    }

    #[cfg(windows)]
    #[test]
    fn terminal_decoder_does_not_treat_even_utf8_as_utf16() {
        assert_eq!(
            decode_terminal_output(b"git --version\r\n"),
            "git --version\r\n"
        );
        assert_eq!(
            decode_terminal_output("UTF-8: Привет".as_bytes()),
            "UTF-8: Привет"
        );
    }

    #[test]
    fn terminal_rejects_empty_command() {
        let (directory, _) = project();
        let lifecycle = ProcessLifecycle::new().unwrap();
        assert!(
            run_project_command(directory.path(), "  ", &lifecycle, &AtomicBool::new(false))
                .is_err()
        );
    }

    #[cfg(windows)]
    #[test]
    fn lifecycle_shutdown_interrupts_long_terminal_command() {
        use std::os::windows::process::CommandExt;

        let (directory, _) = project();
        let mut external = Command::new("cmd.exe")
            .args(["/D", "/S", "/C", "ping 127.0.0.1 -n 60 >nul"])
            .creation_flags(0x08000000)
            .spawn()
            .unwrap();
        let lifecycle = ProcessLifecycle::new().unwrap();
        let mut command = Command::new("cmd.exe");
        command
            .args([
                "/D",
                "/S",
                "/C",
                "cmd.exe /D /S /C \"ping 127.0.0.1 -n 60 >nul\"",
            ])
            .current_dir(directory.path())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let child = lifecycle.spawn(&mut command, ChildIo::Terminal).unwrap();

        lifecycle.shutdown();

        let output = child.wait_with_output().unwrap();
        assert!(!output.status.success());
        assert!(external.try_wait().unwrap().is_none());
        external.kill().unwrap();
        external.wait().unwrap();
    }

    #[cfg(unix)]
    fn sleeping_child(seconds: &str) -> OwnedChild {
        Command::new("sleep")
            .arg(seconds)
            .spawn()
            .expect("sleep fixture")
    }

    fn one_response_server(response: &'static [u8]) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 512];
            let _ = std::io::Read::read(&mut stream, &mut request);
            stream.write_all(response).unwrap();
        });
        (address, handle)
    }

    #[test]
    fn open_tcp_port_without_version_api_is_not_ready() {
        let (address, server) = one_response_server(
            b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        assert!(!ollama_api_is_ready(
            &address,
            std::time::Duration::from_secs(1)
        ));
        server.join().unwrap();
    }

    #[test]
    fn http_503_readiness_preserves_status_for_diagnostics() {
        let (address, server) = one_response_server(
            b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        assert_eq!(
            ollama_api_readiness(&address, std::time::Duration::from_secs(1)),
            OllamaReadiness::HttpStatus(503)
        );
        server.join().unwrap();
    }

    #[test]
    fn successful_version_api_is_ready() {
        let (address, server) = one_response_server(
            b"HTTP/1.1 200 OK\r\nContent-Length: 20\r\nConnection: close\r\n\r\n{\"version\":\"0.11.0\"}",
        );
        assert!(ollama_api_is_ready(
            &address,
            std::time::Duration::from_secs(1)
        ));
        server.join().unwrap();
    }

    #[test]
    fn lifecycle_shutdown_is_idempotent_and_rejects_new_launches() {
        let lifecycle = ProcessLifecycle::new().unwrap();
        lifecycle.shutdown();
        lifecycle.shutdown();
        assert!(!lifecycle.is_accepting());
        let mut command = Command::new("must-not-run");
        assert!(lifecycle.spawn(&mut command, ChildIo::Ollama).is_err());
    }

    #[test]
    fn windows_owned_launch_is_suspended_before_job_assignment() {
        let flags = process_lifecycle::owned_creation_flags(true);
        assert_ne!(flags & 0x0000_0004, 0); // CREATE_SUSPENDED
        assert_ne!(flags & 0x0800_0000, 0); // CREATE_NO_WINDOW
    }

    #[test]
    fn existing_ollama_is_not_owned_or_stopped() {
        let state = OllamaState::new(Arc::new(ProcessLifecycle::new().unwrap()));
        let mut launched = false;
        state
            .ensure_running_with(
                || true,
                || {
                    launched = true;
                    Err("must not launch".into())
                },
                || {},
            )
            .unwrap();
        state.shutdown();
        assert!(!launched);
        assert!(state.owned.lock().unwrap().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn owned_ollama_is_stopped_and_reaped_on_shutdown() {
        let state = OllamaState::new(Arc::new(ProcessLifecycle::new().unwrap()));
        *state.owned.lock().unwrap() = Some(sleeping_child("30"));
        state.shutdown();
        assert!(state.owned.lock().unwrap().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn shutdown_is_safe_when_owned_ollama_already_exited() {
        let state = OllamaState::new(Arc::new(ProcessLifecycle::new().unwrap()));
        let mut child = Command::new("true").spawn().unwrap();
        child.wait().unwrap();
        *state.owned.lock().unwrap() = Some(child);
        state.shutdown();
        assert!(state.owned.lock().unwrap().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn repeated_requests_do_not_launch_another_owned_ollama() {
        use std::cell::Cell;
        let state = OllamaState::new(Arc::new(ProcessLifecycle::new().unwrap()));
        let ready_calls = Cell::new(0);
        let launches = Cell::new(0);
        state
            .ensure_running_with(
                || {
                    let call = ready_calls.get();
                    ready_calls.set(call + 1);
                    call > 0
                },
                || {
                    launches.set(launches.get() + 1);
                    Ok(sleeping_child("30"))
                },
                || {},
            )
            .unwrap();
        state
            .ensure_running_with(
                || true,
                || {
                    launches.set(launches.get() + 1);
                    Ok(sleeping_child("30"))
                },
                || {},
            )
            .unwrap();
        assert_eq!(launches.get(), 1);
        state.shutdown();
    }

    #[test]
    fn production_windows_children_use_create_no_window_unless_overridden() {
        assert_eq!(windows_creation_flags(true, true, false), 0x0800_0000);
        assert_eq!(windows_creation_flags(true, true, true), 0);
        assert_eq!(windows_creation_flags(false, true, false), 0);
        assert_eq!(windows_creation_flags(true, false, false), 0);
    }

    #[test]
    fn managed_ollama_url_accepts_supported_loopback_hosts_only() {
        assert!(is_managed_local_ollama_url(
            "http://127.0.0.1:11434/api/chat"
        ));
        assert!(is_managed_local_ollama_url(
            "http://localhost:11434/api/chat"
        ));
        assert!(is_managed_local_ollama_url("http://[::1]:11434/api/chat"));
        assert!(!is_managed_local_ollama_url(
            "https://ollama.example.com/api/chat"
        ));
    }

    #[test]
    fn chat_request_serializes_cyrillic_as_utf8_without_a_bom() {
        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "Ответь дословно".to_string(),
            }],
            context: Some(ChatContext {
                open_file: Some(OpenFileContext {
                    path: "АвтоКодер_тестовый файл.txt".to_string(),
                    content: "123 123 123".to_string(),
                    saved_content: "123 123 123".to_string(),
                }),
                selection: Some(SelectionContext::None),
                project: None,
            }),
        };
        let bytes = serde_json::to_vec(&request).unwrap();

        assert!(!bytes.starts_with(&[0xef, 0xbb, 0xbf]));
        assert!(bytes
            .windows("Ответь дословно".len())
            .any(|window| window == "Ответь дословно".as_bytes()));
        let decoded: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded["messages"][0]["content"], "Ответь дословно");
        assert_eq!(
            decoded["context"]["openFile"]["path"],
            "АвтоКодер_тестовый файл.txt"
        );
        assert_eq!(
            decoded["context"]["openFile"]["savedContent"],
            "123 123 123"
        );
        assert_eq!(decoded["context"]["selection"]["state"], "none");
    }

    #[cfg(windows)]
    #[test]
    fn packaged_backend_uses_python_from_the_resource_directory() {
        let resources = Path::new(r"C:\Program Files\AutoCoder\resources");
        let (python, backend) = backend_paths(resources, None);

        assert_eq!(python, resources.join("python-runtime").join("python.exe"));
        assert_eq!(backend, resources.join("backend").join("main.py"));
    }

    #[test]
    fn explicit_python_override_is_preserved_for_development() {
        let resources = Path::new("resources");
        let override_path = PathBuf::from("debug-python");
        let (python, _) = backend_paths(resources, Some(override_path.clone()));

        assert_eq!(python, override_path);
    }

    #[test]
    fn chat_response_accepts_bomless_utf8_cyrillic_from_backend() {
        let bytes =
            r#"{"message":{"role":"assistant","content":"Готово: файл сохранён"}}"#.as_bytes();

        assert!(!bytes.starts_with(&[0xef, 0xbb, 0xbf]));
        let response: ChatResponse = serde_json::from_slice(bytes).unwrap();
        assert_eq!(response.message.role, "assistant");
        assert_eq!(response.message.content, "Готово: файл сохранён");
    }

    #[test]
    fn chat_response_preserves_terminal_proposal_across_bridge_serialization() {
        let backend_json = r#"{"message":{"role":"assistant","content":"Run this"},"proposal":null,"commandProposal":{"command":"pwd"}}"#;

        let response: ChatResponse = serde_json::from_str(backend_json).unwrap();
        assert_eq!(
            response
                .command_proposal
                .as_ref()
                .map(|proposal| proposal.command.as_str()),
            Some("pwd")
        );

        let tauri_json = serde_json::to_value(response).unwrap();
        assert_eq!(tauri_json["commandProposal"]["command"], "pwd");
        assert!(tauri_json.get("command_proposal").is_none());
    }

    #[test]
    fn chat_response_accepts_null_or_absent_terminal_proposal() {
        for backend_json in [
            r#"{"message":{"role":"assistant","content":"No command"},"proposal":null,"commandProposal":null}"#,
            r#"{"message":{"role":"assistant","content":"No command"},"proposal":null}"#,
        ] {
            let response: ChatResponse = serde_json::from_str(backend_json).unwrap();
            assert!(response.command_proposal.is_none());
        }
    }

    #[test]
    fn chat_request_distinguishes_active_and_absent_editor_selection() {
        let active: ChatRequest = serde_json::from_str(
            r#"{"messages":[{"role":"user","content":"What is selected?"}],"context":{"selection":{"state":"active","path":"two.txt","content":"123"}}}"#,
        )
        .unwrap();
        let none: ChatRequest = serde_json::from_str(
            r#"{"messages":[{"role":"user","content":"What is selected?"}],"context":{"selection":{"state":"none"}}}"#,
        )
        .unwrap();

        assert!(matches!(
            active.context.unwrap().selection,
            Some(SelectionContext::Active { path, content })
                if path == "two.txt" && content == "123"
        ));
        assert!(matches!(
            none.context.unwrap().selection,
            Some(SelectionContext::None)
        ));
    }

    #[test]
    fn rejects_paths_outside_the_project() {
        let (directory, _) = project();
        let outside = directory.path().parent().unwrap().join("outside.txt");
        fs::write(&outside, "secret").expect("outside fixture");

        let result = resolve_project_file(directory.path(), "../outside.txt");

        assert!(result.is_err());
    }

    #[test]
    fn reads_a_project_file() {
        let (directory, _) = project();
        assert_eq!(read_file(directory.path(), "notes.txt").unwrap(), "before");
    }

    #[test]
    fn saves_a_file_and_preserves_a_backup_with_metadata() {
        let (directory, file) = project();
        let backups = TempDir::new().expect("temporary backups");
        let timestamp = 1_725_000_000_123_000_000;

        save_file(
            directory.path(),
            "notes.txt",
            "after",
            backups.path(),
            timestamp,
        )
        .unwrap();

        let backup_dir = backups.path().join(timestamp.to_string());
        assert_eq!(fs::read_to_string(file).unwrap(), "after");
        assert_eq!(
            fs::read_to_string(backup_dir.join("content.bak")).unwrap(),
            "before"
        );
        let metadata: serde_json::Value =
            serde_json::from_slice(&fs::read(backup_dir.join("metadata.json")).unwrap()).unwrap();
        assert_eq!(
            metadata["createdAtUnixMs"].as_u64(),
            Some(u64::try_from(timestamp / 1_000_000).unwrap())
        );
        assert!(metadata["originalPath"]
            .as_str()
            .unwrap()
            .ends_with("notes.txt"));
        assert_eq!(
            fs::read_dir(directory.path()).unwrap().count(),
            1,
            "the atomic replacement must not leave a temporary file"
        );
    }

    #[test]
    fn open_file_save_refuses_to_overwrite_an_external_change() {
        let (directory, file) = project();
        let backups = TempDir::new().expect("temporary backups");
        fs::write(&file, "external version").unwrap();

        let error = save_open_file(
            directory.path(),
            "notes.txt",
            "editor version",
            b"before",
            backups.path(),
            2_000_000,
        )
        .unwrap_err();

        assert_eq!(error, "The file changed on disk after it was opened.");
        assert_eq!(fs::read_to_string(file).unwrap(), "external version");
        assert!(!backups.path().join("2000000").exists());
    }

    #[test]
    fn lists_only_backups_for_the_open_project_and_restores_safely() {
        let (directory, file) = project();
        let backups = TempDir::new().expect("temporary backups");
        save_file(
            directory.path(),
            "notes.txt",
            "after",
            backups.path(),
            2_000_000,
        )
        .unwrap();

        let other = backups.path().join("3000000");
        fs::create_dir(&other).unwrap();
        fs::write(other.join("content.bak"), "foreign").unwrap();
        fs::write(
            other.join("metadata.json"),
            serde_json::to_vec(&BackupMetadata {
                created_at_unix_ms: 3,
                original_path: directory
                    .path()
                    .join("../foreign.txt")
                    .to_string_lossy()
                    .into_owned(),
            })
            .unwrap(),
        )
        .unwrap();

        let listed = list_backups(directory.path(), backups.path()).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].relative_path, "notes.txt");
        assert_eq!(listed[0].content, "before");
        assert_eq!(listed[0].current_content.as_deref(), Some("after"));

        let stale = restore_backup(
            directory.path(),
            backups.path(),
            "2000000",
            Some("stale"),
            4_000_000,
        );
        assert!(stale.unwrap_err().contains("changed on disk"));
        restore_backup(
            directory.path(),
            backups.path(),
            "2000000",
            Some("after"),
            4_000_000,
        )
        .unwrap();
        assert_eq!(fs::read_to_string(file).unwrap(), "before");
        assert_eq!(
            fs::read_to_string(backups.path().join("4000000/content.bak")).unwrap(),
            "after"
        );
    }

    #[test]
    fn restores_a_deleted_file_without_overwriting_a_new_file() {
        let (directory, file) = project();
        let backups = TempDir::new().expect("temporary backups");
        delete_file(
            directory.path(),
            "notes.txt",
            b"before",
            backups.path(),
            2_000_000,
        )
        .unwrap();

        restore_backup(directory.path(), backups.path(), "2000000", None, 3_000_000).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "before");
        fs::write(&file, "new external content").unwrap();
        let error = restore_backup(directory.path(), backups.path(), "2000000", None, 4_000_000)
            .unwrap_err();
        assert!(error.contains("changed on disk"));
        assert_eq!(fs::read_to_string(file).unwrap(), "new external content");
    }

    #[test]
    fn refuses_restore_when_an_existing_file_changed_after_listing() {
        let (directory, file) = project();
        let backups = TempDir::new().expect("temporary backups");
        save_file(
            directory.path(),
            "notes.txt",
            "listed version",
            backups.path(),
            2_000_000,
        )
        .unwrap();
        let listed = list_backups(directory.path(), backups.path()).unwrap();
        let expected = listed[0].current_content.as_deref().unwrap();
        fs::write(&file, "external version").unwrap();

        let error = restore_backup(
            directory.path(),
            backups.path(),
            "2000000",
            Some(expected),
            3_000_000,
        )
        .unwrap_err();

        assert!(error.contains("changed on disk"));
        assert_eq!(fs::read_to_string(file).unwrap(), "external version");
        assert!(!backups.path().join("3000000").exists());
    }

    #[test]
    fn checked_save_refuses_a_change_between_backup_and_replacement() {
        let (directory, file) = project();
        let backups = TempDir::new().expect("temporary backups");
        let file_during_recheck = file.clone();

        let error = save_file_with_expected(
            directory.path(),
            "notes.txt",
            "restored version",
            Some(b"before"),
            "The file changed on disk after the backup list was opened.",
            backups.path(),
            2_000_000,
            move || fs::write(file_during_recheck, "external version").unwrap(),
        )
        .unwrap_err();

        assert!(error.contains("changed on disk"));
        assert_eq!(fs::read_to_string(file).unwrap(), "external version");
        assert_eq!(
            fs::read_to_string(backups.path().join("2000000/content.bak")).unwrap(),
            "before"
        );
    }

    #[test]
    fn atomic_replace_does_not_overwrite_an_existing_temporary_file() {
        let (directory, file) = project();
        let timestamp = 42;
        let temporary = directory.path().join(".notes.txt.autocoder-42.tmp");
        fs::write(&temporary, "do not overwrite").unwrap();

        let error = atomic_replace(&file, b"after", timestamp).unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(file).unwrap(), "before");
        assert_eq!(fs::read_to_string(temporary).unwrap(), "do not overwrite");
    }

    #[test]
    fn classifies_documented_windows_replace_partial_failures() {
        assert_eq!(
            classify_windows_replace_failure(1175),
            WindowsReplaceFailure::OriginalNamesRetained
        );
        assert_eq!(
            classify_windows_replace_failure(1176),
            WindowsReplaceFailure::OriginalNamesRetained
        );
        assert_eq!(
            classify_windows_replace_failure(1177),
            WindowsReplaceFailure::RestoreSafetyBackup
        );
        assert_eq!(
            classify_windows_replace_failure(87),
            WindowsReplaceFailure::OriginalNamesRetained
        );
    }

    #[test]
    fn tree_excludes_hidden_directories_but_keeps_all_regular_files() {
        let (directory, _) = project();
        fs::create_dir(directory.path().join(".cache")).unwrap();
        fs::write(directory.path().join("unknown.custom"), "unknown text").unwrap();
        fs::write(directory.path().join("README"), "read me").unwrap();
        fs::write(directory.path().join("binary.bin"), [0, 159, 146, 150]).unwrap();
        fs::write(
            directory.path().join("АвтоКодер_тестовый файл.txt"),
            "обычный текст",
        )
        .unwrap();

        let nodes = read_directory(directory.path(), directory.path());
        let names: Vec<_> = nodes.iter().map(|node| node.name.as_str()).collect();

        assert_eq!(
            names,
            [
                "binary.bin",
                "notes.txt",
                "README",
                "unknown.custom",
                "АвтоКодер_тестовый файл.txt"
            ]
        );
    }

    #[test]
    fn creates_a_new_file_without_overwriting_or_escaping_the_project() {
        let (directory, _) = project();
        fs::create_dir(directory.path().join("src")).unwrap();

        create_file(directory.path(), "src/new.txt", "created").unwrap();
        assert_eq!(
            fs::read_to_string(directory.path().join("src/new.txt")).unwrap(),
            "created"
        );
        assert!(create_file(directory.path(), "src/new.txt", "overwritten").is_err());
        assert!(create_file(directory.path(), "../outside.txt", "escaped").is_err());
    }

    #[test]
    fn deletes_a_file_only_after_preserving_a_backup() {
        let (directory, file) = project();
        let backups = TempDir::new().expect("temporary backups");
        let timestamp = 1_725_000_000_456_000_000;

        delete_file(
            directory.path(),
            "notes.txt",
            b"before",
            backups.path(),
            timestamp,
        )
        .unwrap();

        assert!(!file.exists());
        let backup_dir = backups.path().join(timestamp.to_string());
        assert_eq!(
            fs::read_to_string(backup_dir.join("content.bak")).unwrap(),
            "before"
        );
        let metadata: serde_json::Value =
            serde_json::from_slice(&fs::read(backup_dir.join("metadata.json")).unwrap()).unwrap();
        assert!(metadata["originalPath"]
            .as_str()
            .unwrap()
            .ends_with("notes.txt"));
        assert!(delete_file(
            directory.path(),
            "../outside.txt",
            b"escaped",
            backups.path(),
            timestamp + 1
        )
        .is_err());
    }

    #[test]
    fn refuses_to_delete_a_file_changed_on_disk_after_the_proposal() {
        let (directory, file) = project();
        let backups = TempDir::new().expect("temporary backups");
        fs::write(&file, "external change").unwrap();

        let error =
            delete_file(directory.path(), "notes.txt", b"before", backups.path(), 42).unwrap_err();

        assert!(error.contains("changed on disk"));
        assert_eq!(fs::read_to_string(file).unwrap(), "external change");
        assert!(!backups.path().join("42").exists());
    }

    #[test]
    fn validates_windows_file_name_semantics_for_new_files() {
        for allowed in ["src", "new.txt", "данные-שלום.txt"] {
            assert!(is_safe_windows_path_component(std::ffi::OsStr::new(
                allowed
            )));
        }
        for denied in [
            "existing.txt:stream",
            "CON",
            "CON.txt",
            "NUL.txt",
            "COM1.log",
            "question?.txt",
            "star*.txt",
            "trailing.",
            "trailing ",
        ] {
            assert!(
                !is_safe_windows_path_component(std::ffi::OsStr::new(denied)),
                "unexpectedly allowed {denied}"
            );
        }
    }

    #[test]
    fn creates_a_file_with_a_normal_unicode_name() {
        let (directory, _) = project();
        fs::create_dir(directory.path().join("src")).unwrap();
        create_file(directory.path(), "src/данные-שלום.txt", "unicode").unwrap();
        assert_eq!(
            fs::read_to_string(directory.path().join("src/данные-שלום.txt")).unwrap(),
            "unicode"
        );
    }

    #[test]
    fn reads_text_with_known_unknown_and_absent_extensions() {
        let (directory, _) = project();
        fs::write(directory.path().join("message.custom"), "custom text").unwrap();
        fs::write(directory.path().join("NO_EXTENSION"), "plain text").unwrap();

        assert_eq!(read_file(directory.path(), "notes.txt").unwrap(), "before");
        assert_eq!(
            read_file(directory.path(), "message.custom").unwrap(),
            "custom text"
        );
        assert_eq!(
            read_file(directory.path(), "NO_EXTENSION").unwrap(),
            "plain text"
        );
    }

    #[test]
    fn rejects_binary_content_instead_of_returning_it_as_text() {
        let (directory, _) = project();
        fs::write(directory.path().join("binary.txt"), [0, 159, 146, 150]).unwrap();

        assert_eq!(
            read_file(directory.path(), "binary.txt").unwrap_err(),
            "This binary file cannot be opened as text."
        );
    }

    #[cfg(windows)]
    #[test]
    fn tree_skips_windows_shell_shortcuts_even_though_they_are_regular_files() {
        let (directory, _) = project();
        fs::write(
            directory.path().join("Network.lnk"),
            "shell shortcut fixture",
        )
        .unwrap();

        let nodes = read_directory(directory.path(), directory.path());

        assert!(!nodes.iter().any(|node| node.name == "Network.lnk"));
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let lifecycle =
        Arc::new(ProcessLifecycle::new().expect("unable to initialize owned process lifecycle"));
    let app = tauri::Builder::default()
        .manage(ProjectState::default())
        .manage(TerminalState::default())
        .manage(OllamaState::new(Arc::clone(&lifecycle)))
        .manage(Arc::clone(&lifecycle))
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            open_project,
            refresh_project,
            read_project_file,
            save_project_file,
            create_project_file,
            delete_project_file,
            list_project_backups,
            restore_project_backup,
            execute_project_command,
            cancel_project_command,
            send_chat_message,
            load_project_history,
            save_chat_exchange,
            clear_project_history
        ])
        .setup(|app| {
            let history_path = app.path().app_data_dir()?.join("history.sqlite3");
            app.manage(HistoryStore::open(history_path).map_err(std::io::Error::other)?);
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            if uses_managed_local_ollama() {
                let app_handle = app.handle().clone();
                thread::spawn(move || {
                    if let Err(error) = app_handle.state::<OllamaState>().ensure_running() {
                        eprintln!("Managed Ollama startup failed: {error}");
                    }
                });
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");
    app.run(|app_handle, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            app_handle.state::<Arc<ProcessLifecycle>>().shutdown();
            app_handle.state::<OllamaState>().shutdown();
        }
    });
}
