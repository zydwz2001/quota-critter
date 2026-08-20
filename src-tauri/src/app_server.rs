use serde_json::{json, Value};
use std::{
    env, fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    sync::{
        mpsc::{self, Receiver},
        Mutex,
    },
    thread,
    time::{Duration, SystemTime},
};
use tauri::{AppHandle, Emitter};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

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
    pub fn spawn(app: AppHandle, codex_override: Option<PathBuf>) -> Result<Self, String> {
        let path = resolve_codex_path(codex_override.as_deref())?;
        eprintln!(
            "quota-critter: using Codex runtime at {}",
            redact(&path.to_string_lossy())
        );
        let mut command = app_server_command(&path);
        let mut child = command
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
        let event_app = app.clone();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().flatten() {
                if let Ok(value) = serde_json::from_str::<Value>(&line) {
                    if value.get("method").and_then(Value::as_str)
                        == Some("account/rateLimits/updated")
                    {
                        let _ = event_app.emit("account://rate-limits-updated", ());
                    }
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
        self.request("initialize", json!({ "clientInfo": { "name": "quota_critter", "title": "Quota Critter", "version": "0.1.5" } }))?;
        self.notify("initialized", json!({}))
    }

    pub fn is_running(&self) -> bool {
        self.child
            .lock()
            .ok()
            .and_then(|mut child| child.try_wait().ok())
            .is_some_and(|status| status.is_none())
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

fn resolve_codex_path(override_path: Option<&Path>) -> Result<PathBuf, String> {
    let mut configured_path_missing = false;
    if let Some(path) = override_path {
        let trimmed = path.to_string_lossy().trim().to_string();
        if !trimmed.is_empty() {
            let candidate = PathBuf::from(&trimmed);
            if candidate.is_file() {
                return Ok(candidate);
            }
            // An extension/client update can invalidate a saved absolute path.
            // Continue auto-detection instead of permanently blocking startup.
            configured_path_missing = true;
        }
    }
    if let Ok(path) = env::var("QUOTA_CRITTER_CODEX_PATH") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
        configured_path_missing = true;
    }
    let names: &[&str] = if cfg!(windows) {
        &["codex.exe", "codex.cmd", "codex.bat", "codex.ps1", "codex"]
    } else {
        &["codex"]
    };

    // 1) PATH 顺序查找
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

    // 2) Codex VS Code extension bundles its own CLI. It is not added to the
    // desktop app's PATH, but it uses the same ~/.codex authentication state.
    // Start a separate app-server from that binary; the extension's existing
    // stdio process is private to VS Code and cannot be safely reused.
    if let Some(home) = env::var_os("HOME").or_else(|| env::var_os("USERPROFILE")) {
        let home = PathBuf::from(home);
        let mut extension_roots = vec![
            home.join(".vscode/extensions"),
            home.join(".vscode-insiders/extensions"),
            home.join(".vscode-oss/extensions"),
            home.join(".cursor/extensions"),
            home.join(".windsurf/extensions"),
        ];
        for key in ["VSCODE_EXTENSIONS", "VSCODE_EXTENSIONS_DIR"] {
            if let Some(root) = env::var_os(key) {
                extension_roots.push(PathBuf::from(root));
            }
        }
        if let Some(root) = env::var_os("VSCODE_PORTABLE") {
            extension_roots.push(PathBuf::from(root).join("data/extensions"));
        }
        if let Some(path) = find_vscode_bundled_codex(&extension_roots, names) {
            return Ok(path);
        }
    }

    // 3) Codex/ChatGPT desktop installations. Search only known product
    // roots, with a depth limit, instead of crawling all of AppData.
    #[cfg(windows)]
    {
        let mut product_roots = Vec::new();
        if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
            let root = PathBuf::from(local_app_data);
            product_roots.extend([
                root.join("Programs/Codex"),
                root.join("Programs/ChatGPT"),
                root.join("Codex"),
                root.join("ChatGPT"),
            ]);
        }
        for key in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(root) = env::var_os(key) {
                let root = PathBuf::from(root);
                product_roots.extend([root.join("Codex"), root.join("ChatGPT")]);
            }
        }
        if let Some(path) = find_codex_below(&product_roots, names, 6) {
            return Ok(path);
        }
    }

    // 4) 常见全局安装位置（npm global、scoop、cargo、volta、pipx 等）
    let mut common_paths: Vec<PathBuf> = Vec::new();
    if let Some(home) = env::var_os("HOME").or_else(|| env::var_os("USERPROFILE")) {
        let home = PathBuf::from(home);
        common_paths.extend([
            home.join(".local/bin"),
            home.join(".volta/bin"),
            home.join(".npm-global/bin"),
            home.join("AppData/Roaming/npm"),
            home.join("AppData/Local/npm"),
            home.join("scoop/shims"),
            home.join(".cargo/bin"),
            home.join(".local/pipx/venvs/codex/bin"), // pipx
        ]);
        // Windows 专属：%LOCALAPPDATA%\Programs\*（含 winget/包管理器安装位置）
        if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
            let lad = PathBuf::from(local_app_data);
            common_paths.extend([
                lad.join("Programs/codex"),
                lad.join("Programs/Codex"),
                lad.join("Microsoft/WindowsApps"),
                lad.join("PubCache/Bin"), // winget
            ]);
        }
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

    // 5) 用 shell 的 where/which 命令搜（覆盖动态 PATH 链接，比如 WindowsApps）
    #[cfg(windows)]
    {
        if let Ok(out) = std::process::Command::new("where").arg("codex").output() {
            if out.status.success() {
                for line in String::from_utf8_lossy(&out.stdout).lines() {
                    let line = line.trim();
                    if !line.is_empty() {
                        let p = PathBuf::from(line);
                        if p.is_file() {
                            return Ok(p);
                        }
                    }
                }
            }
        }
    }
    #[cfg(not(windows))]
    {
        for cmd in ["which", "command -v"] {
            if let Ok(out) = std::process::Command::new(cmd).arg("codex").output() {
                if out.status.success() {
                    for line in String::from_utf8_lossy(&out.stdout).lines() {
                        let line = line.trim();
                        if !line.is_empty() {
                            let p = PathBuf::from(line);
                            if p.is_file() {
                                return Ok(p);
                            }
                        }
                    }
                }
            }
        }
    }

    // 6) Credentials can remain after a client/extension was removed. Explain
    // that distinction without opening or parsing auth.json.
    let auth_file_exists = env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME")
                .or_else(|| env::var_os("USERPROFILE"))
                .map(|home| PathBuf::from(home).join(".codex"))
        })
        .is_some_and(|home| home.join("auth.json").is_file());
    if auth_file_exists {
        return Err("CODEX_CONFIG_FOUND_BUT_CLI_MISSING".to_string());
    }
    if configured_path_missing {
        return Err("CODEX_CONFIGURED_PATH_MISSING".to_string());
    }
    Err("CODEX_NOT_FOUND".to_string())
}

