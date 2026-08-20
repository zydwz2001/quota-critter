mod app_server;
mod quota;
mod settings;

use app_server::{AppServerState, RpcClient};
use quota::{fetch_quota, QuotaSnapshot};
use serde_json::json;
use settings::{AppSettings, SettingsStore};
use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex, RwLock,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, WindowEvent,
};
use tauri_plugin_autostart::ManagerExt;

pub struct Core {
    app: AppHandle,
    client: Mutex<Option<Arc<RpcClient>>>,
    connect_lock: Mutex<()>,
    snapshot: RwLock<Option<QuotaSnapshot>>,
    settings: SettingsStore,
    cache_path: PathBuf,
    refresh_lock: Mutex<()>,
    visible: AtomicBool,
    last_refresh: Mutex<u64>,
    last_attempt: Mutex<u64>,
    consecutive_errors: AtomicU32,
    last_position: Mutex<Option<settings::WindowPlacement>>,
}

#[derive(Clone)]
pub struct AppState(pub Arc<Core>);

impl Core {
    fn new(app: AppHandle, settings_path: PathBuf) -> Result<Arc<Self>, String> {
        let cache_path = settings_path.with_file_name("quota-cache.json");
        let cached_snapshot = load_cached_snapshot(&cache_path);
        let last_refresh = cached_snapshot.as_ref().map(|s| s.fetched_at).unwrap_or(0);
        Ok(Arc::new(Self {
            app,
            client: Mutex::new(None),
            connect_lock: Mutex::new(()),
            snapshot: RwLock::new(cached_snapshot),
            settings: SettingsStore::load(settings_path)?,
            cache_path,
            refresh_lock: Mutex::new(()),
            visible: AtomicBool::new(true),
            last_refresh: Mutex::new(last_refresh),
            last_attempt: Mutex::new(0),
            consecutive_errors: AtomicU32::new(0),
            last_position: Mutex::new(None),
        }))
    }

    fn emit_server(&self, state: AppServerState) {
        let _ = self.app.emit("app-server://state", state);
    }

    fn emit_auth(&self, state: &str) {
        let _ = self.app.emit("account://state", state.to_string());
    }

    fn bootstrap(self: &Arc<Self>) {
        let core = Arc::clone(self);
        thread::spawn(move || {
            if let Err(error) = core.refresh_quota_internal() {
                core.emit_server(AppServerState::Error);
                eprintln!("initial quota refresh failed: {error}");
            }
            // Keep retrying when a client/extension is installed, updated, or
            // signed in after Quota Critter has already started.
            core.run_scheduler();
        });
    }

    fn client(&self) -> Result<Arc<RpcClient>, String> {
        let existing = {
            let mut slot = self
                .client
                .lock()
                .map_err(|_| "APP_SERVER_LOCK_FAILED".to_string())?;
            match slot.as_ref() {
                Some(client) if client.is_running() => Some(Arc::clone(client)),
                Some(_) => {
                    *slot = None;
                    None
                }
                None => None,
            }
        };
        match existing {
            Some(client) => Ok(client),
            None => self.connect_client(),
        }
    }

    fn connect_client(&self) -> Result<Arc<RpcClient>, String> {
        let _guard = self
            .connect_lock
            .lock()
            .map_err(|_| "APP_SERVER_CONNECT_LOCK_FAILED".to_string())?;
        if let Some(client) = self
            .client
            .lock()
            .map_err(|_| "APP_SERVER_LOCK_FAILED".to_string())?
            .as_ref()
            .filter(|client| client.is_running())
            .cloned()
        {
            return Ok(client);
        }

        self.emit_server(AppServerState::Starting);
        let override_path = self
            .settings
            .get()
            .ok()
            .and_then(|settings| settings.codex_override)
            .map(PathBuf::from);
        let client = RpcClient::spawn(self.app.clone(), override_path)?;
        self.emit_server(AppServerState::Handshaking);
        client.initialize()?;
        let client = Arc::new(client);
        *self
            .client
            .lock()
            .map_err(|_| "APP_SERVER_LOCK_FAILED".to_string())? = Some(Arc::clone(&client));
        self.emit_server(AppServerState::Ready);
        Ok(client)
    }

    fn clear_client(&self) {
        if let Ok(mut slot) = self.client.lock() {
            *slot = None;
        }
    }

