use std::fs;
use std::path::PathBuf;

use crate::domain::AppSettings;
use crate::platform;

pub fn settings_path() -> PathBuf {
    platform::config_dir().join("settings.json")
}

pub fn load_settings() -> AppSettings {
    let path = settings_path();
    let Ok(bytes) = fs::read(&path) else {
        return AppSettings::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

pub fn save_settings(settings: &AppSettings) -> Result<(), String> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())
}