fn find_codex_below(roots: &[PathBuf], names: &[&str], max_depth: usize) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    let mut pending: Vec<(PathBuf, usize)> = roots
        .iter()
        .filter(|root| root.is_dir())
        .cloned()
        .map(|root| (root, 0))
        .collect();

    while let Some((directory, depth)) = pending.pop() {
        for name in names {
            push_codex_candidate(&mut candidates, directory.join(name), true);
        }
        if depth >= max_depth {
            continue;
        }
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
                pending.push((entry.path(), depth + 1));
            }
        }
    }

    candidates.sort_by(|left, right| right.1.cmp(&left.1));
    candidates.into_iter().next().map(|(_, _, path)| path)
}

fn find_vscode_bundled_codex(extension_roots: &[PathBuf], names: &[&str]) -> Option<PathBuf> {
    let mut candidates: Vec<(bool, SystemTime, PathBuf)> = Vec::new();

    for root in extension_roots {
        let Ok(extensions) = fs::read_dir(root) else {
            continue;
        };
        for extension in extensions.flatten() {
            let extension_name = extension.file_name();
            let extension_name = extension_name.to_string_lossy().to_ascii_lowercase();
            if extension_name != "openai.chatgpt" && !extension_name.starts_with("openai.chatgpt-")
            {
                continue;
            }

            let bin = extension.path().join("bin");
            for name in names {
                push_codex_candidate(&mut candidates, bin.join(name), true);
            }

            let Ok(targets) = fs::read_dir(&bin) else {
                continue;
            };
            for target in targets.flatten() {
                let Ok(file_type) = target.file_type() else {
                    continue;
                };
                if !file_type.is_dir() {
                    continue;
                }
                let target_name = target.file_name();
                let target_name = target_name.to_string_lossy().to_ascii_lowercase();
                if !target_matches_current_platform(&target_name) {
                    continue;
                }
                let architecture_match = target_matches_current_architecture(&target_name);
                for name in names {
                    push_codex_candidate(
                        &mut candidates,
                        target.path().join(name),
                        architecture_match,
                    );
                }
            }
        }
    }

    candidates.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| right.1.cmp(&left.1)));
    candidates.into_iter().next().map(|(_, _, path)| path)
}

