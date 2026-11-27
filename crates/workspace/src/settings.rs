//! Settings, and the files the editor remembers between runs.
//!
//! Settings live in `$XDG_CONFIG_HOME/lide/settings.json`. They are read once
//! at startup and written whenever the user changes something, so there is no
//! separate "apply" step and no half-saved state.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How many recent entries to keep.
const MAX_RECENT: usize = 20;
/// The current settings schema. Version 2 moved the default to a hidden
/// sidebar; version 3 moved the default to a hidden document outline; version
/// 4 let the theme follow the system. Older files get each new default once.
const SCHEMA_VERSION: u32 = 4;

/// Which appearance the editor draws in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// Follow the desktop's light or dark appearance.
    #[default]
    System,
    /// Always light.
    Light,
    /// Always dark.
    Dark,
}

impl Theme {
    /// The name shown to the user and stored in the settings file.
    pub fn name(self) -> &'static str {
        match self {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }

    /// The theme named `name`, if there is one.
    pub fn from_name(name: &str) -> Option<Theme> {
        match name {
            "system" => Some(Theme::System),
            "light" => Some(Theme::Light),
            "dark" => Some(Theme::Dark),
            _ => None,
        }
    }

    /// The next theme in the cycle, for a command that steps through them.
    pub fn next(self) -> Theme {
        match self {
            Theme::System => Theme::Light,
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::System,
        }
    }

    /// Whether this theme draws dark, given what the desktop is showing.
    pub fn is_dark(self, system_dark: bool) -> bool {
        match self {
            Theme::System => system_dark,
            Theme::Light => false,
            Theme::Dark => true,
        }
    }
}

/// The editor's settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The settings schema, used to apply one-time default changes.
    #[serde(default)]
    pub schema_version: u32,
    /// The appearance: follow the system, or force light or dark.
    pub theme: Theme,
    /// Whether the dark theme is used. Kept for settings files written before
    /// [`Theme`] existed; [`Settings::migrate`] folds it into `theme` once.
    pub dark: bool,
    /// The editor's font size, in points.
    pub font_size: f32,
    /// The width of the sidebar, in points.
    pub sidebar_width: f32,
    /// Whether the sidebar is shown.
    pub show_sidebar: bool,
    /// Whether the editor is split in two.
    pub split: bool,
    /// The share of the editor width the left pane takes when split.
    pub split_ratio: f32,
    /// The share of a Markdown tab's width the source takes.
    pub markdown_ratio: f32,
    /// Whether a Markdown preview or PDF shows its outline.
    pub show_outline: bool,
    /// Whether long lines wrap to the editor width.
    pub soft_wrap: bool,
    /// Whether the gutter shows line numbers.
    pub line_numbers: bool,
    /// The most recently opened files.
    pub recent_files: Vec<PathBuf>,
    /// The most recently opened folders.
    pub recent_projects: Vec<PathBuf>,
    /// The engine LaTeX builds use: `auto`, `pdflatex`, `xelatex` or
    /// `lualatex`.
    pub latex_engine: String,
    /// The shell new terminals start. Empty means `$SHELL`.
    pub terminal_shell: String,
    /// The directory new terminals start in. Empty means the project root;
    /// a relative path is taken from the project root, and `~/` from home.
    pub terminal_directory: String,
    /// Where LaTeX builds write. Empty means beside the `.tex` file; a
    /// relative path is taken from there, and `~/` from home.
    pub latex_output_directory: String,
    /// Collapsed directories, per workspace root. A workspace with no entry
    /// opens fully collapsed.
    pub collapsed_directories: HashMap<PathBuf, Vec<PathBuf>>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            theme: Theme::System,
            dark: true,
            font_size: 13.0,
            sidebar_width: 240.0,
            show_sidebar: false,
            split: false,
            split_ratio: 0.5,
            markdown_ratio: 0.5,
            show_outline: false,
            soft_wrap: true,
            line_numbers: true,
            recent_files: Vec::new(),
            recent_projects: Vec::new(),
            latex_engine: "auto".to_string(),
            terminal_shell: String::new(),
            terminal_directory: String::new(),
            latex_output_directory: String::new(),
            collapsed_directories: HashMap::new(),
        }
    }
}

/// Apply the one-time default changes for older settings files.
///
/// Returns whether anything changed and the file needs writing back.
fn migrate(settings: &mut Settings) -> bool {
    let version = settings.schema_version;
    if version < 2 {
        // The old default was a visible sidebar; apply the hidden default once.
        settings.show_sidebar = false;
    }
    if version < 3 {
        // The old default was a visible outline; apply the hidden default once.
        settings.show_outline = false;
    }
    if version < 4 {
        // Before this the theme was a `dark` flag. Keep the appearance the user
        // last chose instead of snapping everyone to "follow the system".
        settings.theme = if settings.dark {
            Theme::Dark
        } else {
            Theme::Light
        };
    }
    settings.schema_version = SCHEMA_VERSION;
    version < SCHEMA_VERSION
}

