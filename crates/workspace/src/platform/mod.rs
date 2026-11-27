//! Platform-specific integrations.
//!
//! Everything here is a thin wrapper over a platform facility that the rest
//! of the editor should not have to know about: XDG directories, the desktop
//! session type, terminal detection and the presence of the external tools
//! the editor can drive.
//!
//! Keeping these behind a narrow interface is what makes it possible to add
//! macOS and Windows later without touching the subsystems above.

use std::path::{Path, PathBuf};

pub mod ssh;

/// Where the editor keeps its configuration, state and caches.
///
/// The paths follow the XDG base directory specification, so they land in the
/// usual places and are picked up by `~/.config` style tooling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    /// `$XDG_CONFIG_HOME/lide`
    pub config: PathBuf,
    /// `$XDG_DATA_HOME/lide`
    pub data: PathBuf,
    /// `$XDG_CACHE_HOME/lide`
    pub cache: PathBuf,
}

impl AppPaths {
    /// Resolve the standard application directories.
    pub fn resolve() -> Self {
        Self {
            config: dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from(".config"))
                .join("lide"),
            data: dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from(".local/share"))
                .join("lide"),
            cache: dirs::cache_dir()
                .unwrap_or_else(|| PathBuf::from(".cache"))
                .join("lide"),
        }
    }

    /// Create the directories, ignoring failures (a read-only home is not
    /// fatal — the editor still works, it just cannot remember anything).
    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.config)?;
        std::fs::create_dir_all(&self.data)?;
        std::fs::create_dir_all(&self.cache)?;
        Ok(())
    }
}

/// The display server the editor is running under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionType {
    /// A Wayland compositor.
    Wayland,
    /// An X11 display server.
    X11,
    /// No graphical session, or an unknown one.
    Unknown,
    /// A native window server outside X11/Wayland (macOS).
    Native,
}

impl SessionType {
    /// Detect the session type from the environment.
    pub fn detect() -> Self {
        if cfg!(target_os = "macos") {
            return SessionType::Native;
        }
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            SessionType::Wayland
        } else if std::env::var_os("DISPLAY").is_some() {
            SessionType::X11
        } else {
            SessionType::Unknown
        }
    }

    /// A human readable name.
    pub fn label(self) -> &'static str {
        match self {
            SessionType::Wayland => "Wayland",
            SessionType::X11 => "X11",
            SessionType::Unknown => "unknown session",
            SessionType::Native => "native window server",
        }
    }
}

/// Whether the given file descriptor refers to a terminal.
///
/// Used to decide whether the editor should start an interactive shell or
/// print plain log output.
pub fn is_terminal(fd: std::os::fd::RawFd) -> bool {
    // SAFETY: `isatty` only inspects the descriptor and never dereferences
    // it; passing an invalid descriptor simply returns 0.
    unsafe { libc::isatty(fd) == 1 }
}

/// Whether the editor is attached to a terminal on stdout.
pub fn stdout_is_terminal() -> bool {
    is_terminal(libc::STDOUT_FILENO)
}

/// The program a desktop uses to show a folder in its file manager.
pub fn reveal_command() -> &'static str {
    if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    }
}

/// Ask the system file manager to show `directory`.
pub fn reveal(directory: &Path) -> std::io::Result<()> {
    let mut child = std::process::Command::new(reveal_command())
        .arg(directory)
        .spawn()?;
    // The opener exits quickly, but dropping a `Child` does not reap it on
    // Unix. Reap it off the UI thread so repeated reveals do not leave
    // zombies behind.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_paths_are_lide_specific() {
        let paths = AppPaths::resolve();
        assert!(paths.config.ends_with("lide"));
        assert!(paths.data.ends_with("lide"));
        assert!(paths.cache.ends_with("lide"));
    }

}