fn push_codex_candidate(
    candidates: &mut Vec<(bool, SystemTime, PathBuf)>,
    path: PathBuf,
    architecture_match: bool,
) {
    if !path.is_file() {
        return;
    }
    let modified = path
        .metadata()
        .and_then(|metadata| metadata.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH);
    candidates.push((architecture_match, modified, path));
}

fn target_matches_current_platform(target: &str) -> bool {
    if cfg!(target_os = "windows") {
        target.contains("windows")
    } else if cfg!(target_os = "macos") {
        target.contains("darwin") || target.contains("macos")
    } else {
        target.contains("linux")
    }
}

fn app_server_command(path: &Path) -> Command {
    #[cfg(windows)]
    {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let mut command = match extension.as_str() {
            "cmd" | "bat" => {
                let mut command = Command::new("cmd.exe");
                command.args(["/d", "/s", "/c"]);
                command.arg(format!(
                    "\"{}\" app-server --listen stdio://",
                    path.to_string_lossy()
                ));
                command
            }
            "ps1" => {
                let mut command = Command::new("powershell.exe");
                command.args([
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                ]);
                command.arg(path);
                command.args(["app-server", "--listen", "stdio://"]);
                command
            }
            _ => {
                let mut command = Command::new(path);
                command.args(["app-server", "--listen", "stdio://"]);
                command
            }
        };
        command.creation_flags(CREATE_NO_WINDOW);
        command
    }

    #[cfg(not(windows))]
    {
        let mut command = Command::new(path);
        command.args(["app-server", "--listen", "stdio://"]);
        command
    }
}

fn target_matches_current_architecture(target: &str) -> bool {
    match env::consts::ARCH {
        "x86_64" => target.contains("x86_64") || target.contains("x64"),
        "aarch64" => target.contains("aarch64") || target.contains("arm64"),
        architecture => target.contains(architecture),
    }
}