impl Settings {
    /// Read the settings, falling back to the defaults when there are none or
    /// they cannot be parsed.
    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        match serde_json::from_str::<Settings>(&text) {
            Ok(mut settings) => {
                if migrate(&mut settings) {
                    settings.save();
                }
                settings
            }
            Err(error) => {
                log::warn!("could not parse {}: {error}", path.display());
                Self::default()
            }
        }
    }

    /// Write the settings out.
    pub fn save(&self) {
        let Some(path) = Self::path() else {
            return;
        };
        if let Some(directory) = path.parent() {
            if let Err(error) = std::fs::create_dir_all(directory) {
                log::warn!("could not create {}: {error}", directory.display());
                return;
            }
        }
        let text = match serde_json::to_string_pretty(self) {
            Ok(text) => text,
            Err(error) => {
                log::warn!("could not serialise the settings: {error}");
                return;
            }
        };
        if let Err(error) = std::fs::write(&path, text) {
            log::warn!("could not write {}: {error}", path.display());
        }
    }

    /// Where the settings file lives.
    ///
    /// `LIDE_CONFIG_DIR` overrides the XDG location. Tests and headless
    /// snapshots set it so that checking the editor never rewrites the
    /// settings of the person running the check.
    pub fn path() -> Option<PathBuf> {
        if let Some(directory) = std::env::var_os("LIDE_CONFIG_DIR") {
            return Some(PathBuf::from(directory).join("settings.json"));
        }
        Some(
            crate::platform::AppPaths::resolve()
                .config
                .join("settings.json"),
        )
    }

    /// The line height the editor uses, in points.
    pub fn line_height(&self) -> f32 {
        (self.font_size * 1.55).round()
    }

    /// Remember a file as recently opened.
    pub fn remember_file(&mut self, path: impl AsRef<Path>) {
        remember(&mut self.recent_files, path.as_ref());
    }

    /// Remember a folder as recently opened.
    pub fn remember_project(&mut self, path: impl AsRef<Path>) {
        remember(&mut self.recent_projects, path.as_ref());
    }
}

