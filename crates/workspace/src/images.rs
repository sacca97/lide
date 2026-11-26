//! Image documents.
//!
//! An image opens as its own document and is drawn at its own size, scaled
//! down to fit the window. It cannot be edited, searched or saved — which is
//! what [`lide_documents::Capabilities`] says about it.

use std::path::{Path, PathBuf};

use rune::Image;
use rune::{col, el, row, text, El, ImageCache};
use rune_kit::Kit;

/// A previewable image.
pub struct ImageView {
    /// Where it came from.
    pub path: PathBuf,
    /// The decoded image, once the decoder has run.
    image: Option<Image>,
    /// Why it could not be shown, if it could not.
    error: Option<String>,
    /// Its pixel size, once known.
    size: Option<[u32; 2]>,
    /// Whether the bytes have been handed to the decoder.
    requested: bool,
}

impl std::fmt::Debug for ImageView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageView")
            .field("path", &self.path)
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

impl ImageView {
    /// Open an image. Decoding happens on the first [`ImageView::poll`], when
    /// the cache is available.
    pub fn open(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_path_buf();
        if !path.exists() {
            return Self {
                error: Some(format!("{} does not exist", path.display())),
                path,
                image: None,
                size: None,
                requested: true,
            };
        }
        Self {
            path,
            image: None,
            error: None,
            size: None,
            requested: false,
        }
    }

    /// The path the image was opened from.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The title shown on the tab.
    pub fn title(&self) -> String {
        self.path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "image".to_string())
    }

    /// The image's pixel size, once it has been decoded.
    pub fn size(&self) -> Option<[u32; 2]> {
        self.size
    }

    /// Why the image could not be shown.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Give the bytes to the decoder, and notice when it is done.
    ///
    /// Returns `true` when the view should be redrawn.
    pub fn poll(&mut self, cache: &ImageCache) -> bool {
        if !self.requested {
            self.requested = true;
            match std::fs::read(&self.path) {
                Ok(bytes) => self.image = Some(cache.load(bytes)),
                Err(error) => {
                    self.error = Some(format!("could not read {}: {error}", self.path.display()));
                    return true;
                }
            }
            return true;
        }

        if let Some(image) = &self.image {
            if let Some(size) = image.size() {
                if self.size != Some(size) {
                    self.size = Some(size);
                    return true;
                }
            } else if let Some(error) = image.error() {
                if self.error.is_none() {
                    self.error = Some(error.to_string());
                    return true;
                }
            }
        }
        false
    }

    /// Draw the preview.
    pub fn view(&self, k: &Kit) -> El {
        let mut header = row()
            .w_full()
            .h(40.0)
            .px(8.0)
            .gap(8.0)
            .center_y()
            .fill(k.c.bg_base)
            .child(text(self.title()).font(k.ty.small).color(k.c.text_weak));

        if let Some([width, height]) = self.size {
            header = header.child(
                text(format!("{width} x {height}"))
                    .font(k.ty.small)
                    .color(k.c.text_weak),
            );
        }

        let body: El = if let Some(error) = &self.error {
            col()
                .grow(1.0)
                .min_h(0.0)
                .padding(16.0)
                .child(text(error.clone()).font(k.ty.ui).color(k.c.text_base))
        } else if let Some(image) = &self.image {
            col()
                .key("image-scroll")
                .grow(1.0)
                .min_h(0.0)
                .center()
                .padding(16.0)
                .child(
                    el().key("image")
                        .max_w(760.0)
                        .max_h(560.0)
                        .aspect_fit(image_ratio(self.size))
                        .pixels(image.pixels()),
                )
        } else {
            col()
                .grow(1.0)
                .min_h(0.0)
                .center()
                .child(text("Decoding…").font(k.ty.ui).color(k.c.text_weak))
        };

        col()
            .w_full()
            .h_full()
            .fill(k.c.page)
            .child(header)
            .child(body)
    }
}

fn image_ratio(size: Option<[u32; 2]>) -> f32 {
    match size {
        Some([width, height]) if height > 0 => width as f32 / height as f32,
        _ => 1.0,
    }
}
