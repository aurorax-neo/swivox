use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::i18n::Language;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum UiThemeMode {
    #[default]
    Dark,
    Light,
}

impl UiThemeMode {
    pub fn toggled(self) -> Self {
        match self {
            Self::Dark => Self::Light,
            Self::Light => Self::Dark,
        }
    }

    pub fn is_dark(self) -> bool {
        matches!(self, Self::Dark)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiPreferences {
    #[serde(default)]
    pub theme: UiThemeMode,
    #[serde(default)]
    pub language: Language,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            theme: UiThemeMode::Dark,
            language: Language::Zh,
        }
    }
}

pub fn default_ui_preferences_path() -> PathBuf {
    chatgptx_core::paths::default_app_state_dir().join("ui-preferences.json")
}

pub fn load_ui_preferences() -> UiPreferences {
    load_ui_preferences_from(&default_ui_preferences_path())
}

pub fn load_ui_preferences_from(path: &Path) -> UiPreferences {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<UiPreferences>(&text).ok())
        .unwrap_or_default()
}

pub fn save_ui_preferences(prefs: &UiPreferences) -> anyhow::Result<()> {
    save_ui_preferences_to(&default_ui_preferences_path(), prefs)
}

pub fn save_ui_preferences_to(path: &Path, prefs: &UiPreferences) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_string_pretty(prefs)?;
    fs::write(path, body)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_preferences_roundtrip_and_default_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ui-preferences.json");

        let defaults = load_ui_preferences_from(&path);
        assert_eq!(defaults.theme, UiThemeMode::Dark);
        assert_eq!(defaults.language, Language::Zh);

        let updated = UiPreferences {
            theme: UiThemeMode::Light,
            language: Language::En,
        };
        save_ui_preferences_to(&path, &updated).unwrap();
        assert_eq!(load_ui_preferences_from(&path), updated);
    }
}
