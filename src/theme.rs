//! TUI color themes. Built-ins ship compiled in; `~/.lazyreq/themes.json`
//! (plain JSON, deliberately not encrypted — it's config, not data) selects
//! the active one via `current` and may override or add themes:
//!
//! ```json
//! {
//!   "current": "default",
//!   "themes": {
//!     "default": { "background": "#160f09", "base": "#e6d7bf", "primary": "#bb671f", ... }
//!   }
//! }
//! ```
//!
//! The file is scaffolded with every built-in on first run so the shape is
//! discoverable; missing fields in a theme fall back to `default`'s values.

use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;

use crate::vault;

#[derive(Clone, Copy)]
pub struct Theme {
    pub background: Color,
    pub text: Color,
    pub primary: Color,
    pub secondary: Color,
    pub border: Color,
    pub running: Color,
    pub muted: Color,
    pub selection_text: Color,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct ThemeSpec {
    #[serde(skip_serializing_if = "Option::is_none")]
    background: Option<String>,
    /// main text color (named `base` in themes.json)
    #[serde(rename = "base", skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    primary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    secondary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    border: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    running: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    muted: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    selection_text: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct ThemesFile {
    current: String,
    themes: BTreeMap<String, ThemeSpec>,
}

pub struct Themes {
    pub current: String,
    pub names: Vec<String>,
    map: BTreeMap<String, Theme>,
}

fn spec(colors: [&str; 8]) -> ThemeSpec {
    let [background, text, primary, secondary, border, running, muted, selection_text] =
        colors.map(|c| Some(c.to_string()));
    ThemeSpec {
        background,
        text,
        primary,
        secondary,
        border,
        running,
        muted,
        selection_text,
    }
}

/// background, base, primary, secondary, border, running, muted, selection_text
fn builtins() -> BTreeMap<String, ThemeSpec> {
    BTreeMap::from([
        // the lazyreq logo palette
        ("default".into(),
         spec(["#160f09", "#e6d7bf", "#bb671f", "#9a5619", "#613614", "#8b7b2e", "#8a7963", "#1d140b"])),
        ("dracula".into(),
         spec(["#282a36", "#f8f8f2", "#bd93f9", "#ff79c6", "#44475a", "#f1fa8c", "#6272a4", "#282a36"])),
        ("solarized-dark".into(),
         spec(["#002b36", "#839496", "#268bd2", "#2aa198", "#073642", "#b58900", "#586e75", "#002b36"])),
        ("solarized-light".into(),
         spec(["#fdf6e3", "#657b83", "#268bd2", "#d33682", "#eee8d5", "#b58900", "#93a1a1", "#fdf6e3"])),
        // Atom One Dark
        ("atom".into(),
         spec(["#282c34", "#abb2bf", "#61afef", "#c678dd", "#3e4452", "#e5c07b", "#5c6370", "#282c34"])),
    ])
}

fn parse_hex(raw: &str) -> Option<Color> {
    let hex = raw.trim().trim_start_matches('#');
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let n = u32::from_str_radix(hex, 16).ok()?;
    Some(Color::Rgb((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

fn default_theme() -> Theme {
    Theme {
        background: Color::Rgb(0x16, 0x0f, 0x09),
        text: Color::Rgb(0xe6, 0xd7, 0xbf),
        primary: Color::Rgb(0xbb, 0x67, 0x1f),
        secondary: Color::Rgb(0x9a, 0x56, 0x19),
        border: Color::Rgb(0x61, 0x36, 0x14),
        running: Color::Rgb(0x8b, 0x7b, 0x2e),
        muted: Color::Rgb(0x8a, 0x79, 0x63),
        selection_text: Color::Rgb(0x1d, 0x14, 0x0b),
    }
}

fn resolve(spec: &ThemeSpec, fallback: &Theme) -> Theme {
    let color = |raw: &Option<String>, fb: Color| {
        raw.as_deref().and_then(parse_hex).unwrap_or(fb)
    };
    Theme {
        background: color(&spec.background, fallback.background),
        text: color(&spec.text, fallback.text),
        primary: color(&spec.primary, fallback.primary),
        secondary: color(&spec.secondary, fallback.secondary),
        border: color(&spec.border, fallback.border),
        running: color(&spec.running, fallback.running),
        muted: color(&spec.muted, fallback.muted),
        selection_text: color(&spec.selection_text, fallback.selection_text),
    }
}

/// Pure core of `load`, testable without touching the filesystem: merges
/// file themes over the built-ins and resolves the current selection.
fn assemble(file: Option<ThemesFile>) -> Themes {
    let mut specs = builtins();
    let mut current = "default".to_string();
    if let Some(file) = file {
        for (name, theme) in file.themes {
            specs.insert(name, theme);
        }
        current = file.current;
    }

    let fallback = default_theme();
    let map: BTreeMap<String, Theme> = specs
        .iter()
        .map(|(name, spec)| (name.clone(), resolve(spec, &fallback)))
        .collect();

    if !map.contains_key(&current) {
        current = "default".to_string();
    }

    Themes {
        current,
        names: map.keys().cloned().collect(),
        map,
    }
}

impl Themes {
    pub fn active(&self) -> Theme {
        self.map
            .get(&self.current)
            .copied()
            .unwrap_or_else(default_theme)
    }

    /// The theme after `current`, wrapping around (for the cycle key).
    pub fn next_name(&self) -> String {
        let idx = self
            .names
            .iter()
            .position(|n| *n == self.current)
            .unwrap_or(0);
        self.names[(idx + 1) % self.names.len()].clone()
    }
}

pub fn load() -> Themes {
    let file = themes_path().and_then(|path| {
        if !path.exists() {
            // Scaffold with every built-in so the format is discoverable.
            let scaffold = ThemesFile {
                current: "default".to_string(),
                themes: builtins(),
            };
            if let Ok(json) = serde_json::to_string_pretty(&scaffold) {
                let _ = fs::write(&path, json + "\n");
            }
            return Some(scaffold);
        }
        let raw = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&raw).ok()
    });

    assemble(file)
}

/// Persists the selected theme name, preserving any user-defined themes.
pub fn save_current(name: &str) -> Result<(), String> {
    let path = themes_path().ok_or("cannot locate ~/.lazyreq".to_string())?;
    let mut file: ThemesFile = fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| ThemesFile {
            current: "default".to_string(),
            themes: builtins(),
        });

    file.current = name.to_string();
    let json = serde_json::to_string_pretty(&file)
        .map_err(|e| format!("cannot serialize themes: {}", e))?;
    fs::write(&path, json + "\n").map_err(|e| format!("cannot write `{}`: {}", path.display(), e))
}

fn themes_path() -> Option<std::path::PathBuf> {
    vault::lazyreq_dir().ok().map(|dir| dir.join("themes.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_parsing() {
        assert_eq!(parse_hex("#bb671f"), Some(Color::Rgb(0xbb, 0x67, 0x1f)));
        assert_eq!(parse_hex("bb671f"), Some(Color::Rgb(0xbb, 0x67, 0x1f)));
        assert_eq!(parse_hex("#fff"), None);
        assert_eq!(parse_hex("not a color"), None);
    }

    #[test]
    fn builtins_all_resolve_without_fallbacks() {
        let themes = assemble(None);
        for name in ["default", "dracula", "solarized-dark", "solarized-light", "atom"] {
            assert!(themes.names.contains(&name.to_string()), "missing {}", name);
        }
        assert_eq!(themes.current, "default");
        assert_eq!(themes.active().primary, Color::Rgb(0xbb, 0x67, 0x1f));
    }

    #[test]
    fn file_selects_and_overrides() {
        let file: ThemesFile = serde_json::from_str(
            r##"{
                "current": "dracula",
                "themes": {
                    "dracula": { "primary": "#ff0000" },
                    "mine": { "background": "#000000", "base": "#ffffff" }
                }
            }"##,
        )
        .unwrap();

        let themes = assemble(Some(file));
        assert_eq!(themes.current, "dracula");
        // the file's partial dracula replaces the built-in; missing fields
        // fall back to the DEFAULT theme's values
        assert_eq!(themes.active().primary, Color::Rgb(0xff, 0x00, 0x00));
        assert_eq!(themes.active().background, Color::Rgb(0x16, 0x0f, 0x09));
        // user-defined themes join the cycle list
        assert!(themes.names.contains(&"mine".to_string()));
    }

    #[test]
    fn unknown_current_falls_back_to_default() {
        let file: ThemesFile =
            serde_json::from_str(r#"{ "current": "nope", "themes": {} }"#).unwrap();
        assert_eq!(assemble(Some(file)).current, "default");
    }

    #[test]
    fn next_name_cycles_alphabetically_and_wraps() {
        let mut themes = assemble(None);
        themes.current = "atom".to_string(); // first alphabetically
        assert_eq!(themes.next_name(), "default");
        themes.current = "solarized-light".to_string(); // last
        assert_eq!(themes.next_name(), "atom");
    }

    #[test]
    fn scaffold_shape_roundtrips() {
        let scaffold = ThemesFile { current: "default".to_string(), themes: builtins() };
        let json = serde_json::to_string_pretty(&scaffold).unwrap();
        assert!(json.contains("\"base\": \"#e6d7bf\"")); // field named `base` in JSON
        let back: ThemesFile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.themes.len(), 5);
    }
}
