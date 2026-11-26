//! The editor's Store over the real agent.

use std::sync::Arc;

use lide_editor::Editor;
use lide_remote::store::AgentStore;
use lide_resource::Location;

#[test]
fn a_remote_document_opens_edits_and_saves_through_the_agent() {
    let root = std::env::temp_dir().join(format!("lide-agent-store-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("a.txt"), "one\r\ntwo\r\n").unwrap();

    let store = Arc::new(
        AgentStore::local(env!("CARGO_BIN_EXE_lide-agent"), &root).expect("the agent starts"),
    );
    let location = store.location("a.txt");
    let mut editor = Editor::open_with(store.clone(), location.clone()).expect("open");
    assert_eq!(editor.text(), "one\ntwo\n");
    editor.edit.replace_text("three\nfour\n");
    editor.save().expect("save");
    assert!(!editor.dirty());
    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).unwrap(),
        "three\r\nfour\r\n",
        "the remote file keeps its line endings"
    );

    let reopened = Editor::open_with(store, location).unwrap();
    assert_eq!(reopened.text(), "three\nfour\n");

    let other = AgentStore::local(env!("CARGO_BIN_EXE_lide-agent"), &root).unwrap();
    let wrong = Location::remote("another-host", &root, "a.txt");
    assert!(
        Editor::open_with(Arc::new(other), wrong).is_err(),
        "a location for another host is not served by this store"
    );

    let _ = std::fs::remove_dir_all(&root);
}
