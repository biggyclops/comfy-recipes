use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub comfyui_address: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            comfyui_address: "http://hades:8188".to_string(),
        }
    }
}

fn settings_path() -> Option<PathBuf> {
    ProjectDirs::from("com", "comfyrecipes", "Comfy Recipes").map(|dirs| {
        let config_dir = dirs.config_dir();
        fs::create_dir_all(config_dir).ok();
        config_dir.join("settings.json")
    })
}

pub fn load_settings() -> Settings {
    settings_path()
        .and_then(|path| fs::read_to_string(&path).ok())
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

pub fn save_settings(settings: &Settings) -> Result<(), String> {
    let path = settings_path().ok_or("Could not determine settings path")?;
    let content = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(&path, content).map_err(|e| e.to_string())
}
