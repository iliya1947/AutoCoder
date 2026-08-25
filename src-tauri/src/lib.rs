use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
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
const TEXT_EXTENSIONS: &[&str] = &[
    "c", "cc", "cpp", "cs", "css", "go", "h", "hpp", "html", "java", "js", "jsx", "json", "md",
    "php", "py", "rb", "rs", "sh", "sql", "toml", "ts", "tsx", "txt", "xml", "yaml", "yml",
];
const TEXT_FILE_NAMES: &[&str] = &["dockerfile", "license", "makefile", "readme"];

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
struct FileReadResult {
    content: String,
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
    fs::read_to_string(path).map_err(|error| format!("Unable to read this file as text: {error}"))
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

fn is_supported_text_file(name: &str) -> bool {
    let normalized = name.to_lowercase();
    if TEXT_FILE_NAMES.contains(&normalized.as_str()) {
        return true;
    }
    Path::new(&normalized)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| TEXT_EXTENSIONS.contains(&extension))
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
            if is_excluded_directory(&name) {
                continue;
            }
            nodes.push(FileTreeNode {
                name,
                path: relative_path,
                kind: FileTreeNodeKind::Directory,
                children: read_directory(root, &entry_path),
            });
        } else if file_type.is_file() && is_supported_text_file(&name) {
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
    fn tree_excludes_hidden_directories_and_unsupported_files() {
        let (directory, _) = project();
        fs::create_dir(directory.path().join(".cache")).unwrap();
        fs::write(directory.path().join("image.png"), "not really an image").unwrap();
        fs::write(directory.path().join("README"), "read me").unwrap();

        let nodes = read_directory(directory.path(), directory.path());
        let names: Vec<_> = nodes.iter().map(|node| node.name.as_str()).collect();

        assert_eq!(names, ["notes.txt", "README"]);
    }
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
