//! SVG source and preview, through the real workspace.
//!
//! An SVG is both bytes an editor can hold and an image a reader can see, so
//! the two views are two documents of the same path: the preview, and the
//! source in the editor. Saving the source must redraw the preview.

use std::path::PathBuf;
use std::time::Duration;

use lide_workspace::{Document, Workspace};

/// Keep the tests from reading or writing the real settings file.
fn isolate_settings() {
    crate::isolate_settings();
}

fn scratch(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("lide-images-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

fn svg(width: u32, height: u32) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}">
  <rect width="{width}" height="{height}" fill="#3b82f6"/>
</svg>
"##
    )
}

/// Poll until the preview has decoded, or give up.
fn wait_for_size(workspace: &mut Workspace) -> Option<[u32; 2]> {
    for _ in 0..300 {
        workspace.poll();
        for document in &workspace.documents {
            if let Document::Image(image) = document {
                if let Some(size) = image.size() {
                    return Some(size);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    None
}

#[test]
fn an_svg_has_a_preview_and_a_source() {
    isolate_settings();
    let directory = scratch("toggle");
    let path = directory.join("icon.svg");
    std::fs::write(&path, svg(120, 60)).unwrap();

    let mut workspace = Workspace::new();
    workspace.open_path(&path).expect("the SVG opens");
    assert!(
        matches!(workspace.documents[workspace.active], Document::Image(_)),
        "an SVG opens as its preview"
    );

    workspace.toggle_image_source();
    assert_eq!(workspace.documents.len(), 2, "the source opens beside it");
    assert!(
        matches!(workspace.documents[workspace.active], Document::Text(_)),
        "the source is the active document"
    );
    assert_eq!(
        workspace.active_document().and_then(|document| document.path()),
        Some(path.as_path())
    );

    // Toggling back focuses the preview instead of opening a third copy.
    workspace.toggle_image_source();
    assert_eq!(workspace.documents.len(), 2);
    assert!(matches!(
        workspace.documents[workspace.active],
        Document::Image(_)
    ));

    // A document that is neither an SVG nor its preview says so.
    let text = directory.join("notes.txt");
    std::fs::write(&text, "hello").unwrap();
    workspace.open_path(&text).unwrap();
    workspace.toggle_image_source();
    assert_eq!(
        workspace.notice.as_deref(),
        Some("the active document has no SVG source")
    );

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn saving_the_source_redraws_the_preview() {
    isolate_settings();
    let directory = scratch("refresh");
    let path = directory.join("icon.svg");
    std::fs::write(&path, svg(120, 60)).unwrap();

    let mut workspace = Workspace::new();
    workspace.images = Some(rune_image::ImageCache::new(std::sync::Arc::new(|| {})));
    workspace.open_path(&path).expect("the SVG opens");

    let size = wait_for_size(&mut workspace).expect("the preview decodes");
    assert!(
        (size[0] as f32 / size[1] as f32 - 2.0).abs() < 0.05,
        "the fixture is twice as wide as it is tall: {size:?}"
    );

    // Edit the source to a square, and save it.
    workspace.toggle_image_source();
    assert_eq!(
        workspace.documents[workspace.active].path(),
        Some(path.as_path())
    );
    let source = workspace
        .active_editor_mut()
        .expect("the source is editable");
    source.edit.replace_text(svg(100, 100));
    let index = workspace.active;
    workspace.save(index);

    let size = wait_for_size(&mut workspace).expect("the preview decodes again");
    assert!(
        (size[0] as f32 / size[1] as f32 - 1.0).abs() < 0.05,
        "the saved square replaced the old preview: {size:?}"
    );

    let _ = std::fs::remove_dir_all(&directory);
}
