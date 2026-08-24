use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
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
    kind: FileTreeNodeKind,
    children: Vec<FileTreeNode>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectTree {
    name: String,
    children: Vec<FileTreeNode>,
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

    let metadata = fs::metadata(&root).map_err(|error| error.to_string())?;
    if !metadata.is_dir() {
        return Err("The selected path is not a directory.".to_string());
    }
    fs::read_dir(&root).map_err(|error| error.to_string())?;

    let children = read_directory(&root);
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

fn read_directory(path: &Path) -> Vec<FileTreeNode> {
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

        if file_type.is_dir() {
            if EXCLUDED_DIRECTORY_NAMES.contains(&name.as_str()) {
                continue;
            }

            nodes.push(FileTreeNode {
                name,
                kind: FileTreeNodeKind::Directory,
                children: read_directory(&entry_path),
            });
        } else if file_type.is_file() {
            nodes.push(FileTreeNode {
                name,
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
        .invoke_handler(tauri::generate_handler![open_project])
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
