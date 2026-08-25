use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;

const EXCLUDED_DIRECTORY_NAMES: &[&str] = &[".git", "node_modules", "target", ".venv"];

#[derive(Default)]
struct ProjectState {
    root: Mutex<Option<PathBuf>>,
}

#[derive(Clone, Copy, Ord, PartialOrd, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum FileTreeNodeKind {
    Directory,
    File,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileTreeNode {
    name: String,
    path: String,
    kind: FileTreeNodeKind,
    children: Vec<FileTreeNode>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectTree {
    name: String,
    children: Vec<FileTreeNode>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupMetadata {
    created_at_unix_ms: u128,
    original_path: String,
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
fn read_project_file(
    relative_path: String,
    project_state: State<'_, ProjectState>,
) -> Result<String, String> {
    let path = resolve_project_file(&relative_path, &project_state)?;
    fs::read_to_string(path).map_err(|error| format!("Unable to read this file as text: {error}"))
}

#[tauri::command]
fn save_project_file(
    app: tauri::AppHandle,
    relative_path: String,
    content: String,
    project_state: State<'_, ProjectState>,
) -> Result<(), String> {
    // Resolve and canonicalize again immediately before backup/write. This prevents
    // relative traversal and rejects symlinks whose target is outside the project.
    let path = resolve_project_file(&relative_path, &project_state)?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let backup_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("backups")
        .join(timestamp.to_string());
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

    let rechecked_path = resolve_project_file(&relative_path, &project_state)?;
    if rechecked_path != path {
        return Err("The file path changed while it was being saved.".to_string());
    }
    fs::write(rechecked_path, content).map_err(|error| format!("Unable to save file: {error}"))
}

fn resolve_project_file(
    relative_path: &str,
    project_state: &State<'_, ProjectState>,
) -> Result<PathBuf, String> {
    let root = project_state
        .root
        .lock()
        .map_err(|_| "Unable to access the project state.".to_string())?
        .clone()
        .ok_or_else(|| "Open a project first.".to_string())?;
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

fn read_directory(root: &Path, path: &Path) -> Vec<FileTreeNode> {
    let mut nodes = Vec::new();
    let Ok(entries) = fs::read_dir(path) else {
        return nodes;
    };

    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
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
            if EXCLUDED_DIRECTORY_NAMES.contains(&name.as_str()) {
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(ProjectState::default())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            open_project,
            read_project_file,
            save_project_file
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
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
