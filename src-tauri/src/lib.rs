use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Mutex,
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

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
struct OllamaState {
    owned: Mutex<Option<Child>>,
}

impl OllamaState {
    fn ensure_running(&self) -> Result<(), String> {
        self.ensure_running_with(ollama_is_ready, launch_ollama, || {
            thread::sleep(std::time::Duration::from_millis(250))
        })
    }

    fn ensure_running_with<R, L, S>(
        &self,
        mut ready: R,
        launch: L,
        mut sleep: S,
    ) -> Result<(), String>
    where
        R: FnMut() -> bool,
        L: FnOnce() -> Result<Child, String>,
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

#[derive(Serialize)]
struct FileReadResult {
    content: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupMetadata {
    created_at_unix_ms: u128,
    original_path: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatMessage {
    role: String,
    content: String,
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
struct OpenFileContext {
    path: String,
    content: String,
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
struct ChatResponse {
    message: ChatMessage,
    proposal: Option<FileProposal>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct FileProposal {
    path: String,
    content: String,
    original_content: String,
}

#[tauri::command]
async fn open_project(
    app: tauri::AppHandle,
    project_state: State<'_, ProjectState>,
) -> Result<Option<ProjectTree>, String> {
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

    let children = read_directory(&root, &root);
    let name = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Project".to_string());

    *project_state
        .root
        .lock()
        .map_err(|_| "Unable to access the project state.".to_string())? = Some(root);

    Ok(Some(ProjectTree { name, children }))
}

#[tauri::command]
fn send_chat_message(
    app: tauri::AppHandle,
    ollama: State<'_, OllamaState>,
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
    run_chat_backend(&resource_dir, &request)
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
    ("127.0.0.1", 11434)
        .to_socket_addrs()
        .ok()
        .and_then(|mut addresses| {
            addresses
                .any(|address| {
                    TcpStream::connect_timeout(&address, std::time::Duration::from_millis(500))
                        .is_ok()
                })
                .then_some(())
        })
        .is_some()
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

fn launch_ollama() -> Result<Child, String> {
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
    configure_child_process(&mut command);
    let mut child = command.spawn().map_err(|error| {
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

fn run_chat_backend(resource_dir: &Path, request: &ChatRequest) -> Result<ChatResponse, String> {
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
    configure_child_process(&mut command);
    let mut child = command.spawn().map_err(|error| {
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
    save_file(&root, &relative_path, &content, &backup_root, timestamp)
}

fn project_root(project_state: &State<'_, ProjectState>) -> Result<PathBuf, String> {
    project_state
        .root
        .lock()
        .map_err(|_| "Unable to access the project state.".to_string())?
        .clone()
        .ok_or_else(|| "Open a project first.".to_string())
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

fn read_file(root: &Path, relative_path: &str) -> Result<String, String> {
    let path = resolve_project_file(root, relative_path)?;
    let content = fs::read(path).map_err(|error| format!("Unable to read this file: {error}"))?;
    if is_binary(&content) {
        return Err("This binary file cannot be opened as text.".to_string());
    }
    String::from_utf8(content).map_err(|error| format!("Unable to read this file as text: {error}"))
}

fn save_file(
    root: &Path,
    relative_path: &str,
    content: &str,
    backup_root: &Path,
    timestamp: u128,
) -> Result<(), String> {
    // Resolve immediately before backup and again before writing. This rejects
    // traversal and symlinks outside the project, including a path changed mid-save.
    let path = resolve_project_file(root, relative_path)?;
    let backup_dir = backup_root.join(timestamp.to_string());
    fs::create_dir_all(&backup_dir).map_err(|error| error.to_string())?;
    fs::copy(&path, backup_dir.join("content.bak"))
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
        return Err("The file path changed while it was being saved.".to_string());
    }
    fs::write(rechecked_path, content).map_err(|error| format!("Unable to save file: {error}"))
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
    use tempfile::TempDir;

    fn project() -> (TempDir, PathBuf) {
        let directory = TempDir::new().expect("temporary project");
        let file = directory.path().join("notes.txt");
        fs::write(&file, "before").expect("fixture file");
        (directory, file)
    }

    #[cfg(unix)]
    fn sleeping_child(seconds: &str) -> Child {
        Command::new("sleep")
            .arg(seconds)
            .spawn()
            .expect("sleep fixture")
    }

    #[test]
    fn existing_ollama_is_not_owned_or_stopped() {
        let state = OllamaState::default();
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
        let state = OllamaState::default();
        *state.owned.lock().unwrap() = Some(sleeping_child("30"));
        state.shutdown();
        assert!(state.owned.lock().unwrap().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn shutdown_is_safe_when_owned_ollama_already_exited() {
        let state = OllamaState::default();
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
        let state = OllamaState::default();
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
    let app = tauri::Builder::default()
        .manage(ProjectState::default())
        .manage(OllamaState::default())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            open_project,
            read_project_file,
            save_project_file,
            send_chat_message
        ])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");
    app.run(|app_handle, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            app_handle.state::<OllamaState>().shutdown();
        }
    });
}
