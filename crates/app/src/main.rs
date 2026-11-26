//! `lide` — a lightweight native code editor and document workspace.
//!
//! The binary is a thin shell around the workspace crate: it parses the
//! command line, opens a window and hands the work to the workspace.

use std::path::PathBuf;
use std::sync::OnceLock;

use lide_workspace::{Document, Workspace};
use rune::{AppSpec, Cx, El, Waker};

/// The application identifier, used as the window class.
const APP_ID: &str = "lide";

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
    spec.poll = Some(|workspace, seconds, _tx| workspace.poll_at(seconds));

    spec.on_key = Some(lide_workspace::keys::handle);

    rune::run(spec);
    Ok(())
}

/// Named states for headless snapshots.
///
/// `--scene palette` renders the command palette over the workspace, and so
/// on. They exist so that a picture of each state can be taken without
/// clicking through the editor.
type Scene = (&'static str, fn(&mut Workspace));

const SCENES: &[Scene] = &[
    ("palette", |s| {
        open_showcase(s);
        let tasks: Vec<String> = s.tasks.iter().map(|task| task.label.clone()).collect();
        s.palette.open(
            &tasks,
            &lide_workspace::command::Recents {
                files: &s.settings.recent_files,
                projects: &s.settings.recent_projects,
            },
        );
    }),
    ("palette-filtered", |s| {
        open_showcase(s);
        let tasks: Vec<String> = s.tasks.iter().map(|task| task.label.clone()).collect();
        let files = s.settings.recent_files.clone();
        let projects = s.settings.recent_projects.clone();
        s.palette.open(
            &tasks,
            &lide_workspace::command::Recents {
                files: &files,
                projects: &projects,
            },
        );
        s.palette.query = rune_edit::TextEdit::new("save");
        s.palette.refresh();
        s.palette.active = 1;
    }),
    ("find", |s| {
        open_showcase(s);
        s.find.open();
        s.find.query = rune_edit::TextEdit::new("the");
        let text = s.active_document().and_then(|d| d.text()).unwrap_or("").to_string();
        s.find.refresh(&text);
        s.find.next();
    }),
    ("light", |s| {
        s.theme_override = Some(false);
        open_showcase(s);
    }),
    ("split", |s| {
        open_showcase(s);
        if s.documents.len() > 1 {
            s.split_document = Some(1);
            s.active = 0;
        }
    }),
    ("notebook", |s| {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let _ = s.open_folder(&root);
        if let Err(error) = s.open_path(root.join("tests/fixtures/notebook.ipynb")) {
            s.set_notice(error);
        }
    }),
    ("csv", |s| {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let _ = s.open_folder(&root);
        if let Err(error) = s.open_path(root.join("tests/fixtures/table.csv")) {
            s.set_notice(error);
        }
    }),
    ("code-actions", |s| {
        open_showcase(s);
        s.code_actions = vec![
            lide_lsp::client::CodeAction {
                title: "Import `std::fmt`".to_string(),
                edits: Vec::new(),
            },
            lide_lsp::client::CodeAction {
                title: "Add `#[derive(Debug)]`".to_string(),
                edits: Vec::new(),
            },
        ];
        s.code_actions_open = true;
    }),
    ("diff", |s| {
        open_diff_scene(s);
    }),
    ("svg-source", |s| {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let _ = s.open_folder(&root);
        if let Err(error) = s.open_path(root.join("tests/fixtures/icon.svg")) {
            s.set_notice(error);
        }
        s.toggle_image_source();
        // The preview is document 0; put it in the other pane.
        s.split_document = Some(0);
        for _ in 0..40 {
            s.poll();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }),
    ("image", |s| {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let _ = s.open_folder(&root);
        if let Err(error) = s.open_path(root.join("tests/fixtures/picture.png")) {
            s.set_notice(error);
        }
        for _ in 0..40 {
            s.poll();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }),
    ("pdf-outline", |s| {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let _ = s.open_folder(&root);
        if let Err(error) = s.open_path(root.join("tests/fixtures/outline.pdf")) {
            s.set_notice(error);
        }
        for _ in 0..40 {
            s.poll();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }),
    ("pdf-select", |s| {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let _ = s.open_folder(&root);
        if let Err(error) = s.open_path(root.join("tests/fixtures/sample.pdf")) {
            s.set_notice(error);
        }
        for _ in 0..40 {
            s.poll();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        if let Some(Document::Pdf(pdf)) = s
            .documents
            .iter_mut()
            .find(|document| matches!(document, Document::Pdf(_)))
        {
            pdf.select_text("Second line");
        }
    }),
    ("build", |s| {
        open_showcase(s);
        let options = lide_latex::BuildOptions {
            program: "sh".to_string(),
            args: vec![
                "-c".to_string(),
                "echo 'This is pdfTeX, Version 3.141592653'; \
                 echo './main.tex:12: Undefined control sequence.'; \
                 echo './main.tex:40: Missing $ inserted.'; \
                 exit 1"
                    .to_string(),
            ],
            directory: std::env::temp_dir(),
        };
        if let Ok(mut build) = lide_latex::Build::start(options) {
            for _ in 0..60 {
                build.poll();
                if build.finished {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            s.build = Some(build);
        }
    }),
    ("pdf-search", |s| {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let _ = s.open_folder(&root);
        if let Err(error) = s.open_path(root.join("tests/fixtures/sample.pdf")) {
            s.set_notice(error);
        }
        for _ in 0..40 {
            s.poll();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        if let Some(Document::Pdf(pdf)) = s.documents.iter_mut().find(|d| matches!(d, Document::Pdf(_))) {
            pdf.search = rune_edit::TextEdit::new("line");
            let query = pdf.search.text.clone();
            pdf.search_for(&query);
        }
    }),
    ("latex-output", |s| {
        open_showcase(s);
        s.settings.latex_output_directory = "build".to_string();
        s.begin_latex_output();
    }),
    ("terminal-select", |s| {
        open_showcase(s);
        if let Err(error) = s.open_terminal() {
            s.set_notice(error);
        }
        for _ in 0..40 {
            s.poll();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        if let Some(terminal) = s.active_terminal_mut() {
            terminal.write("printf 'selected line one\\nselected line two\\nthird line\\n'\r");
        }
        for _ in 0..100 {
            s.poll();
            if s.terminal_text()
                .map(|text| text.contains("third line"))
                .unwrap_or(false)
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        if let Some(terminal) = s.active_terminal_mut() {
            // Select the two printed lines after the command line.
            terminal.selection = Some(((1, 0), (2, 17)));
        }
    }),
    ("terminal-settings", |s| {
        open_showcase(s);
        s.settings.terminal_shell = "/bin/zsh".to_string();
        s.settings.terminal_directory = "~/work".to_string();
        s.begin_terminal_settings();
    }),
    ("terminal-find", |s| {
        let _ = s.open_terminal();
        for _ in 0..40 {
            s.poll();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        if let Some(terminal) = s.active_terminal_mut() {
            terminal.write("echo lide-find-me\r");
        }
        for _ in 0..100 {
            s.poll();
            let found = s
                .terminal_text()
                .map(|text| text.contains("lide-find-me"))
                .unwrap_or(false);
            if found {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        s.open_terminal_find();
        s.terminal_find.query = rune_edit::TextEdit::new("lide-find-me");
        s.terminal_find_next();
    }),
    ("terminal", |s| {
        open_showcase(s);
        if let Err(error) = s.open_terminal() {
            s.set_notice(error);
        }
        // Let the shell draw its prompt before the snapshot.
        for _ in 0..40 {
            s.poll();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }),
    ("completion", |s| {
        open_showcase(s);
        s.completions = vec![
            lide_lsp::CompletionItem {
                label: "new".to_string(),
                detail: Some("fn() -> Workspace".to_string()),
                kind: Some(3),
                insert: "new()".to_string(),
            },
            lide_lsp::CompletionItem {
                label: "now".to_string(),
                detail: Some("f64".to_string()),
                kind: Some(5),
                insert: "now".to_string(),
            },
            lide_lsp::CompletionItem {
                label: "notice".to_string(),
                detail: Some("Option<String>".to_string()),
                kind: Some(5),
                insert: "notice".to_string(),
            },
        ];
        s.completion_open = true;
        s.completion_selected = 0;
    }),
    ("hover", |s| {
        open_showcase(s);
        s.hover = Some("fn new() -> Workspace — the state of the whole editor".to_string());
    }),
    ("places", |s| {
        open_showcase(s);
        s.sidebar = lide_workspace::SidebarMode::Places;
        s.references = vec![
            (std::path::PathBuf::from("crates/workspace/src/lib.rs"), 196, 11),
            (std::path::PathBuf::from("crates/workspace/src/lib.rs"), 344, 8),
            (std::path::PathBuf::from("crates/app/src/main.rs"), 96, 20),
        ];
    }),
    ("diagnostics", |s| {
        open_showcase(s);
        let path = s
            .active_document()
            .and_then(|document| document.path())
            .map(|path| path.to_path_buf())
            .expect("the showcase has a document");
        s.servers.seed_diagnostics(
            path.clone(),
            vec![
                lide_lsp::Diagnostic {
                    path: path.clone(),
                    start: lide_lsp::Position { line: 3, character: 0 },
                    end: lide_lsp::Position { line: 3, character: 6 },
                    severity: lide_lsp::Severity::Warning,
                    message: "this line is a warning".to_string(),
                    source: "showcase".to_string(),
                },
                lide_lsp::Diagnostic {
                    path,
                    start: lide_lsp::Position { line: 5, character: 0 },
                    end: lide_lsp::Position { line: 5, character: 8 },
                    severity: lide_lsp::Severity::Error,
                    message: "and this one is an error".to_string(),
                    source: "showcase".to_string(),
                },
            ],
        );
        s.sidebar = lide_workspace::SidebarMode::Problems;
    }),
    ("problems", |s| {
        open_showcase(s);
        s.sidebar = lide_workspace::SidebarMode::Problems;
    }),
    ("search", |s| {
        open_showcase(s);
        s.sidebar = lide_workspace::SidebarMode::Search;
        s.search_query = rune_edit::TextEdit::new("the");
        s.run_search();
        // Give ripgrep a moment so the snapshot has results in it.
        for _ in 0..200 {
            s.poll();
            if s.search.as_ref().map(|search| !search.running).unwrap_or(true) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }),
    ("sidebar", |s| {
        open_showcase(s);
        s.settings.show_sidebar = true;
    }),
    ("remote-file", |s| {
        open_showcase(s);
        s.dialog = Some(lide_workspace::DialogState::RemoteFile {
            hosts: vec!["work.example.com".to_string(), "build-box".to_string()],
            host: Box::new(rune_edit::TextEdit::new("build-box")),
            path: Box::new(rune_edit::TextEdit::new("/srv/project/src/main.rs")),
        });
    }),
    ("ssh-hosts", |s| {
        open_showcase(s);
        s.dialog = Some(lide_workspace::DialogState::SshHosts {
            hosts: vec![
                "work.example.com".to_string(),
                "build-box".to_string(),
                "pi".to_string(),
            ],
            name: Box::new(rune_edit::TextEdit::new("work.example.com")),
        });
    }),
    ("replace-confirm", |s| {
        open_showcase(s);
        s.sidebar = lide_workspace::SidebarMode::Search;
        s.search_query = rune_edit::TextEdit::new("fn ");
        s.run_search();
        for _ in 0..200 {
            s.poll();
            let done = s
                .search
                .as_ref()
                .map(|search| !search.running)
                .unwrap_or(false);
            if done {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        s.search_replacement = rune_edit::TextEdit::new("function ");
        s.begin_replace_all();
    }),
    ("conflict", |s| {
        open_showcase(s);
        let active = s.active;
        if let Some(path) = s.active_document().and_then(|d| d.path()).map(|p| p.to_path_buf()) {
            s.changed_on_disk.push(path);
            let _ = active;
        }
    }),
];

/// Open the project itself, for the scenes that need documents on screen.
fn open_showcase(workspace: &mut Workspace) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let _ = workspace.open_folder(&root);
    let _ = workspace.open_path(root.join("README.md"));
    let _ = workspace.open_path(root.join("Cargo.toml"));
}

/// A tiny repository with one modified file, so the diff snapshot is stable
/// whatever the editor's own working tree looks like.
fn open_diff_scene(workspace: &mut Workspace) {
    let root = std::env::temp_dir().join("lide-diff-scene");
    let _ = std::fs::remove_dir_all(&root);
    if std::fs::create_dir_all(&root).is_err() {
        open_showcase(workspace);
        return;
    }
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(&root)
            .env("GIT_AUTHOR_NAME", "lide")
            .env("GIT_AUTHOR_EMAIL", "lide@example.invalid")
            .env("GIT_COMMITTER_NAME", "lide")
            .env("GIT_COMMITTER_EMAIL", "lide@example.invalid")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    };

    let original = "fn main() {\n    println!(\"hi\");\n}\n";
    let changed = "fn main() {\n    println!(\"hello\");\n    println!(\"world\");\n}\n";
    let ready = std::fs::write(root.join("main.rs"), original).is_ok()
        && git(&["init", "--initial-branch=main"])
        && git(&["add", "--all"])
        && git(&["commit", "-m", "first"])
        && std::fs::write(root.join("main.rs"), changed).is_ok();
    if !ready {
        open_showcase(workspace);
        return;
    }

    let _ = workspace.open_folder(&root);
    let _ = workspace.open_path(root.join("main.rs"));
    workspace.refresh_git(true);
    workspace.open_diff_for_active();
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
