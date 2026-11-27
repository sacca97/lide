//! `lide` — a lightweight native code editor and document workspace.
//!
//! The binary is a thin shell around the workspace crate: it parses the
//! command line, opens a window and hands the work to the workspace.

use std::path::PathBuf;
use std::sync::OnceLock;

use lide_workspace::Workspace;
use rune::{AppSpec, Cx, El, Waker};

mod scenes;

use scenes::SCENES;

/// The application identifier, used as the window class.
const APP_ID: &str = "lide";

/// The strip macOS keeps clear at the top for its three window buttons. The
/// content view fills the window, so the editor reserves this band instead of
/// starting under the traffic lights.
#[cfg(target_os = "macos")]
const TITLEBAR_HEIGHT: f32 = 30.0;

/// What the editor should open at startup.
#[derive(Debug, Default)]
struct Startup {
    folder: Option<PathBuf>,
    files: Vec<PathBuf>,
}

/// Parse the command line.
///
/// ```text
/// lide [FOLDER] [FILE...]
/// ```
///
/// With no arguments the editor starts with an empty workspace.
fn parse_args(args: impl IntoIterator<Item = String>) -> Startup {
    let mut startup = Startup::default();
    for argument in args {
        let path = PathBuf::from(argument);
        if path.is_dir() {
            startup.folder = Some(path);
        } else {
            startup.files.push(path);
        }
    }
    startup
}

/// The paths the editor was started with.
///
/// Rune's hooks are plain function pointers, so the command line is parsed
/// once and kept here.
static STARTUP: OnceLock<Startup> = OnceLock::new();

/// Open the workspace the command line asked for.
fn setup(workspace: &mut Workspace, waker: Waker) {
    workspace.images = Some(rune_image::ImageCache::new(waker));

    // Rune's own flags (`--snap`, `--tree`, ...) are removed from the
    // command line; what is left belongs to the editor.
    let startup = STARTUP.get_or_init(|| parse_args(rune::tooling::extra_args().iter().cloned()));

    if let Some(folder) = &startup.folder {
        if let Err(error) = workspace.open_folder(folder) {
            log::warn!("could not open {}: {error}", folder.display());
            workspace.set_notice(format!("could not open {}", folder.display()));
        }
    }

    for file in &startup.files {
        if let Err(error) = workspace.open_path(file) {
            log::warn!("could not open {}: {error}", file.display());
            workspace.set_notice(format!("could not open {}", file.display()));
        }
    }
}

fn main() -> anyhow::Result<()> {
    init_logging();

    // The application id is the window's class on Wayland and X11. It is what
    // lets the compositor match the window to the editor's `.desktop` entry,
    // its icon and its window rules.
    rune::tooling::set_app_id(APP_ID);

    // The editor's own directories follow the XDG base directory
    // specification; they are created up front so that a later write cannot
    // fail for want of a directory.
    let paths = lide_platform::AppPaths::resolve();
    if let Err(error) = paths.ensure() {
        log::warn!("could not create the editor's directories: {error}");
    }
    log::info!(
        "lide starting on {} (config in {})",
        lide_platform::SessionType::detect().label(),
        paths.config.display()
    );

    let mut spec = AppSpec {
        scenes: SCENES,
        ..AppSpec::new("lide", (1200.0, 800.0), Workspace::new, view)
    };

    spec.setup = Some(setup);

    // Collect the pages the PDF viewer rendered in the background, the files
    // that changed, search results and language-server answers. The clock
    // drives the completion debounce.
    spec.poll = Some(|workspace, seconds, tx| {
        let changed = workspace.poll_at(seconds);
        // Scrolls a jump asked for. They are repeated for a few frames, as
        // the page or the text they target may not have its height yet.
        let mut pending = !workspace.scroll_requests.is_empty();
        for (key, offset, tries) in &mut workspace.scroll_requests {
            tx.scroll_to_instant(key.clone(), 0.0, *offset);
            *tries -= 1;
        }
        workspace.scroll_requests.retain(|(_, _, tries)| *tries > 0);
        pending |= changed;
        pending
    });

    spec.on_key = Some(lide_workspace::keys::handle);

    let app = rune::App::new(spec)
        .modifier_keys()
        // The modifier state is needed for a Cmd-click on a source line, and
        // the window hears it whatever has the focus.
        .on_modifiers(
            |workspace: &mut Workspace, mods: rune::Mods, _tx: &mut rune::Tx| {
                workspace.set_primary_modifier(mods.primary() || mods.ctrl);
            },
        );

    // On macOS the content view fills the window under a transparent titlebar.
    // Declaring a titlebar puts the traffic lights in its band and tells the
    // view how much room to leave them (`Cx::chrome_insets`); other platforms
    // keep their native frame.
    #[cfg(target_os = "macos")]
    let app = app.titlebar(rune::Titlebar::new(TITLEBAR_HEIGHT));

    app.run();
    Ok(())
}

/// Draw the editor.
fn view(workspace: &Workspace, cx: &mut Cx<Workspace>) -> El {
    lide_workspace::view(workspace, cx)
}

/// Set up logging.
///
/// The editor writes to standard error so that it does not interfere with
/// anything that reads its standard output.
fn init_logging() {
    let filter = std::env::var("LIDE_LOG").unwrap_or_else(|_| "warn".to_string());
    let _ = env_logger::Builder::new()
        .parse_filters(&filter)
        .target(env_logger::Target::Stderr)
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_becomes_the_workspace_root() {
        let startup = parse_args([".".to_string()]);
        assert!(startup.folder.is_some());
        assert!(startup.files.is_empty());
    }

    #[test]
    fn no_arguments_means_an_empty_workspace() {
        let startup = parse_args(Vec::<String>::new());
        assert!(startup.folder.is_none());
        assert!(startup.files.is_empty());
    }
}
