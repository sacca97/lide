//! The editor's Store over the real agent.

use std::sync::Arc;

use lide_remote::store::AgentStore;
use lide_resource::{Location, Store};

#[test]
fn a_remote_file_is_read_and_written_through_the_agent() {
    let root = std::env::temp_dir().join(format!("lide-agent-store-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("a.txt"), "one\r\ntwo\r\n").unwrap();

    let store = Arc::new(
        AgentStore::local(env!("CARGO_BIN_EXE_lide-agent"), &root).expect("the agent starts"),
    );
    let location = store.location("a.txt");

    // The store hands over the bytes as they are on disk, line endings and
    // all; the editor is what normalises them for editing.
    assert_eq!(
        store.read(&location).unwrap(),
        b"one\r\ntwo\r\n",
        "the remote file is read as it is stored"
    );

    store.write(&location, b"three\r\nfour\r\n").expect("save");
    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).unwrap(),
        "three\r\nfour\r\n",
        "the remote file keeps its line endings"
    );

    assert_eq!(
        store.read(&location).unwrap(),
        b"three\r\nfour\r\n",
        "the write is what the agent holds"
    );

    let other = AgentStore::local(env!("CARGO_BIN_EXE_lide-agent"), &root).unwrap();
    let wrong = Location::remote("another-host", &root, "a.txt");
    assert!(
        Arc::new(other).read(&wrong).is_err(),
        "a location for another host is not served by this store"
    );

    let _ = std::fs::remove_dir_all(&root);
}