    fn refresh_quota_internal(&self) -> Result<QuotaSnapshot, String> {
        let _guard = self
            .refresh_lock
            .lock()
            .map_err(|_| "REFRESH_LOCK_FAILED".to_string())?;
        if let Ok(mut slot) = self.last_attempt.lock() {
            *slot = now_millis();
        }
        let mut retried_connection = false;
        let result = loop {
            let client = match self.client() {
                Ok(client) => client,
                Err(error) => break Err(error),
            };
            match fetch_quota(&client) {
                Ok(snapshot) => break Ok(snapshot),
                Err(error) if !retried_connection && is_connection_error(&error) => {
                    retried_connection = true;
                    self.clear_client();
                }
                Err(error) => break Err(error),
            }
        };
        let snapshot = match result {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.record_refresh_error(&error);
                return Err(error);
            }
        };
        if let Ok(mut slot) = self.snapshot.write() {
            *slot = Some(snapshot.clone());
        }
        let _ = persist_snapshot(&self.cache_path, &snapshot);
        let _ = self.app.emit("quota://snapshot", snapshot.clone());
        self.emit_auth("chatgpt");
        if let Ok(mut slot) = self.last_refresh.lock() {
            *slot = snapshot.fetched_at;
        }
        self.consecutive_errors.store(0, Ordering::Relaxed);
        self.emit_server(AppServerState::Ready);
        Ok(snapshot)
    }

    fn record_refresh_error(&self, error: &str) {
        self.consecutive_errors.fetch_add(1, Ordering::Relaxed);
        if error.contains("AUTH_REQUIRED") {
            self.emit_auth("signedOut");
        } else if error.contains("AUTH_UNSUPPORTED") {
            self.emit_auth("unsupported");
        }

        let stale_snapshot = self.snapshot.write().ok().and_then(|mut slot| {
            let snapshot = slot.as_mut()?;
            snapshot.source = "cache".into();
            snapshot.stale = true;
            Some(snapshot.clone())
        });
        if let Some(snapshot) = stale_snapshot {
            let _ = self.app.emit("quota://snapshot", snapshot);
        }
    }

    fn start_login(&self) -> Result<(), String> {
        let client = self.client()?;
        let result = client.request(
            "account/login/start",
            json!({
                "type": "chatgpt",
                "useHostedLoginSuccessPage": true,
                "appBrand": "chatgpt"
            }),
        )?;
        let url = result
            .get("authUrl")
            .and_then(|value| value.as_str())
            .ok_or_else(|| "AUTH_URL_MISSING".to_string())?;
        open_url(url)
    }

    fn run_scheduler(self: &Arc<Self>) {
        loop {
            thread::sleep(Duration::from_secs(5));

            let settings = match self.settings.get() {
                Ok(value) => value,
                Err(_) => continue,
            };

            let visible = self.visible.load(Ordering::Relaxed);
            let base_interval = if visible {
                settings.refresh_seconds_visible
            } else {
                settings.refresh_seconds_hidden
            };

            let errors = self.consecutive_errors.load(Ordering::Relaxed);
            let backoff_multiplier = 1u64 << errors.saturating_sub(1).min(4);
            let interval_secs = base_interval.saturating_mul(backoff_multiplier).min(300);

            let last = *self
                .last_attempt
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let elapsed_secs = now_millis().saturating_sub(last) / 1000;

            if elapsed_secs >= interval_secs {
                if let Err(error) = self.refresh_quota_internal() {
                    if is_connection_error(&error) {
                        self.clear_client();
                    }
                    self.emit_server(AppServerState::Backoff);
                    eprintln!("scheduled quota refresh failed: {error}");
                }
            }
        }
    }

    fn update_last_position(&self, x: i32, y: i32) {
        if let Ok(mut slot) = self.last_position.lock() {
            *slot = Some(settings::WindowPlacement {
                monitor_id: None,
                x,
                y,
            });
        }
    }

    fn save_window_placement(&self, window: &tauri::Window) {
        let placement = self
            .last_position
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
            .or_else(|| {
                window
                    .outer_position()
                    .ok()
                    .map(|position| settings::WindowPlacement {
                        monitor_id: None,
                        x: position.x,
                        y: position.y,
                    })
            });

        if let Some(placement) = placement {
            if let Ok(current) = self.settings.get() {
                let mut next = current.clone();
                next.window_placement = Some(placement);
                let _ = self.settings.set(next);
            }
        }
    }

    fn should_refresh_on_show(&self, settings: &AppSettings) -> bool {
        let last = *self
            .last_refresh
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let elapsed_secs = now_millis().saturating_sub(last) / 1000;
        elapsed_secs >= settings.refresh_seconds_visible
    }
}

