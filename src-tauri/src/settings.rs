use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, sync::Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub schema_version: u8,
    pub always_on_top: bool,
    pub locked: bool,
    pub launch_at_login: bool,
    #[serde(default = "default_true")]
    pub quota_reminder: bool,
    pub reduced_motion: String,
    pub widget_mode: String,
    pub refresh_seconds_visible: u64,
    pub refresh_seconds_hidden: u64,
    pub locale: String,
    #[serde(default)]
    pub codex_override: Option<String>,
    pub window_placement: Option<WindowPlacement>,
}

fn default_true() -> bool {
    true
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
            quota_reminder: true,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path() -> PathBuf {
        let mut p = std::env::temp_dir();
        let nonce = format!(
            "quota-critter-settings-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        p.push(nonce);
        // 文件可能不存在，路径本身存在即可
        p
    }

    #[test]
    fn missing_file_yields_default_settings() {
        let p = temp_path();
        let store = SettingsStore::load(p).expect("load");
        let got = store.get().expect("get");
        assert_eq!(got.schema_version, 1);
        assert!(got.always_on_top);
        assert!(got.quota_reminder);
    }

    #[test]
    fn round_trips_persisted_settings() {
        let p = temp_path();
        {
            let store = SettingsStore::load(p.clone()).expect("load");
            let mut next = store.get().unwrap();
            next.always_on_top = false;
            next.locale = "zh-CN".into();
            next.refresh_seconds_visible = 90;
            store.set(next).expect("set");
        }
        // 重新加载验证落盘正确
        let store2 = SettingsStore::load(p).expect("reload");
        let got = store2.get().unwrap();
        assert!(!got.always_on_top);
        assert_eq!(got.locale, "zh-CN");
        assert_eq!(got.refresh_seconds_visible, 90);
    }

    #[test]
    fn set_is_atomic_via_rename() {
        let p = temp_path();
        let store = SettingsStore::load(p.clone()).expect("load");
        store
            .set(AppSettings {
                locale: "en".into(),
                ..store.get().unwrap()
            })
            .expect("set");
        // 不应残留 .tmp 临时文件
        let mut tmp = p.clone();
        tmp.set_extension("json.tmp");
        assert!(
            !tmp.exists(),
            "temp file should be renamed to final: {tmp:?}"
        );
        assert!(p.exists());
    }
}
