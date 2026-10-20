//! Preferences that survive a restart, saved as JSON in
//! `$XDG_CONFIG_HOME/purrtop/settings.json`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The theme name. `None` means the default theme.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Read awake NVIDIA GPUs through NVML. Off by default.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub nvidia: bool,
}

impl Settings {
    /// Reads the settings file. A missing or broken file means defaults.
    pub fn load() -> Self {
        path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    pub fn parse(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = path().ok_or_else(|| std::io::Error::other("no config directory (HOME isn't set)"))?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self).map_err(std::io::Error::other)?)
    }
}

fn path() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(config.join("purrtop/settings.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_survives_bad_files() {
        let settings = Settings { theme: Some("Phosphor".into()), nvidia: true };
        let json = serde_json::to_string(&settings).unwrap();
        assert_eq!(Settings::parse(&json), settings);
        assert_eq!(Settings::parse("not json"), Settings::default());
        assert_eq!(Settings::parse(r#"{"theme": 7, "future": true}"#), Settings::default());
        assert_eq!(Settings::parse(r#"{"future": true}"#), Settings::default());
    }
}
