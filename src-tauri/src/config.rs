//! Réglages et options mémorisées par scanner (JSON dans le dossier de configuration).

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use rustscanner_shared::{OptValue, Settings};
use serde::{Deserialize, Serialize};

fn config_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(std::env::temp_dir).join("rustscanner")
}

fn read<T: for<'de> Deserialize<'de> + Default>(name: &str) -> T {
    fs::read_to_string(config_dir().join(name))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write<T: Serialize>(name: &str, value: &T) -> std::io::Result<()> {
    let dir = config_dir();
    fs::create_dir_all(&dir)?;
    let tmp = dir.join(format!("{name}.tmp"));
    fs::write(&tmp, serde_json::to_vec_pretty(value).unwrap_or_default())?;
    fs::rename(tmp, dir.join(name))
}

pub fn load_settings() -> Settings {
    read("settings.json")
}

pub fn save_settings(s: &Settings) -> std::io::Result<()> {
    write("settings.json", s)
}

/// Options modifiées par l'utilisateur, par nom de scanner, dans l'ordre des modifications.
#[derive(Serialize, Deserialize, Default)]
pub struct DeviceOptions {
    #[serde(flatten)]
    pub devices: HashMap<String, Vec<(String, OptValue)>>,
}

impl DeviceOptions {
    pub fn load() -> DeviceOptions {
        read("devices.json")
    }

    pub fn save(&self) -> std::io::Result<()> {
        write("devices.json", self)
    }

    pub fn get(&self, device: &str) -> Vec<(String, OptValue)> {
        self.devices.get(device).cloned().unwrap_or_default()
    }

    pub fn remember(&mut self, device: &str, name: &str, value: Option<OptValue>) {
        let list = self.devices.entry(device.to_string()).or_default();
        list.retain(|(n, _)| n != name);
        if let Some(v) = value {
            list.push((name.to_string(), v));
        }
    }

    pub fn forget(&mut self, device: &str) {
        self.devices.remove(device);
    }
}

/// Dossier d'enregistrement : réglage, sinon Documents, sinon le dossier personnel.
pub fn save_dir(settings: &Settings) -> PathBuf {
    if !settings.save_dir.trim().is_empty() {
        return PathBuf::from(settings.save_dir.trim());
    }
    dirs::document_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(std::env::temp_dir)
}

/// Locale du système (pour la langue automatique) ; sous Windows, à défaut de
/// variable d'environnement, la langue d'affichage de la session.
pub fn system_locale() -> String {
    let env = ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .find(|v| !v.is_empty() && v != "C" && v != "POSIX");
    #[cfg(windows)]
    let env = env.or_else(|| {
        // langue principale : les 10 bits de poids faible de l'identifiant
        const LANG_FRENCH: u16 = 0x0c;
        let lang = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() } & 0x3ff;
        Some(if lang == LANG_FRENCH { "fr" } else { "en" }.to_string())
    });
    env.unwrap_or_default()
}