/// Move `path` to the front of `list`, dropping duplicates and trimming the tail.
fn remember(list: &mut Vec<PathBuf>, path: &Path) {
    list.retain(|existing| existing != path);
    list.insert(0, path.to_path_buf());
    list.truncate(MAX_RECENT);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sensible() {
        let settings = Settings::default();
        assert!(settings.dark);
        assert!(settings.font_size >= 10.0);
        assert!(settings.line_height() > settings.font_size);
        assert!(settings.recent_files.is_empty());
    }

    #[test]
    fn remembering_a_file_moves_it_to_the_front() {
        let mut settings = Settings::default();
        settings.remember_file("/a");
        settings.remember_file("/b");
        settings.remember_file("/a");
        assert_eq!(
            settings.recent_files,
            vec![PathBuf::from("/a"), PathBuf::from("/b")]
        );
    }

    #[test]
    fn the_recent_list_is_bounded() {
        let mut settings = Settings::default();
        for index in 0..50 {
            settings.remember_file(format!("/file-{index}"));
        }
        assert_eq!(settings.recent_files.len(), MAX_RECENT);
        assert_eq!(settings.recent_files[0], PathBuf::from("/file-49"));
    }

    #[test]
    fn terminal_preferences_default_to_the_environment_and_project() {
        let settings = Settings::default();
        assert!(settings.terminal_shell.is_empty());
        assert!(settings.terminal_directory.is_empty());
    }

    #[test]
    fn the_latex_engine_defaults_to_auto() {
        assert_eq!(Settings::default().latex_engine, "auto");
    }

    #[test]
    fn a_latex_output_directory_survives_the_round_trip() {
        let settings = Settings {
            latex_output_directory: "build".to_string(),
            ..Default::default()
        };
        let text = serde_json::to_string(&settings).unwrap();
        let parsed: Settings = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed.latex_output_directory, "build");
        assert!(Settings::default().latex_output_directory.is_empty());
    }

    #[test]
    fn a_chosen_latex_engine_survives_the_round_trip() {
        let settings = Settings {
            latex_engine: "xelatex".to_string(),
            ..Default::default()
        };
        let text = serde_json::to_string(&settings).unwrap();
        let parsed: Settings = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed.latex_engine, "xelatex");
        // An older file with no engine keeps the default.
        let old: Settings = serde_json::from_str(r#"{"dark": true}"#).unwrap();
        assert_eq!(old.latex_engine, "auto");
    }

    #[test]
    fn the_sidebar_starts_hidden() {
        assert!(!Settings::default().show_sidebar);
    }

    #[test]
    fn the_outline_starts_hidden_and_the_splits_are_even() {
        let settings = Settings::default();
        assert!(!settings.show_outline);
        assert_eq!(settings.split_ratio, 0.5);
        assert_eq!(settings.markdown_ratio, 0.5);
        assert!(settings.soft_wrap);
    }

    #[test]
    fn older_settings_hide_the_outline_once() {
        let mut settings = Settings {
            schema_version: 2,
            show_outline: true,
            ..Default::default()
        };
        assert!(migrate(&mut settings));
        assert_eq!(settings.schema_version, SCHEMA_VERSION);
        assert!(!settings.show_outline);
        assert!(!migrate(&mut settings), "a current file is left alone");
    }

    #[test]
    fn a_theme_chosen_before_the_theme_setting_existed_is_kept() {
        // A file written when the theme was a `dark` flag, with no `theme`.
        let mut settings: Settings =
            serde_json::from_str(r#"{"schema_version": 3, "dark": false}"#).unwrap();
        assert!(migrate(&mut settings));
        assert_eq!(
            settings.theme,
            Theme::Light,
            "a user who chose light keeps light, rather than being sent back to the system"
        );

        let mut settings: Settings =
            serde_json::from_str(r#"{"schema_version": 3, "dark": true}"#).unwrap();
        assert!(migrate(&mut settings));
        assert_eq!(settings.theme, Theme::Dark);
    }

    #[test]
    fn a_new_settings_file_follows_the_system() {
        assert_eq!(Settings::default().theme, Theme::System);
    }

    #[test]
    fn the_theme_names_round_trip_and_cycle() {
        let mut theme = Theme::System;
        let mut seen = Vec::new();
        for _ in 0..3 {
            seen.push(theme.name());
            assert_eq!(Theme::from_name(theme.name()), Some(theme));
            theme = theme.next();
        }
        assert_eq!(seen, vec!["system", "light", "dark"]);
        assert_eq!(theme, Theme::System, "three steps in a cycle of three");
        assert_eq!(Theme::from_name("solarized"), None);
    }

    #[test]
    fn the_system_theme_follows_the_desktop() {
        assert!(Theme::System.is_dark(true));
        assert!(!Theme::System.is_dark(false));
        assert!(!Theme::Light.is_dark(true), "light ignores a dark desktop");
        assert!(Theme::Dark.is_dark(false), "dark ignores a light desktop");
    }

    #[test]
    fn collapsed_directories_survive_the_round_trip() {
        let mut settings = Settings::default();
        settings.collapsed_directories.insert(
            PathBuf::from("/work"),
            vec![PathBuf::from("/work/src"), PathBuf::from("/work/tests")],
        );
        let text = serde_json::to_string(&settings).unwrap();
        let parsed: Settings = serde_json::from_str(&text).unwrap();
        assert_eq!(
            parsed.collapsed_directories.get(Path::new("/work")),
            Some(&vec![
                PathBuf::from("/work/src"),
                PathBuf::from("/work/tests")
            ])
        );
        let old: Settings = serde_json::from_str(r#"{"dark": true}"#).unwrap();
        assert!(old.collapsed_directories.is_empty());
    }

    #[test]
    fn settings_round_trip_through_json() {
        let mut settings = Settings {
            dark: false,
            font_size: 16.0,
            ..Default::default()
        };
        settings.remember_file("/tmp/x");
        let text = serde_json::to_string(&settings).unwrap();
        let parsed: Settings = serde_json::from_str(&text).unwrap();
        assert!(!parsed.dark);
        assert_eq!(parsed.font_size, 16.0);
        assert_eq!(parsed.recent_files, settings.recent_files);
    }

    #[test]
    fn a_partial_settings_file_keeps_the_defaults() {
        let parsed: Settings = serde_json::from_str(r#"{"dark": false}"#).unwrap();
        assert!(!parsed.dark);
        assert_eq!(parsed.font_size, Settings::default().font_size);
        assert_eq!(parsed.split_ratio, Settings::default().split_ratio);
        assert_eq!(
            parsed.markdown_ratio,
            Settings::default().markdown_ratio
        );
        assert_eq!(parsed.show_outline, Settings::default().show_outline);
        assert_eq!(parsed.soft_wrap, Settings::default().soft_wrap);
    }
}
