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
#[derive(Default)]
struct ProjectState {
    root: Mutex<Option<PathBuf>>,
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

#[tauri::command]
async fn open_project(
    app: tauri::AppHandle,
    project_state: State<'_, ProjectState>,
) -> Result<Option<ProjectTree>, String> {
    let Some(root) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    let root = root.into_path().map_err(|error| error.to_string())?;
    diagnostic(format_args!("open_project picker path: {:?}", root));
    let root = fs::canonicalize(root).map_err(|error| error.to_string())?;
    diagnostic(format_args!(
        "open_project canonical physical path: {:?}",
        root
    ));

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

    let tree = ProjectTree { name, children };
    match serde_json::to_string(&tree) {
        Ok(json) => diagnostic(format_args!("open_project ProjectTree JSON: {json}")),
        Err(error) => diagnostic(format_args!(
            "open_project could not serialize diagnostic ProjectTree: {error}"
        )),
    }

    Ok(Some(tree))
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
    diagnostic(format_args!("read_dir start: {:?}", path));
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) => {
            diagnostic(format_args!("read_dir failed for {:?}: {error}", path));
            return nodes;
        }
    };

    for entry_result in entries {
        let entry = match entry_result {
            Ok(entry) => entry,
            Err(error) => {
                diagnostic(format_args!("read_dir yielded an entry error: {error}"));
                continue;
            }
        };
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                diagnostic(format_args!(
                    "file_type failed for {:?}: {error}",
                    entry.path()
                ));
                continue;
            }
        };
        diagnostic_entry(&entry, file_type);
        if let Some(reason) = should_skip_entry(&entry, file_type) {
            diagnostic(format_args!(
                "should_skip_entry=true path={:?} reason={reason}",
                entry.path()
            ));
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
                diagnostic(format_args!(
                    "directory filter skipped path={:?} reason=excluded directory name",
                    entry_path
                ));
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
        } else {
            diagnostic(format_args!(
                "tree skipped path={:?} reason=neither regular file nor directory",
                entry_path
            ));
        }
    }

    nodes.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    nodes
}

fn should_skip_entry(entry: &fs::DirEntry, file_type: fs::FileType) -> Option<&'static str> {
    if file_type.is_symlink() {
        return Some("filesystem symlink");
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        if entry
            .metadata()
            .is_ok_and(|metadata| metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
        {
            return Some("Windows reparse-point attribute");
        }

        // Shell shortcuts are ordinary .lnk files to std::fs, not symlinks.
        // They are navigation objects rather than project file contents.
        if file_type.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("lnk"))
        {
            return Some("Windows Shell shortcut (.lnk)");
        }
    }

    #[cfg(not(windows))]
    let _ = entry;

    None
}

fn diagnostic(arguments: std::fmt::Arguments<'_>) {
    if cfg!(debug_assertions) {
        eprintln!("[AutoCoder project diagnostic] {arguments}");
    }
}

fn diagnostic_entry(entry: &fs::DirEntry, file_type: fs::FileType) {
    diagnostic(format_args!(
        "read_dir entry name={:?} path={:?} is_file={} is_dir={} is_symlink={}",
        entry.file_name(),
        entry.path(),
        file_type.is_file(),
        file_type.is_dir(),
        file_type.is_symlink()
    ));

    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        match fs::symlink_metadata(entry.path()) {
            Ok(metadata) => {
                let attributes = metadata.file_attributes();
                diagnostic(format_args!(
                    "Windows metadata path={:?} attributes=0x{attributes:08X} reparse_point={}",
                    entry.path(),
                    attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
                ));
            }
            Err(error) => diagnostic(format_args!(
                "Windows metadata failed path={:?}: {error}",
                entry.path()
            )),
        }
    }
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
