//! The editor's dialog: the kit's, with a body that has room to breathe.
//!
//! The kit's dialog pads its header and nothing else, so a field or a row of
//! buttons sits against the dialog's edge. Every dialog here gets the same
//! body: 16 points at the sides and bottom, 12 between its parts. The palette
//! pads its own contents and asks for none of that with [`Dialog::bare`].

use rune::{col, El};
use rune_kit::Kit;

pub struct Dialog {
    inner: rune_kit::Dialog,
    body: Vec<El>,
    bare: bool,
}

impl Dialog {
    pub fn new(key: impl Into<String>, title: impl Into<String>) -> Self {
        Self { inner: rune_kit::Dialog::new(key, title), body: Vec::new(), bare: false }
    }

    pub fn open(mut self, open: bool) -> Self {
        self.inner = self.inner.open(open);
        self
    }

    pub fn fit(mut self) -> Self {
        self.inner = self.inner.fit();
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.inner = self.inner.width(width);
        self
    }

    pub fn click_outside(mut self, close: bool) -> Self {
        self.inner = self.inner.click_outside(close);
        self
    }

    pub fn on_close<S: 'static>(mut self, f: impl Fn(&mut S) + Clone + 'static) -> Self {
        self.inner = self.inner.on_close(f);
        self
    }

    pub fn child(mut self, body: El) -> Self {
        self.body.push(body);
        self
    }

    /// Leave the body unpadded: the caller lays out to the edges itself.
    pub fn bare(mut self) -> Self {
        self.bare = true;
        self
    }

    pub fn build(self, k: &Kit, layer: &mut Vec<El>) {
        let body = if self.bare {
            self.body
        } else {
            vec![col().w_full().px(16.0).pb(16.0).gap(12.0).children(self.body)]
        };
        let mut inner = self.inner;
        for child in body {
            inner = inner.child(child);
        }
        inner.build(k, layer);
    }
}