fn redact(value: &str) -> String {
    let mut output = value.to_string();
    for key in ["accessToken", "refreshToken", "apiKey", "authUrl", "email"] {
        let mut cursor = 0usize;
        while let Some(relative) = output.get(cursor..).and_then(|rest| rest.find(key)) {
            let index = cursor + relative;
            // 优先在 JSON 标点（, } ] 换行）处截断；
            // 若直到行尾都没遇到（说明这是日志最后一段），最多截 200 字符，避免整行泄露。
            let end = [',', '}', ']', '\n']
                .iter()
                .filter_map(|ch| output[index..].find(*ch))
                .min()
                .map(|offset| index + offset)
                .unwrap_or_else(|| (index + 200).min(output.len()));
            output.replace_range(index..end, &format!("{key}=<redacted>"));
            cursor = index + 1;
        }
    }
    if let Ok(home) = env::var("HOME") {
        if !home.is_empty() {
            return output.replace(&home, "<home>");
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_codex_bundled_with_the_openai_vscode_extension() {
        let root = std::env::temp_dir().join(format!(
            "quota-critter-vscode-extension-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let target = if cfg!(target_os = "windows") {
            "windows-x86_64"
        } else if cfg!(target_os = "macos") {
            "darwin-x86_64"
        } else {
            "linux-x86_64"
        };
        let executable = if cfg!(target_os = "windows") {
            "codex.exe"
        } else {
            "codex"
        };
        let expected = root
            .join("openai.chatgpt-99.0.0")
            .join("bin")
            .join(target)
            .join(executable);
        fs::create_dir_all(expected.parent().expect("binary parent")).expect("create fixture");
        fs::write(&expected, []).expect("create binary fixture");

        let found = find_vscode_bundled_codex(&[root.clone()], &[executable]);
        assert_eq!(found.as_deref(), Some(expected.as_path()));

        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn finds_codex_inside_a_nested_desktop_installation() {
        let root = std::env::temp_dir().join(format!(
            "quota-critter-desktop-runtime-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let executable = if cfg!(target_os = "windows") {
            "codex.exe"
        } else {
            "codex"
        };
        let expected = root
            .join("app-1.2.3")
            .join("resources")
            .join("bin")
            .join(executable);
        fs::create_dir_all(expected.parent().expect("binary parent")).expect("create fixture");
        fs::write(&expected, []).expect("create binary fixture");

        let found = find_codex_below(&[root.clone()], &[executable], 4);
        assert_eq!(found.as_deref(), Some(expected.as_path()));

        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[cfg(windows)]
    #[test]
    fn launches_npm_cmd_wrappers_through_cmd_exe() {
        let command = app_server_command(Path::new(r"C:\Program Files\Codex\codex.cmd"));
        assert_eq!(command.get_program(), "cmd.exe");
        let args = command
            .get_args()
            .map(|value| value.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert!(args
            .iter()
            .any(|value| value.contains("app-server --listen stdio://")));
    }

    // 之前 bug：原实现用 `if let Some(...)` 只替换首次出现。
    // 同一行日志出现多个 token 时，只有第一个被脱敏，会泄露后续的。
    #[test]
    fn redacts_all_occurrences_of_each_key() {
        let input = r#"{"accessToken":"abc","refreshToken":"def","accessToken":"ghi"}"#;
        let out = redact(input);
        assert!(!out.contains("abc"), "first accessToken leaked: {out}");
        assert!(!out.contains("def"), "refreshToken leaked: {out}");
        assert!(!out.contains("ghi"), "second accessToken leaked: {out}");
        assert_eq!(
            out.matches("<redacted>").count(),
            3,
            "should redact 3 values: {out}"
        );
    }

    #[test]
    fn redacts_keys_inside_nested_json() {
        let input = r#"outer={"apiKey":"k1","child":{"apiKey":"k2","other":"k3"}}"#;
        let out = redact(input);
        assert!(!out.contains("k1"));
        assert!(!out.contains("k2"));
        assert!(
            out.contains("k3"),
            "non-sensitive field should survive: {out}"
        );
    }

    #[test]
    fn truncates_long_values_without_punctuation_boundary() {
        // value 末尾没有标点，靠 200 字符上限截断
        let long_secret = "x".repeat(500);
        let input = format!(r#"{{"authUrl":"{long_secret}""#);
        let out = redact(&input);
        assert!(!out.contains(&long_secret));
        assert!(out.contains("authUrl=<redacted>"));
    }

    #[test]
    fn respects_punctuation_boundary_when_present() {
        let input = r#"{"email":"foo@bar","other":"baz"}"#;
        let out = redact(input);
        assert!(!out.contains("foo@bar"));
        assert!(out.contains("\"other\":\"baz\""));
    }

    #[test]
    fn leaves_input_untouched_when_no_sensitive_key_present() {
        let input = r#"{"latencyMs":42,"window":"5h"}"#;
        assert_eq!(redact(input), input);
    }

    #[test]
    fn redacts_home_path() {
        // 用唯一 marker 临时设 HOME，输入里包含它才能触发替换
        let marker = "/qc-test-home-marker-9c4d";
        let prev = env::var("HOME").ok();
        // SAFETY: cargo test 默认单线程，set_var 不会并发冲突
        unsafe {
            env::set_var("HOME", marker);
        }
        let out = redact(&format!("opened {marker}/.config/codex/log.txt"));
        match prev {
            Some(v) => unsafe {
                env::set_var("HOME", v);
            },
            None => unsafe {
                env::remove_var("HOME");
            },
        }
        assert!(out.contains("<home>"), "HOME path not redacted: {out}");
        assert!(!out.contains(marker), "HOME marker leaked: {out}");
    }
}
