//! Passive, local development diagnostics. All public operations deliberately
//! swallow write/serialization failures: observability must never become a
//! business-logic dependency.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    fs,
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_EVENT_BYTES: usize = 32 * 1024;
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
const RETAIN_FILES: usize = 5;
const REQUIRED_BOUNDARIES: &[&str] = &[
    "frontend",
    "tauri",
    "python",
    "provider",
    "orchestration",
    "file-tool",
    "terminal-tool",
    "filesystem",
    "editor",
    "workspace",
    "persistence",
    "backup-restore",
    "process",
    "cancellation",
    "recovery",
];

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticEventInput {
    pub subsystem: String,
    pub component: String,
    pub event_type: String,
    #[serde(default = "default_severity")]
    pub severity: String,
    pub trace_id: String,
    pub span_id: String,
    #[serde(default)]
    pub parent_span_id: Option<String>,
    #[serde(default)]
    pub data: Value,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<Value>,
    #[serde(default)]
    pub state_transition: Option<Value>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
}

fn default_severity() -> String {
    "info".into()
}

impl DiagnosticEventInput {
    pub fn lifecycle(event_type: &str) -> Self {
        Self {
            subsystem: "tauri".into(),
            component: "application".into(),
            event_type: event_type.into(),
            severity: "info".into(),
            trace_id: new_id(),
            span_id: new_id(),
            parent_span_id: None,
            data: json!({}),
            result: None,
            error: None,
            state_transition: None,
            duration_ms: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticEvent {
    pub timestamp: String,
    pub subsystem: String,
    pub component: String,
    pub event_type: String,
    pub severity: String,
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub data: Value,
    pub result: Option<Value>,
    pub error: Option<Value>,
    pub state_transition: Option<Value>,
    pub duration_ms: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageReport {
    pub known_components: Vec<String>,
    pub observed_components: Vec<String>,
    pub uncovered_components: Vec<String>,
}

pub struct Diagnostics {
    directory: PathBuf,
    events: Mutex<Vec<DiagnosticEvent>>,
}

impl Diagnostics {
    pub fn open(directory: PathBuf) -> Self {
        let _ = fs::create_dir_all(&directory);
        let events = read_existing(&directory);
        let this = Self {
            directory,
            events: Mutex::new(events),
        };
        this.rotate();
        this
    }

    pub fn record(&self, input: DiagnosticEventInput) {
        let event = DiagnosticEvent {
            timestamp: timestamp(),
            subsystem: clean_text(&input.subsystem, 80),
            component: clean_text(&input.component, 120),
            event_type: clean_text(&input.event_type, 120),
            severity: clean_text(&input.severity, 20),
            trace_id: clean_text(&input.trace_id, 128),
            span_id: clean_text(&input.span_id, 128),
            parent_span_id: input.parent_span_id.map(|v| clean_text(&v, 128)),
            data: sanitize(input.data, 0),
            result: input.result.map(|v| sanitize(v, 0)),
            error: input.error.map(|v| sanitize(v, 0)),
            state_transition: input.state_transition.map(|v| sanitize(v, 0)),
            duration_ms: input.duration_ms,
        };
        let Ok(mut encoded) = serde_json::to_vec(&event) else {
            return;
        };
        if encoded.len() > MAX_EVENT_BYTES {
            encoded = serde_json::to_vec(&DiagnosticEvent {
                data: json!({"truncated": true, "originalBytes": encoded.len()}),
                ..event.clone()
            })
            .unwrap_or_default();
        }
        encoded.push(b'\n');
        let path = self.directory.join("current.jsonl");
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = file.write_all(&encoded);
        }
        if let Ok(mut events) = self.events.lock() {
            events.push(event);
            if events.len() > 5000 {
                events.drain(..1000);
            }
        }
        self.rotate();
    }

    pub fn events(&self) -> Vec<DiagnosticEvent> {
        self.events.lock().map(|v| v.clone()).unwrap_or_default()
    }

    pub fn coverage(&self) -> CoverageReport {
        let observed: BTreeSet<String> = self
            .events()
            .into_iter()
            .flat_map(|e| [e.subsystem, e.component])
            .collect();
        let known: BTreeSet<String> = REQUIRED_BOUNDARIES
            .iter()
            .map(|v| (*v).to_string())
            .collect();
        CoverageReport {
            known_components: known.iter().cloned().collect(),
            observed_components: observed.iter().cloned().collect(),
            uncovered_components: known.difference(&observed).cloned().collect(),
        }
    }

    pub fn export_bundle(&self) -> Result<PathBuf, String> {
        let path = self
            .directory
            .join(format!("diagnostic-bundle-{}.json", now_ms()));
        let value = json!({"formatVersion": 1, "createdAt": timestamp(), "events": self.events(), "coverage": self.coverage(), "privacy": {"localOnly": true, "redacted": true}});
        fs::write(
            &path,
            serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        Ok(path)
    }

    fn rotate(&self) {
        let current = self.directory.join("current.jsonl");
        if fs::metadata(&current).map(|m| m.len()).unwrap_or(0) > MAX_FILE_BYTES {
            let _ = fs::rename(
                &current,
                self.directory.join(format!("events-{}.jsonl", now_ms())),
            );
        }
        let mut old: Vec<_> = fs::read_dir(&self.directory)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("events-") && n.ends_with(".jsonl"))
            })
            .collect();
        old.sort();
        while old.len() > RETAIN_FILES {
            let _ = fs::remove_file(old.remove(0));
        }
    }
}

fn read_existing(directory: &PathBuf) -> Vec<DiagnosticEvent> {
    let mut files: Vec<_> = fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .collect();
    files.sort();
    let mut result = Vec::new();
    for path in files {
        if let Ok(text) = fs::read_to_string(path) {
            result.extend(
                text.lines()
                    .filter_map(|line| serde_json::from_str(line).ok()),
            );
        }
    }
    result
        .into_iter()
        .rev()
        .take(5000)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn sanitize(value: Value, depth: usize) -> Value {
    if depth > 8 {
        return json!("[depth-limited]");
    }
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| {
                    let lower = k.to_lowercase();
                    let secret = [
                        "token",
                        "secret",
                        "password",
                        "authorization",
                        "api_key",
                        "apikey",
                        "content",
                    ]
                    .iter()
                    .any(|needle| lower.contains(needle));
                    (
                        k,
                        if secret {
                            json!("[REDACTED]")
                        } else {
                            sanitize(v, depth + 1)
                        },
                    )
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(
            values
                .into_iter()
                .take(100)
                .map(|v| sanitize(v, depth + 1))
                .collect(),
        ),
        Value::String(v) => json!(clean_text(&v, 4096)),
        other => other,
    }
}
fn clean_text(value: &str, max: usize) -> String {
    let mut v = value.replace('\0', "");
    if v.len() > max {
        v.truncate(v.floor_char_boundary(max));
        v.push_str("…[truncated]");
    }
    v
}
fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
fn timestamp() -> String {
    format!("{}Z", now_ms())
}
fn new_id() -> String {
    format!("diag-{}", now_ms())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn correlation_and_redaction_are_structured() {
        let d = Diagnostics::open(tempfile::tempdir().unwrap().path().into());
        d.record(DiagnosticEventInput {
            subsystem: "frontend".into(),
            component: "editor".into(),
            event_type: "save".into(),
            severity: "info".into(),
            trace_id: "run-1".into(),
            span_id: "op-1".into(),
            parent_span_id: Some("task-1".into()),
            data: json!({"password":"oops","nested":{"content":"source"}}),
            result: None,
            error: None,
            state_transition: Some(json!({"from":"dirty","to":"saved"})),
            duration_ms: Some(4),
        });
        let e = d.events().pop().unwrap();
        assert_eq!(e.trace_id, "run-1");
        assert_eq!(e.parent_span_id.as_deref(), Some("task-1"));
        assert_eq!(e.data["password"], "[REDACTED]");
        assert_eq!(e.data["nested"]["content"], "[REDACTED]");
    }
    #[test]
    fn failed_storage_is_isolated() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("not-directory");
        fs::write(&file, "x").unwrap();
        let d = Diagnostics::open(file);
        d.record(DiagnosticEventInput::lifecycle("test"));
        assert_eq!(d.events().len(), 1);
    }
    #[test]
    fn retention_and_bundle_are_bounded_and_exportable() {
        let temp = tempfile::tempdir().unwrap();
        for i in 0..8 {
            fs::write(temp.path().join(format!("events-{i}.jsonl")), "").unwrap();
        }
        let d = Diagnostics::open(temp.path().into());
        d.rotate();
        assert!(fs::read_dir(temp.path()).unwrap().flatten().count() <= 5);
        let bundle = d.export_bundle().unwrap();
        assert!(
            serde_json::from_slice::<Value>(&fs::read(bundle).unwrap()).unwrap()["coverage"]
                .is_object()
        );
    }
}