fn load_cached_snapshot(path: &PathBuf) -> Option<QuotaSnapshot> {
    let text = fs::read_to_string(path).ok()?;
    let mut snapshot = serde_json::from_str::<QuotaSnapshot>(&text).ok()?;
    snapshot.source = "cache".into();
    snapshot.stale = now_millis().saturating_sub(snapshot.fetched_at) > 24 * 60 * 60 * 1000;
    Some(snapshot)
}

fn persist_snapshot(path: &PathBuf, snapshot: &QuotaSnapshot) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "CACHE_PATH_INVALID".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("CACHE_DIR_FAILED: {error}"))?;
    let bytes = serde_json::to_vec_pretty(snapshot)
        .map_err(|error| format!("CACHE_SERIALIZE_FAILED: {error}"))?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, bytes).map_err(|error| format!("CACHE_WRITE_FAILED: {error}"))?;
    fs::rename(&temporary, path).map_err(|error| format!("CACHE_RENAME_FAILED: {error}"))
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[tauri::command]
fn get_quota_snapshot(state: State<'_, AppState>) -> Result<Option<QuotaSnapshot>, String> {
    state
        .0
        .snapshot
        .read()
        .map(|snapshot| snapshot.clone())
        .map_err(|_| "SNAPSHOT_LOCK_FAILED".to_string())
}

#[tauri::command]
fn refresh_quota(state: State<'_, AppState>) -> Result<QuotaSnapshot, String> {
    state.0.refresh_quota_internal()
}

#[tauri::command]
fn start_login(state: State<'_, AppState>) -> Result<(), String> {
    state.0.start_login()
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    state.0.settings.get()
}

#[tauri::command]
fn set_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    state.0.settings.set(settings.clone())?;
    let autostart = app.autolaunch();
    if settings.launch_at_login {
        autostart
            .enable()
            .map_err(|error| format!("AUTOSTART_ENABLE: {error}"))?;
    } else {
        autostart
            .disable()
            .map_err(|error| format!("AUTOSTART_DISABLE: {error}"))?;
    }
    Ok(settings)
}

#[tauri::command]
fn set_always_on_top(app: AppHandle, value: bool) -> Result<(), String> {
    app.get_webview_window("main")
        .ok_or_else(|| "MAIN_WINDOW_MISSING".to_string())?
        .set_always_on_top(value)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    // 安全校验：只允许 https/http URL，防止命令注入到 `cmd /C start` / `open` / `xdg-open`
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("UNSAFE_URL".to_string());
    }
    if url.contains('\n') || url.contains('\r') || url.contains('"') {
        return Err("UNSAFE_URL".to_string());
    }
    open_url(&url)
}

#[tauri::command]
fn set_widget_size(app: AppHandle, width: f64, height: f64) -> Result<(), String> {
    let width = width.clamp(260.0, 500.0);
    let height = height.clamp(100.0, 600.0);
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "MAIN_WINDOW_MISSING".to_string())?;
    window
        .set_size(tauri::Size::Logical(tauri::LogicalSize::new(width, height)))
        .map_err(|error| error.to_string())?;

    // Expanding a widget parked near the bottom/right edge used to put part of
    // the panel outside the visible desktop. Keep the resized window inside
    // the current monitor's work area (which excludes the taskbar/dock).
    if let (Ok(position), Ok(size), Ok(Some(monitor))) = (
        window.outer_position(),
        window.outer_size(),
        window.current_monitor(),
    ) {
        let work_area = monitor.work_area();
        let min_x = work_area.position.x;
        let min_y = work_area.position.y;
        let max_x = min_x
            .saturating_add(work_area.size.width as i32)
            .saturating_sub(size.width as i32);
        let max_y = min_y
            .saturating_add(work_area.size.height as i32)
            .saturating_sub(size.height as i32);
        let x = position.x.clamp(min_x, max_x.max(min_x));
        let y = position.y.clamp(min_y, max_y.max(min_y));
        if x != position.x || y != position.y {
            window
                .set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
                    x, y,
                )))
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn is_connection_error(error: &str) -> bool {
    error.contains("APP_SERVER_WRITE_FAILED")
        || error.contains("APP_SERVER_FLUSH_FAILED")
        || error.contains("APP_SERVER_STDIN")
        || error.contains("APP_SERVER_MESSAGES")
        || error.contains("APP_SERVER_RPC")
        || error.contains("REQUEST_TIMEOUT")
        || error.contains("RATE_LIMITS_EMPTY")
}

