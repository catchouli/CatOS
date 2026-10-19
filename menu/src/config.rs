//! menu.toml: what's in the menu and which remote keys do what.

use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
pub struct Config {
    #[serde(default)]
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub controls: Controls,
    #[serde(default)]
    pub display: Display,
    #[serde(default)]
    pub avatar: Avatar,
    pub category: Vec<Category>,
}

#[derive(Deserialize)]
#[serde(default)]
pub struct Display {
    /// How much to enlarge apps on a 1080p screen, so they're readable from the
    /// couch. Adjusted for the real resolution, so they look the same size on any screen.
    pub scale: f32,
    /// Screen mode to switch to when the menu starts, e.g. "1920x1080" or
    /// "1920x1080@60", if the screen offers it. "auto" keeps the screen's own.
    /// A mode picked in Settings → Resolution takes its place.
    pub mode: String,
}

impl Default for Display {
    fn default() -> Self {
        Self { scale: 1.75, mode: "1920x1080".into() }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Avatar {
    /// A Live2D model's .model3.json, to show beside the menu. None for no avatar.
    pub model: Option<PathBuf>,
}

#[derive(Deserialize)]
#[serde(default)]
pub struct Controls {
    /// Keys that close the open app immediately (Linux evdev names, e.g. KEY_HOMEPAGE).
    pub home_keys: Vec<String>,
    /// Keys that close the open app when held down.
    pub hold_keys: Vec<String>,
    pub hold_seconds: f32,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            home_keys: vec!["KEY_HOMEPAGE".into()],
            hold_keys: vec!["KEY_BACK".into(), "KEY_ESC".into()],
            hold_seconds: 1.0,
        }
    }
}

#[derive(Deserialize)]
pub struct Category {
    pub name: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub item: Vec<Item>,
}

#[derive(Deserialize)]
pub struct Item {
    pub name: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub description: String,
    /// Program and arguments to run.
    #[serde(default)]
    pub command: Vec<String>,
    /// Built-in action instead of a command: reboot, poweroff, network, sound, wifi, resolution, quit.
    #[serde(default)]
    pub action: Option<String>,
    /// Overrides [display] scale for this app.
    #[serde(default)]
    pub scale: Option<f32>,
}

pub fn load(explicit: Option<PathBuf>) -> Result<Config, String> {
    let candidates: Vec<PathBuf> = match explicit {
        Some(p) => vec![p],
        None => vec![
            std::env::home_dir().unwrap_or_default().join(".config/catos/menu.toml"),
            Path::new("/etc/catos/menu.toml").to_path_buf(),
        ],
    };
    for p in &candidates {
        if let Ok(text) = std::fs::read_to_string(p) {
            return toml::from_str(&text).map_err(|e| format!("{}: {e}", p.display()));
        }
    }
    Err(format!("no config found (looked in {candidates:?})"))
}
