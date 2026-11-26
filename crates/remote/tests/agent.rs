//! The agent and client, end to end, over the real framed protocol.

use std::path::PathBuf;

use lide_remote::client::Remote;
use lide_remote::protocol::READ_CHUNK;

fn scratch(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("lide-agent-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

fn agent(root: &PathBuf) -> Remote {
    Remote::spawn_local(env!("CARGO_BIN_EXE_lide-agent"), root).expect("the agent starts")
}

#[test]
fn the_agent_lists_reads_and_writes_inside_its_root() {
    let root = scratch("files");
    std::fs::write(root.join("a.txt"), "hello\n").unwrap();

    let mut remote = agent(&root);
    let hello = remote.hello().expect("initialize");
    assert_eq!(hello.protocol, 1);

    let entries = remote.list("").expect("list");
    assert!(entries.iter().any(|entry| entry.name == "a.txt"));

    assert_eq!(remote.read("a.txt").unwrap(), b"hello\n");

    remote
        .write("missing/b.bin", b"not yet")
        .expect_err("parent is missing");
    remote.mkdir("sub").expect("mkdir");
    remote.write("sub/b.bin", &[0, 1, 2, 255]).unwrap();
    assert_eq!(remote.read("sub/b.bin").unwrap(), vec![0, 1, 2, 255]);

    let stat = remote.stat("sub/b.bin").unwrap();
    assert_eq!(stat.size, 4);
    assert!(!stat.is_dir);

    remote.rename("a.txt", "sub/a.txt").expect("rename");
    assert!(!root.join("a.txt").exists());
    assert_eq!(std::fs::read(root.join("sub/a.txt")).unwrap(), b"hello\n");

    remote.remove("sub", true).expect("remove directory");
    assert!(!root.join("sub").exists());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_file_larger_than_a_chunk_is_read_whole() {
    let root = scratch("chunks");
    let data: Vec<u8> = (0..(READ_CHUNK + 123))
        .map(|index| (index % 251) as u8)
        .collect();
    std::fs::write(root.join("large.bin"), &data).unwrap();

    let mut remote = agent(&root);
    assert_eq!(remote.read("large.bin").unwrap(), data);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn paths_cannot_escape_the_root() {
    let root = scratch("escape");
    let outside = scratch("outside");
    std::fs::write(outside.join("secret"), "no").unwrap();

    let mut remote = agent(&root);
    assert!(remote.read("../outside/secret").is_err());
    assert!(remote.write("../escape", b"x").is_err());
    assert!(remote.list("..").is_err());
    assert!(
        remote.remove("", true).is_err(),
        "the root itself is protected"
    );

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside.join("secret"), root.join("link")).unwrap();
        assert!(remote.read("link").is_err(), "a symlink out is refused");
        assert!(
            remote.write("link", b"pwned").is_err(),
            "a write through a symlink out is refused"
        );
        assert_eq!(std::fs::read(outside.join("secret")).unwrap(), b"no");
    }

    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&outside);
}

#[test]
fn a_missing_path_is_an_agent_error() {
    let root = scratch("missing");
    let mut remote = agent(&root);
    let error = remote.read("absent").unwrap_err();
    let text = error.to_string();
    assert!(text.contains("agent error"), "{text}");
    let _ = std::fs::remove_dir_all(&root);
}
