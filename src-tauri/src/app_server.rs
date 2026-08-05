use serde_json::{json, Value};
use std::{
    env,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    sync::{
        mpsc::{self, Receiver},
        Mutex,
    },
    thread,
    time::Duration,
};
use tauri::AppHandle;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AppServerState {
    Starting,
    Handshaking,
    Ready,
    Backoff,
    Error,
}

pub struct RpcClient {
    child: Mutex<Child>,
    stdin: Mutex<ChildStdin>,
    messages: Mutex<Receiver<Value>>,
    next_id: AtomicU64,
    request_lock: Mutex<()>,
    _app: AppHandle,
}

impl RpcClient {
    pub fn spawn(app: AppHandle) -> Result<Self, String> {
        let path = resolve_codex_path()?;
        let mut child = Command::new(path)
            .args(["app-server", "--listen", "stdio://"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("APP_SERVER_START_FAILED: {error}"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "APP_SERVER_STDIN_MISSING".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "APP_SERVER_STDOUT_MISSING".to_string())?;
        if let Some(stderr) = child.stderr.take() {
            thread::spawn(move || {
                for line in BufReader::new(stderr).lines().flatten() {
                    eprintln!("codex: {}", redact(&line));
                }
            });
        }
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().flatten() {
                if let Ok(value) = serde_json::from_str::<Value>(&line) {
                    let _ = sender.send(value);
                }
            }
        });
        Ok(Self {
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            messages: Mutex::new(receiver),
            next_id: AtomicU64::new(1),
            request_lock: Mutex::new(()),
            _app: app,
        })
    }

    pub fn initialize(&self) -> Result<(), String> {
        self.request("initialize", json!({ "clientInfo": { "name": "quota_critter", "title": "Quota Critter", "version": "0.1.0" } }))?;
        self.notify("initialized", json!({}))
    }

    pub fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        let message = serde_json::to_string(&json!({ "method": method, "params": params }))
            .map_err(|error| error.to_string())?;
        let mut stdin = self
            .stdin
            .lock()
            .map_err(|_| "APP_SERVER_STDIN_LOCK_FAILED".to_string())?;
        writeln!(stdin, "{message}")
            .map_err(|error| format!("APP_SERVER_WRITE_FAILED: {error}"))?;
        stdin
            .flush()
            .map_err(|error| format!("APP_SERVER_FLUSH_FAILED: {error}"))
    }

    pub fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let _guard = self
            .request_lock
            .lock()
            .map_err(|_| "APP_SERVER_REQUEST_LOCK_FAILED".to_string())?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let message =
            serde_json::to_string(&json!({ "method": method, "id": id, "params": params }))
                .map_err(|error| error.to_string())?;
        {
            let mut stdin = self
                .stdin
                .lock()
                .map_err(|_| "APP_SERVER_STDIN_LOCK_FAILED".to_string())?;
            writeln!(stdin, "{message}")
                .map_err(|error| format!("APP_SERVER_WRITE_FAILED: {error}"))?;
            stdin
                .flush()
                .map_err(|error| format!("APP_SERVER_FLUSH_FAILED: {error}"))?;
        }
        let receiver = self
            .messages
            .lock()
            .map_err(|_| "APP_SERVER_MESSAGES_LOCK_FAILED".to_string())?;
        loop {
            let response = receiver
                .recv_timeout(Duration::from_secs(20))
                .map_err(|error| format!("REQUEST_TIMEOUT: {error}"))?;
            if response.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = response.get("error") {
                return Err(format!("APP_SERVER_RPC: {}", redact(&error.to_string())));
            }
            return Ok(response.get("result").cloned().unwrap_or_else(|| json!({})));
        }
    }
}

impl Drop for RpcClient {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn resolve_codex_path() -> Result<PathBuf, String> {
    if let Ok(path) = env::var("QUOTA_CRITTER_CODEX_PATH") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
        return Err("CODEX_NOT_FOUND".to_string());
    }
    let names = if cfg!(windows) {
        ["codex.exe", "codex"]
    } else {
        ["codex", "codex"]
    };
    if let Some(path_var) = env::var_os("PATH") {
        for directory in env::split_paths(&path_var) {
            for name in names {
                let candidate = directory.join(name);
                if candidate.is_file() {
                    return Ok(candidate);
                }
            }
        }
    }

    let mut common_paths = Vec::new();
    if let Some(home) = env::var_os("HOME").or_else(|| env::var_os("USERPROFILE")) {
        let home = PathBuf::from(home);
        common_paths.extend([
            home.join(".local/bin"),
            home.join(".volta/bin"),
            home.join(".npm-global/bin"),
            home.join("AppData/Roaming/npm"),
            home.join("scoop/shims"),
        ]);
    }
    common_paths.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
    ]);
    for directory in common_paths {
        for name in names {
            let candidate = directory.join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err("CODEX_NOT_FOUND".to_string())
}

fn redact(value: &str) -> String {
    let mut output = value.to_string();
    for key in ["accessToken", "refreshToken", "apiKey", "authUrl", "email"] {
        if let Some(index) = output.find(key) {
            let end = output[index..]
                .find(',')
                .map(|offset| index + offset)
                .unwrap_or(output.len());
            output.replace_range(index..end, &format!("{key}=<redacted>"));
        }
    }
    if let Ok(home) = env::var("HOME") {
        if !home.is_empty() {
            return output.replace(&home, "<home>");
        }
    }
    output
}
