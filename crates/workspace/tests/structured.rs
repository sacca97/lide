//! Checking and formatting structured files, through the workspace.

use std::path::{Path, PathBuf};

use lide_workspace::command::{self, Command};
use lide_workspace::Workspace;

/// Keep the tests from reading or writing the real settings file.
fn isolate_settings() {
    use std::sync::OnceLock;
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let directory = std::env::temp_dir().join("lide-test-config-structured");
        let _ = std::fs::create_dir_all(&directory);
        // SAFETY: this runs once, before any test touches the settings.
        unsafe { std::env::set_var("LIDE_CONFIG_DIR", &directory) };
    });
}

fn scratch(name: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("lide-structured-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

fn open(directory: &Path, name: &str, text: &str) -> Workspace {
    isolate_settings();
    let path = directory.join(name);
    std::fs::write(&path, text).unwrap();
    let mut workspace = Workspace::new();
    workspace.open_path(&path).expect("the file opens");
    workspace
}

#[test]
fn an_invalid_json_document_is_reported_with_its_line() {
    let directory = scratch("json-error");
    let mut workspace = open(
        &directory,
        "broken.json",
        "{\n  \"a\": 1,\n  \"b\": ,\n}\n",
    );

    command::run(Command::ValidateDocument, &mut workspace);
    let notice = workspace.notice.clone().unwrap_or_default();
    assert!(notice.contains("line 3"), "notice: {notice}");
    assert!(notice.contains("broken.json"), "notice: {notice}");

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn formatting_json_rewrites_the_buffer_and_marks_it_dirty() {
    let directory = scratch("json-format");
    let mut workspace = open(&directory, "package.json", r#"{"b":1,"a":[1,2]}"#);

    command::run(Command::FormatStructuredDocument, &mut workspace);
    let text = workspace.documents[0].text().unwrap().to_string();
    assert_eq!(
        text,
        "{\n  \"b\": 1,\n  \"a\": [\n    1,\n    2\n  ]\n}\n"
    );
    assert!(workspace.documents[0].is_dirty(), "formatting is an edit");

    // Formatting again changes nothing.
    command::run(Command::FormatStructuredDocument, &mut workspace);
    assert_eq!(workspace.documents[0].text().unwrap(), text);
    assert_eq!(
        workspace.notice.as_deref(),
        Some("JSON is already formatted")
    );

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn toml_is_validated_but_not_reformatted() {
    let directory = scratch("toml");
    let mut workspace = open(&directory, "Cargo.toml", "[package]\nname = \"x\"\n");

    command::run(Command::ValidateDocument, &mut workspace);
    assert_eq!(
        workspace.notice.as_deref(),
        Some("Cargo.toml: valid TOML")
    );

    command::run(Command::FormatStructuredDocument, &mut workspace);
    let notice = workspace.notice.clone().unwrap_or_default();
    assert!(notice.contains("comments"), "notice: {notice}");
    assert_eq!(
        workspace.documents[0].text().unwrap(),
        "[package]\nname = \"x\"\n",
        "the buffer is untouched"
    );
    assert!(!workspace.documents[0].is_dirty());

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn xml_is_formatted_and_complains_about_mixed_content() {
    let directory = scratch("xml");
    let mut workspace = open(
        &directory,
        "layout.xml",
        "<root><a>text</a><b/></root>",
    );

    command::run(Command::FormatStructuredDocument, &mut workspace);
    assert_eq!(
        workspace.documents[0].text().unwrap(),
        "<root>\n  <a>text</a>\n  <b/>\n</root>\n"
    );

    std::fs::write(
        directory.join("layout.xml"),
        "<p>Hello <b>world</b>.</p>",
    )
    .unwrap();
    workspace.documents[0]
        .editor_mut()
        .unwrap()
        .reload()
        .unwrap();
    command::run(Command::FormatStructuredDocument, &mut workspace);
    let notice = workspace.notice.clone().unwrap_or_default();
    assert!(notice.contains("mixes text"), "notice: {notice}");

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn a_plain_text_file_has_nothing_to_validate() {
    let directory = scratch("plain");
    let mut workspace = open(&directory, "notes.txt", "hello\n");

    command::run(Command::ValidateDocument, &mut workspace);
    assert!(workspace
        .notice
        .as_deref()
        .unwrap_or_default()
        .contains("no JSON, TOML, YAML or XML checker"));

    let _ = std::fs::remove_dir_all(&directory);
}