fn setup_tray(app: &tauri::App, core: Arc<Core>) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show / Hide", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "Refresh quota", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &refresh, &settings, &quit])?;

    let menu_core = Arc::clone(&core);
    let click_core = Arc::clone(&core);

    TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("Quota Critter")
        .on_menu_event(move |app, event| {
            if let Some(window) = app.get_webview_window("main") {
                match event.id.as_ref() {
                    "show" => {
                        if window.is_visible().unwrap_or(false) {
                            menu_core.visible.store(false, Ordering::Relaxed);
                            let _ = window.hide();
                        } else {
                            menu_core.visible.store(true, Ordering::Relaxed);
                            let _ = window.show();
                            let _ = window.set_focus();
                            if let Ok(settings) = menu_core.settings.get() {
                                if menu_core.should_refresh_on_show(&settings) {
                                    let _ = menu_core.refresh_quota_internal();
                                }
                            }
                        }
                    }
                    "refresh" => {
                        let _ = app.state::<AppState>().0.refresh_quota_internal();
                    }
                    "settings" => {
                        menu_core.visible.store(true, Ordering::Relaxed);
                        let _ = app.emit("ui://open-settings", ());
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                    "quit" => app.exit(0),
                    _ => {}
                }
            }
        })
        .on_tray_icon_event(move |tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if let Some(window) = tray.app_handle().get_webview_window("main") {
                    if window.is_visible().unwrap_or(false) {
                        click_core.visible.store(false, Ordering::Relaxed);
                        let _ = window.hide();
                    } else {
                        click_core.visible.store(true, Ordering::Relaxed);
                        let _ = window.show();
                        let _ = window.set_focus();
                        if let Ok(settings) = click_core.settings.get() {
                            if click_core.should_refresh_on_show(&settings) {
                                let _ = click_core.refresh_quota_internal();
                            }
                        }
                    }
                }
            }
        })
        .build(app)?;
    Ok(())
}

fn open_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(url).status();
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .status();
    #[cfg(target_os = "linux")]
    let result = std::process::Command::new("xdg-open").arg(url).status();
    result
        .map_err(|error| format!("OPEN_BROWSER_FAILED: {error}"))?
        .success()
        .then_some(())
        .ok_or_else(|| "OPEN_BROWSER_FAILED".to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .on_window_event(|window, event| {
            let core = window.app_handle().state::<AppState>().0.clone();
            match event {
                WindowEvent::Moved(position) => {
                    core.update_last_position(position.x, position.y);
                }
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    core.save_window_placement(window);
                    let _ = window.hide();
                }
                _ => {}
            }
        })
        .setup(|app| {
            let config_dir = app
                .path()
                .app_config_dir()
                .unwrap_or_else(|_| PathBuf::from(".quota-critter"));
            let settings_path = config_dir.join("settings.json");
            let core =
                Core::new(app.handle().clone(), settings_path).map_err(std::io::Error::other)?;
            app.manage(AppState(Arc::clone(&core)));

            if let Some(window) = app.get_webview_window("main") {
                if let Ok(settings) = core.settings.get() {
                    if let Some(placement) = settings.window_placement {
                        let _ = window.set_position(tauri::Position::Logical(
                            tauri::LogicalPosition::new(placement.x as f64, placement.y as f64),
                        ));
                    }
                }
            }

            setup_tray(app, Arc::clone(&core))?;
            core.bootstrap();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_quota_snapshot,
            refresh_quota,
            start_login,
            get_settings,
            set_settings,
            set_always_on_top,
            set_widget_size,
            open_external_url
        ])
        .run(tauri::generate_context!())
        .expect("error while running Quota Critter");
}
