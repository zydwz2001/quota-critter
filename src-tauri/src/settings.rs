use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, sync::Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub schema_version: u8,
    pub always_on_top: bool,
    pub locked: bool,
    pub launch_at_login: bool,
    pub reduced_motion: String,
    pub widget_mode: String,
    pub refresh_seconds_visible: u64,
    pub refresh_seconds_hidden: u64,
    pub locale: String,
    #[serde(default)]
    pub codex_override: Option<String>,
    pub window_placement: Option<WindowPlacement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowPlacement {
    pub monitor_id: Option<String>,
    pub x: i32,
    pub y: i32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            always_on_top: true,
            locked: false,
            launch_at_login: false,
            reduced_motion: "system".into(),
            widget_mode: "default".into(),
            refresh_seconds_visible: 60,
            refresh_seconds_hidden: 300,
            locale: "system".into(),
            codex_override: None,
            window_placement: None,
        }
    }
}

pub struct SettingsStore {
    path: PathBuf,
    value: Mutex<AppSettings>,
}

impl SettingsStore {
    pub fn load(path: PathBuf) -> Result<Self, String> {
        let value = fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<AppSettings>(&text).ok())
            .unwrap_or_default();
        Ok(Self {
            path,
            value: Mutex::new(value),
        })
    }

    pub fn get(&self) -> Result<AppSettings, String> {
        self.value
            .lock()
            .map(|value| value.clone())
            .map_err(|_| "SETTINGS_LOCK_FAILED".into())
    }

    pub fn set(&self, value: AppSettings) -> Result<(), String> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| "SETTINGS_PATH_INVALID".to_string())?;
        fs::create_dir_all(parent).map_err(|error| format!("SETTINGS_DIR_FAILED: {error}"))?;
        let serialized = serde_json::to_vec_pretty(&value)
            .map_err(|error| format!("SETTINGS_SERIALIZE_FAILED: {error}"))?;
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, serialized)
            .map_err(|error| format!("SETTINGS_WRITE_FAILED: {error}"))?;
        fs::rename(&temporary, &self.path)
            .map_err(|error| format!("SETTINGS_RENAME_FAILED: {error}"))?;
        *self
            .value
            .lock()
            .map_err(|_| "SETTINGS_LOCK_FAILED".to_string())? = value;
        Ok(())
    }
}
