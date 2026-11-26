//! The PDF viewer.
//!
//! The viewer renders one page at a time. Rendering happens on a background
//! thread — PDFium can take tens of milliseconds for a complex page, which
//! is long enough to make the window stutter if it happened on the UI
//! thread — and the finished page is decoded into an image the runtime can
//! draw.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;

use rune_kit::{Button, ControlSize, Kit};
use rune::{El, Image, ImageCache, col, el, row, text};

use rune_edit::TextEdit;

use crate::document::{PdfDocument, PdfError};
use crate::render::{PdfiumEngine, TextChar};

/// The smallest zoom the viewer allows.
const MIN_SCALE: f32 = 0.25;
/// The largest zoom the viewer allows.
const MAX_SCALE: f32 = 8.0;
/// How much one zoom step changes the scale.
const ZOOM_STEP: f32 = 1.25;
/// How many rendered pages (or zoom levels) to keep for revisiting.
const CACHED_PAGES: usize = 8;
/// How many bytes of rendered pages to keep before dropping the oldest.
const CACHE_BYTES: usize = 128 * 1024 * 1024;

/// A request to render a page, sent to the rendering thread.
#[derive(Debug, Clone, Copy)]
struct RenderRequest {
    /// The page to render.
    page: u32,
    /// The zoom factor.
    scale: f32,
}

/// The result of a render: the page's pixels, ready to draw.
#[derive(Debug)]
struct RenderResult {
    page: u32,
    /// The scale the page was rendered at.
    scale: f32,
    /// The width of the bitmap in pixels.
    width: u32,
    /// The height of the bitmap in pixels.
    height: u32,
    /// The RGBA pixels.
    rgba: Option<Vec<u8>>,
    /// The page's text, with every character's box in PDF points.
    text: Vec<crate::render::TextChar>,
    error: Option<String>,
}

/// A rendered page kept for the next time it is shown.
struct CachedPage {
    page: u32,
    /// The scale the bitmap was rendered at.
    scale: f32,
    bitmap: PageBitmap,
}

/// A PDF that is open in the workspace.
pub struct PdfView {
    document: PdfDocument,
    path: PathBuf,
    page: u32,
    /// The zoom factor the page is drawn at.
    scale: f32,
    /// The scale of the bitmap currently drawn. Until a new render arrives, the
    /// old bitmap is stretched to the new zoom, so pinch and the zoom buttons
    /// move at once rather than waiting for the renderer.
    image_scale: f32,
    /// The rendered page.
    image: Option<Image>,
    /// The channel used to ask the rendering thread for a page.
    requests: Option<mpsc::SyncSender<RenderRequest>>,
    /// The results that came back from the rendering thread.
    inbox: mpsc::Receiver<RenderResult>,
    /// The last request handed to the rendering thread, so that a burst of
    /// zoom changes coalesces instead of queueing a render for every step.
    requested: Option<(u32, f32)>,
    /// The rendering thread. It ends when the request channel is dropped;
    /// holding the handle keeps it alive for as long as the view is.
    _worker: Option<std::thread::JoinHandle<()>>,
    /// The error shown to the user, if any.
    error: Option<String>,
    /// The pages that have been rendered, by page and scale.
    rendered: Vec<CachedPage>,
    /// The page and scale whose image is currently drawn.
    shown: Option<(u32, f32)>,
    /// The scroll offset the viewport last reported, and its maximum. The
    /// wheel handler uses them to scroll, and to turn past the last line.
    scroll_y: Rc<Cell<f32>>,
    scroll_max: Rc<Cell<f32>>,
    /// Where the wheel is sending the scroll viewport. Keeping this separate
    /// from the actual offset lets a burst of wheel events accumulate while
    /// the previous glide is still moving.
    scroll_target: Rc<Cell<f32>>,
    /// A page that was rendered but not handed to the image cache yet.
    pending: Option<(f32, PageBitmap)>,
    /// The text of each page that has been rendered, by page number.
    texts: std::collections::HashMap<u32, Vec<crate::render::TextChar>>,
    /// The text being searched for.
    pub search: TextEdit,
    /// The boxes of the matches on the current page, in PDF points.
    pub matches: Vec<(f32, f32, f32, f32)>,
    /// The match the viewer is on.
    pub selected: usize,
    /// The selected characters, as a range of indices into the current page's
    /// text layer.
    pub selection: Option<(usize, usize)>,
    /// Where the pointer last was on the page, in PDF points. Used by SyncTeX
    /// to ask what is under it.
    pub pointer: Option<(f32, f32)>,
    /// The document's bookmarks, as `(title, page, depth)`.
    pub outline: Vec<(String, u32, usize)>,
    /// A double click asked the workspace to jump back to the source.
    pub double_click: bool,
}

/// A rendered page: its size in pixels and its RGBA pixels.
type PageBitmap = (u32, u32, Vec<u8>);

impl std::fmt::Debug for PdfView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PdfView")
            .field("path", &self.path)
            .field("page", &self.page)
            .field("scale", &self.scale)
            .finish_non_exhaustive()
    }
}

impl PdfView {
    /// Open a PDF.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, PdfError> {
        let path = path.as_ref().to_path_buf();
        let document = PdfDocument::open(&path)?;

        let (request_tx, request_rx) = mpsc::sync_channel::<RenderRequest>(1);
        let (result_tx, result_rx) = mpsc::channel::<RenderResult>();
        let worker_path = path.clone();
        let worker =
            std::thread::spawn(move || render_worker(worker_path, request_rx, result_tx));

        let mut view = Self {
            document,
            path,
            page: 0,
            scale: 1.0,
            image_scale: 1.0,
            image: None,
            requests: Some(request_tx),
            inbox: result_rx,
            requested: None,
            _worker: Some(worker),
            error: None,
            rendered: Vec::new(),
            shown: None,
            scroll_y: Rc::new(Cell::new(0.0)),
            scroll_max: Rc::new(Cell::new(0.0)),
            scroll_target: Rc::new(Cell::new(0.0)),
            pending: None,
            texts: std::collections::HashMap::new(),
            search: TextEdit::new(""),
            matches: Vec::new(),
            selected: 0,
            selection: None,
            pointer: None,
            outline: Vec::new(),
            double_click: false,
        };

        // The first page is rendered up front so that the document shows
        // something as soon as it is opened. Later pages are rendered in the
        // background.
        view.render_first_page();
        Ok(view)
    }

    /// The path the document was opened from.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The title shown on the tab.
    pub fn title(&self) -> String {
        self.path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "untitled.pdf".to_string())
    }

    /// The page that is currently shown.
    pub fn page(&self) -> u32 {
        self.page
    }

    /// The number of pages.
    pub fn page_count(&self) -> u32 {
        self.document.page_count()
    }

    /// The zoom factor.
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// The error shown to the user, if any.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Go to the next page.
    pub fn next_page(&mut self) {
        if self.page + 1 < self.page_count() {
            self.set_page(self.page + 1);
        }
    }

    /// Go to the previous page.
    pub fn previous_page(&mut self) {
        if self.page > 0 {
            self.set_page(self.page - 1);
        }
    }

    /// Zoom in one step.
    pub fn zoom_in(&mut self) {
        self.set_scale(self.scale * ZOOM_STEP);
    }

    /// Zoom out one step.
    pub fn zoom_out(&mut self) {
        self.set_scale(self.scale / ZOOM_STEP);
    }

    /// Return to actual size.
    pub fn reset_zoom(&mut self) {
        self.set_scale(1.0);
    }

    /// Zoom by a factor, for a pinch or a Ctrl+wheel.
    pub fn zoom_by(&mut self, factor: f32) {
        if factor.is_finite() && factor > 0.0 {
            self.set_scale(self.scale * factor);
        }
    }

    /// Set the zoom factor, clamped to what the viewer supports.
    fn set_scale(&mut self, scale: f32) {
        let scale = scale.clamp(MIN_SCALE, MAX_SCALE);
        if (scale - self.scale).abs() < 0.0001 {
            return;
        }
        self.scale = scale;
        self.shown = None;
        self.request_page();
    }

    /// Collect the results of finished renders.
    ///
    /// This is called once per frame. It returns `true` when something
    /// changed and the view should be redrawn.
    pub fn poll(&mut self, cache: &ImageCache) -> bool {
        let mut changed = false;
        if let Some((scale, bitmap)) = self.pending.take() {
            self.rendered.push(CachedPage {
                page: self.page,
                scale,
                bitmap,
            });
            changed = true;
        }
        while let Ok(result) = self.inbox.try_recv() {
            changed = true;
            match (result.rgba, result.error) {
                (Some(rgba), _) => {
                    self.texts.insert(result.page, result.text);
                    // One scale per page is enough: a zoom step that was
                    // overtaken while the renderer worked need not stay.
                    self.rendered.retain(|cached| {
                        cached.page != result.page || cached.scale == result.scale
                    });
                    self.rendered.push(CachedPage {
                        page: result.page,
                        scale: result.scale,
                        bitmap: (result.width, result.height, rgba),
                    });
                    self.trim_cache();
                }
                (None, Some(error)) => self.error = Some(error),
                (None, None) => {}
            }
        }

        if changed && !self.search.text.is_empty() {
            let query = self.search.text.clone();
            self.search_for(&query);
        }

        // A request may have been dropped while the renderer was busy with
        // the previous scale. Ask again for whatever is on screen now.
        self.request_page();

        // Show the current page as soon as it is available, whether it was
        // just rendered or was rendered earlier.
        if self.shown != Some((self.page, self.scale)) {
            if let Some(cached) = self
                .rendered
                .iter()
                .find(|cached| cached.page == self.page && cached.scale == self.scale)
            {
                let (width, height, rgba) = cached.bitmap.clone();
                let scale = cached.scale;
                self.load_page(width, height, rgba, scale, cache);
                self.shown = Some((self.page, self.scale));
                changed = true;
            }
        }

        changed
    }

    /// Drop the oldest cached pages once the cache is large.
    fn trim_cache(&mut self) {
        let mut bytes: usize = self
            .rendered
            .iter()
            .map(|cached| cached.bitmap.2.len())
            .sum();
        while (self.rendered.len() > CACHED_PAGES || bytes > CACHE_BYTES) && self.rendered.len() > 1
        {
            let index = self
                .rendered
                .iter()
                .position(|cached| cached.page != self.page)
                .unwrap_or(0);
            bytes = bytes.saturating_sub(self.rendered[index].bitmap.2.len());
            self.rendered.remove(index);
        }
    }

    /// Render the first page on the calling thread.
    fn render_first_page(&mut self) {
        let Ok(engine) = PdfiumEngine::load() else {
            self.error = Some(
                "PDFium was not found. Run scripts/fetch-pdfium.sh or set LIDE_PDFIUM_PATH."
                    .to_string(),
            );
            return;
        };
        let Ok(document) = engine.open(&self.path) else {
            return;
        };
        let Ok(page) = document.render_page(self.page, self.scale) else {
            return;
        };
        // The text layer comes with the first page too, so that searching works
        // before the background worker has been asked for anything.
        if let Ok(text) = document.page_text(self.page) {
            self.texts.insert(self.page, text.chars);
        }
        // The outline comes with the first page too, so that navigation is
        // there before anything else has been asked for.
        self.outline = document.outline();
        self.pending = Some((self.scale, (page.width, page.height, page.rgba)));
        self.requested = Some((self.page, self.scale));
    }

    /// Ask the rendering thread for the current page.
    ///
    /// The channel holds a single request, so a burst of zoom steps collapses
    /// into the scale that is on screen when the renderer is ready for it.
    fn request_page(&mut self) {
        let target = (self.page, self.scale);
        if self.requested == Some(target) {
            return;
        }
        if self
            .rendered
            .iter()
            .any(|cached| cached.page == self.page && cached.scale == self.scale)
        {
            return;
        }
        let Some(sender) = self.requests.as_ref() else {
            return;
        };
        if sender
            .try_send(RenderRequest {
                page: self.page,
                scale: self.scale,
            })
            .is_ok()
        {
            self.requested = Some(target);
        }
    }

    /// Hand a rendered page to the image cache.
    ///
    /// The wait makes the page appear in the same frame it was decoded,
    /// rather than a frame later. The bitmap has already been rendered by
    /// this point, so only the conversion to texture data is waited for.
    fn load_page(
        &mut self,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
        scale: f32,
        cache: &ImageCache,
    ) {
        let image = cache.rgba(width, height, rgba);
        let _ = image.wait();
        self.image = Some(image);
        self.image_scale = scale;
    }

    /// Find `query` on the pages that have been rendered.
    ///
    /// Matching is case-insensitive, like the editor's find bar, and it works
    /// on the text layer rather than the picture, so a match is a real word
    /// and its box is where the glyph is.
    pub fn search_for(&mut self, query: &str) {
        self.matches.clear();
        self.selected = 0;
        let Some(chars) = self.texts.get(&self.page) else {
            return;
        };
        self.matches = find_matches(chars, query);
    }

    /// Select from one point on the page to another.
    ///
    /// The points are in PDF points — origin at the bottom left — which is
    /// what the pointer positions become once the page's height is subtracted
    /// from their y.
    pub fn select(&mut self, start: (f32, f32), end: (f32, f32)) {
        let Some(chars) = self.texts.get(&self.page) else {
            return;
        };
        let (Some(first), Some(last)) = (
            char_at(chars, start.0, start.1),
            char_at(chars, end.0, end.1),
        ) else {
            return;
        };
        self.selection = Some((first.min(last), first.max(last) + 1));
    }

    /// Select the first occurrence of `needle` on the current page.
    ///
    /// The same machinery the pointer uses, driven by text instead: useful for
    /// tests, and for "select this search match".
    pub fn select_text(&mut self, needle: &str) -> bool {
        let Some(chars) = self.texts.get(&self.page) else {
            return false;
        };
        let wanted: Vec<char> = needle.chars().collect();
        if wanted.is_empty() || chars.len() < wanted.len() {
            return false;
        }
        for start in 0..=chars.len() - wanted.len() {
            if chars[start..start + wanted.len()]
                .iter()
                .map(|c| c.ch)
                .eq(wanted.iter().copied())
            {
                self.selection = Some((start, start + wanted.len()));
                return true;
            }
        }
        false
    }

    /// Go to a page, for SyncTeX and for the page box.
    pub fn go_to_page(&mut self, page: u32) {
        if page < self.page_count() {
            self.set_page(page);
        }
    }

    /// Move to a page and reset the per-page state.
    fn set_page(&mut self, page: u32) {
        if page == self.page {
            return;
        }
        self.page = page;
        self.shown = None;
        self.image = None;
        self.selection = None;
        self.matches.clear();
        self.selected = 0;
        self.scroll_y.set(0.0);
        self.scroll_target.set(0.0);
        if !self.search.text.is_empty() {
            let query = self.search.text.clone();
            self.search_for(&query);
        }
        self.request_page();
    }

    /// Clear the selection.
    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    /// The selected text, with a line break where the selection crosses one.
    pub fn selected_text(&self) -> String {
        let Some((start, end)) = self.selection else {
            return String::new();
        };
        let Some(chars) = self.texts.get(&self.page) else {
            return String::new();
        };
        let end = end.min(chars.len());
        if start >= end {
            return String::new();
        }

        let mut text = String::new();
        let mut previous_bottom = chars[start].bottom;
        for character in &chars[start..end] {
            // A new line starts when the baseline moves: the text layer has no
            // newline characters in it.
            if (character.bottom - previous_bottom).abs() > character.height().max(1.0) {
                text.push('\n');
            } else if let Some(last) = text.chars().last() {
                // A visible gap between glyphs is a space the layer did not emit.
                if let Some(previous) = chars.get(start) {
                    let _ = previous;
                }
                let _ = last;
            }
            previous_bottom = character.bottom;
            text.push(character.ch);
        }
        text
    }

    /// Move to the next match on the page.
    pub fn next_match(&mut self) {
        if !self.matches.is_empty() {
            self.selected = (self.selected + 1) % self.matches.len();
        }
    }

    /// The number of matches on the current page.
    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    /// Draw the viewer.
    ///
    /// The handlers mutate the document, but Rune's events are delivered to
    /// the application's state (`Workspace`), not to this view. `lens` finds
    /// the document in that state; it is cloned into each handler.
    pub fn view<S: 'static>(
        &self,
        k: &Kit,
        pane: usize,
        lens: impl Fn(&mut S) -> &mut PdfView + Clone + 'static,
    ) -> El {
        let scroll_key = format!("pdf-scroll-{pane}");
        let mut header = row()
            .w_full()
            .h(40.0)
            .px(10.0)
            .gap(8.0)
            .center_y()
            .fill(k.c.bg_base)
            .child(
                row()
                    .gap(3.0)
                    .center_y()
                    .child({
                        let lens = lens.clone();
                        let reset = scroll_key.clone();
                        Button::new("‹")
                            .control_size(ControlSize::Mini)
                            .help("Previous page")
                            .disabled(self.page == 0)
                            .on_press_tx(move |s: &mut S, tx: &mut rune::Tx| {
                                lens(s).previous_page();
                                tx.scroll_to_instant(&reset, 0.0, 0.0);
                            })
                            .build(k)
                            .key(format!("pdf-prev-page-{pane}"))
                    })
                    .child(
                        text(format!("{} / {}", self.page + 1, self.page_count()))
                            .font(k.ty.small)
                            .color(k.c.text_weak),
                    )
                    .child({
                        let lens = lens.clone();
                        let reset = scroll_key.clone();
                        Button::new("›")
                            .control_size(ControlSize::Mini)
                            .help("Next page")
                            .disabled(self.page + 1 >= self.page_count())
                            .on_press_tx(move |s: &mut S, tx: &mut rune::Tx| {
                                lens(s).next_page();
                                tx.scroll_to_instant(&reset, 0.0, 0.0);
                            })
                            .build(k)
                            .key(format!("pdf-next-page-{pane}"))
                    }),
            )
            .child(el().w(1.0).h(14.0).fill(k.c.border_base))
            .child(
                row()
                    .gap(3.0)
                    .center_y()
                    .child({
                        let lens = lens.clone();
                        Button::new("−")
                            .control_size(ControlSize::Mini)
                            .help("Zoom out")
                            .disabled(self.scale <= MIN_SCALE)
                            .on_press(move |s: &mut S| lens(s).zoom_out())
                            .build(k)
                            .key(format!("pdf-zoom-out-{pane}"))
                    })
                    .child({
                        let lens = lens.clone();
                        Button::new(format!("{:.0}%", self.scale * 100.0))
                            .control_size(ControlSize::Mini)
                            .help("Actual size")
                            .on_press(move |s: &mut S| lens(s).reset_zoom())
                            .build(k)
                            .key(format!("pdf-zoom-reset-{pane}"))
                    })
                    .child({
                        let lens = lens.clone();
                        Button::new("+")
                            .control_size(ControlSize::Mini)
                            .help("Zoom in")
                            .disabled(self.scale >= MAX_SCALE)
                            .on_press(move |s: &mut S| lens(s).zoom_in())
                            .build(k)
                            .key(format!("pdf-zoom-in-{pane}"))
                    }),
            )
            .child({
                let on_edit = lens.clone();
                let on_change = lens.clone();
                rune_kit::TextInput::new(&self.search)
                    .placeholder("Find in page")
                    .width(Some(150.0))
                    .on_edit(move |s: &mut S| &mut on_edit(s).search)
                    .on_change(move |s: &mut S, _tx| {
                        let view = on_change(s);
                        let query = view.search.text.clone();
                        view.search_for(&query);
                    })
                    .build(k)
                    .key(format!("pdf-find-{pane}"))
            });

        if !self.matches.is_empty() {
            let lens = lens.clone();
            header = header
                .child(
                    text(format!("{} of {}", self.selected + 1, self.matches.len()))
                        .font(k.ty.small)
                        .color(k.c.text_weak),
                )
                .child(
                    rune_kit::Button::new("Next")
                        .help("Next match")
                        .on_press(move |s: &mut S| lens(s).next_match())
                        .build(k),
                );
        }

        let mut outline = col()
            .w(170.0)
            .shrink()
            .gap(1.0)
            .px(6.0)
            .py(4.0)
            .child(
                text("Outline")
                    .font(k.ty.small_medium)
                    .color(k.c.text_muted)
                    .px(6.0)
                    .pb(4.0),
            );
        for (index, (title, page, depth)) in self.outline.iter().enumerate() {
            let page = *page;
            let to_page = lens.clone();
            let reset = scroll_key.clone();
            let mut entry = text(title.clone())
                .key(format!("bookmark-{pane}-{index}"))
                .font(k.ty.text_xsmall)
                .w_full()
                .min_w(0.0)
                .truncate()
                .px(6.0)
                .py(2.0)
                .radius(5.0)
                .pl(6.0 + 10.0 * (*depth).min(4) as f32)
                .on_press_tx(move |s: &mut S, tx: &mut rune::Tx| {
                    to_page(s).go_to_page(page);
                    tx.scroll_to_instant(&reset, 0.0, 0.0);
                });
            if self.page == page {
                entry = entry.fill(k.c.surface_base);
            }
            outline = outline.child(entry);
        }

        let body: El = if let Some(error) = &self.error {
            col()
                .grow(1.0)
                .min_h(0.0)
                .padding(16.0)
                .child(
                    text(error.clone())
                        .font(k.ty.ui)
                        .color(k.c.danger.fg),
                )
        } else if let Some(image) = &self.image {
            // The bitmap is `image_scale` pixels per PDF point. It is drawn at
            // the viewer's zoom, so a page is its own width times the zoom;
            // while a new render is on its way the old bitmap is stretched, and
            // pinch or the zoom buttons move without waiting for the renderer.
            let [width, height] = image.size().unwrap_or([0, 0]);
            let zoom = self.scale;
            let image_scale = self.image_scale.max(0.01);
            let page_width = width as f32 / image_scale * zoom;
            let page_height = height as f32 / image_scale * zoom;

            // A match's box is in PDF points, with the origin at the bottom
            // left; the picture's origin is the top left.
            let mut highlights = col();

            // The selection first, so that the search highlights show through
            // it if they overlap.
            if let (Some((start, end)), Some(chars)) = (self.selection, self.texts.get(&self.page))
            {
                for (index, (left, bottom, width, height)) in
                    selection_boxes(chars, start, end).iter().enumerate()
                {
                    highlights = highlights.child(
                        el().key(format!("selection-{pane}-{index}"))
                            .absolute(*left * zoom, page_height - (*bottom + *height) * zoom)
                            .w(*width * zoom)
                            .h(*height * zoom)
                            .fill(k.c.text_accent.alpha(0.35)),
                    );
                }
            }

            for (index, (left, bottom, width, height)) in self.matches.iter().enumerate() {
                let selected = index == self.selected;
                highlights = highlights.child(
                    el().key(format!("match-{pane}-{index}"))
                        .absolute(*left * zoom, page_height - (*bottom + *height) * zoom)
                        .w(*width * zoom)
                        .h(*height * zoom)
                        .fill(if selected {
                            k.c.text_accent.alpha(0.45)
                        } else {
                            k.c.warning.fg.alpha(0.3)
                        }),
                );
            }

            let drag_lens = lens.clone();
            let magnify_lens = lens.clone();
            let double_lens = lens.clone();
            let page = el()
                .key(format!("pdf-page-{pane}"))
                .w(page_width)
                .h(page_height)
                .pixels(image.pixels())
                .child(highlights)
                // Dragging on the page selects text. The handler gets the
                // pointer in the element's own points, measured from its top
                // left, so the y is flipped to reach PDF's bottom-left origin.
                .on_drag(move |s: &mut S, drag: &mut rune::Drag, tx: &mut rune::Tx| {
                    let view = drag_lens(s);
                    let page_height = drag.h;
                    let start = (drag.start_x, page_height - drag.start_y);
                    let end = (drag.x, page_height - drag.y);
                    view.pointer = Some(end);
                    view.select(start, end);

                    if drag.phase == rune::Phase::Up {
                        let text = view.selected_text();
                        if !text.trim().is_empty() {
                            tx.copy(text);
                        }
                    }
                })
                // A trackpad pinch or Ctrl+wheel zooms around the page. The
                // renderer coalesces the steps; until it catches up the old
                // bitmap is stretched, so the gesture stays smooth.
                .on_magnify(
                    move |s: &mut S, event: rune::MagnifyEvent, _tx: &mut rune::Tx| {
                        magnify_lens(s).zoom_by(1.0 + event.delta);
                        true
                    },
                )
                // Double-clicking a word on the page asks the workspace to
                // jump back to the matching source line through SyncTeX.
                .on_double_press(move |s: &mut S| {
                    double_lens(s).double_click = true;
                });
            let track_y = self.scroll_y.clone();
            let track_max = self.scroll_max.clone();
            let track_target = self.scroll_target.clone();
            col()
                .key(scroll_key.clone())
                .grow(1.0)
                .min_h(0.0)
                .scroll(rune::ScrollSpec {
                    stick_bottom: false,
                })
                .on_scroll_offset(move |at: rune::ScrollPos| {
                    let actual = at.offset[1];
                    let target = track_target.get();
                    let previous = track_y.get();
                    track_y.set(actual);
                    track_max.set(at.max[1]);
                    // A scrollbar drag or a fling can move the viewport away
                    // from where the wheel left it; take that place as the new
                    // target. A glide toward the target only shrinks the gap.
                    if (actual - target).abs() > (previous - target).abs() + 0.5
                        || target > at.max[1]
                    {
                        track_target.set(actual.min(at.max[1]));
                    }
                })
                .center_y()
                .padding(16.0)
                .child(page)
        } else {
            col()
                .grow(1.0)
                .min_h(0.0)
                .center()
                .child(
                    text("Rendering page…")
                        .font(k.ty.ui)
                        .color(k.c.text_weak),
                )
        };

        let mut document = col().w_full().h_full().fill(k.c.page).child(header);
        let wheel_lens = lens.clone();
        let wheel_key = scroll_key.clone();
        let wheel_max = self.scroll_max.clone();
        let wheel_target = self.scroll_target.clone();
        document = document.on_wheel(
            move |s: &mut S, event: rune::WheelEvent, tx: &mut rune::Tx| {
                // Ctrl+wheel is a magnify event handled on the page; leave it
                // alone if the platform sends it as a wheel too.
                if event.mods.ctrl || event.mods.cmd || event.dy == 0.0 {
                    return false;
                }
                if event.dx.abs() > event.dy.abs() {
                    return false;
                }

                let view = wheel_lens(s);
                let max = wheel_max.get();
                let target = wheel_target.get();
                let at_end = target >= max - 0.5;
                let at_start = target <= 0.5;
                let next = (target - event.dy).clamp(0.0, max);

                // A wheel past the end of the page turns the page. This also
                // makes the wheel work when the whole page already fits.
                if event.dy < 0.0 && at_end {
                    view.next_page();
                    tx.scroll_to_instant(&wheel_key, 0.0, 0.0);
                } else if event.dy > 0.0 && at_start {
                    view.previous_page();
                    tx.scroll_to_instant(&wheel_key, 0.0, 0.0);
                } else {
                    // Keep the target, not the drawn offset: a burst of wheel
                    // events accumulates even while the previous glide moves.
                    wheel_target.set(next);
                    tx.scroll_to(&wheel_key, 0.0, next);
                }
                true
            },
        );
        if self.outline.is_empty() {
            document = document.child(body);
        } else {
            document = document.child(
                row()
                    .w_full()
                    .grow(1.0)
                    .min_h(0.0)
                    .child(outline)
                    .child(col().w(1.0).h_full().fill(k.c.border_base))
                    .child(col().grow(1.0).min_w(0.0).h_full().child(body)),
            );
        }
        document
    }
}

/// The index of the character nearest a point on the page.
///
/// A point inside a glyph's box picks that glyph. Otherwise the nearest one
/// wins, with the vertical distance weighted so that being on the right line
/// matters more than being at the right column — which is what makes a drag
/// down the margin select whole lines rather than jump about.
pub fn char_at(chars: &[TextChar], x: f32, y: f32) -> Option<usize> {
    if chars.is_empty() {
        return None;
    }
    if let Some(index) = chars
        .iter()
        .position(|c| x >= c.left && x <= c.right && y >= c.bottom && y <= c.top)
    {
        return Some(index);
    }

    let mut best: Option<usize> = None;
    let mut best_score = f32::MAX;
    for (index, character) in chars.iter().enumerate() {
        let centre_x = (character.left + character.right) / 2.0;
        let centre_y = (character.bottom + character.top) / 2.0;
        let score = (centre_y - y).abs() * 10.0 + (centre_x - x).abs();
        if score < best_score {
            best_score = score;
            best = Some(index);
        }
    }
    best
}

/// The boxes that cover a selection, one per run of characters on a line.
pub fn selection_boxes(
    chars: &[TextChar],
    start: usize,
    end: usize,
) -> Vec<(f32, f32, f32, f32)> {
    let end = end.min(chars.len());
    if start >= end {
        return Vec::new();
    }

    let mut boxes = Vec::new();
    let mut run_start = start;
    for index in start + 1..end {
        let previous = &chars[index - 1];
        let current = &chars[index];
        let same_line = (current.bottom - previous.bottom).abs() <= previous.height().max(1.0) / 2.0;
        if !same_line {
            boxes.push(run_box(&chars[run_start..index]));
            run_start = index;
        }
    }
    boxes.push(run_box(&chars[run_start..end]));
    boxes
}

fn run_box(chars: &[TextChar]) -> (f32, f32, f32, f32) {
    let left = chars.iter().map(|c| c.left).fold(f32::MAX, f32::min);
    let right = chars.iter().map(|c| c.right).fold(f32::MIN, f32::max);
    let bottom = chars.iter().map(|c| c.bottom).fold(f32::MAX, f32::min);
    let top = chars.iter().map(|c| c.top).fold(f32::MIN, f32::max);
    (left, bottom, right - left, top - bottom)
}

/// The boxes of `query` in a page's characters, in PDF points.
///
/// Matching is case-insensitive and runs on the text layer, so a match is a
/// real word rather than a run of pixels, and its box is where the glyphs
/// are. The box of a multi-character match is the union of its characters'.
pub fn find_matches(chars: &[crate::render::TextChar], query: &str) -> Vec<(f32, f32, f32, f32)> {
    let needle: Vec<char> = query.to_lowercase().chars().collect();
    if needle.is_empty() || chars.len() < needle.len() {
        return Vec::new();
    }
    let haystack: Vec<char> = chars
        .iter()
        .map(|c| c.ch.to_lowercase().next().unwrap_or(c.ch))
        .collect();

    let mut matches = Vec::new();
    for start in 0..=haystack.len() - needle.len() {
        if haystack[start..start + needle.len()] != needle[..] {
            continue;
        }
        let window = &chars[start..start + needle.len()];
        let left = window.iter().map(|c| c.left).fold(f32::MAX, f32::min);
        let right = window.iter().map(|c| c.right).fold(f32::MIN, f32::max);
        let bottom = window.iter().map(|c| c.bottom).fold(f32::MAX, f32::min);
        let top = window.iter().map(|c| c.top).fold(f32::MIN, f32::max);
        matches.push((left, bottom, right - left, top - bottom));
    }
    matches
}

/// Render pages in the background.
fn render_worker(
    path: PathBuf,
    requests: mpsc::Receiver<RenderRequest>,
    results: mpsc::Sender<RenderResult>,
) {
    let engine = PdfiumEngine::load();
    let mut document = None;

    while let Ok(request) = requests.recv() {
        let engine = match &engine {
            Ok(engine) => engine,
            Err(error) => {
                let _ = results.send(RenderResult {
                    page: request.page,
                    scale: request.scale,
                    width: 0,
                    height: 0,
                    rgba: None,
                    text: Vec::new(),
                    error: Some(error.to_string()),
                });
                continue;
            }
        };

        if document.is_none() {
            match engine.open(&path) {
                Ok(opened) => document = Some(opened),
                Err(error) => {
                    let _ = results.send(RenderResult {
                        page: request.page,
                        scale: request.scale,
                        width: 0,
                        height: 0,
                        rgba: None,
                        text: Vec::new(),
                        error: Some(error.to_string()),
                    });
                    continue;
                }
            }
        }

        let rendered = document
            .as_ref()
            .expect("the document was just opened")
            .render_page(request.page, request.scale);

        let result = match rendered {
            Ok(page) => RenderResult {
                page: request.page,
                scale: request.scale,
                width: page.width,
                height: page.height,
                rgba: Some(page.rgba),
                text: document
                    .as_ref()
                    .expect("the document was just opened")
                    .page_text(request.page)
                    .map(|text| text.chars)
                    .unwrap_or_default(),
                error: None,
            },
            Err(error) => RenderResult {
                page: request.page,
                scale: request.scale,
                width: 0,
                height: 0,
                rgba: None,
                text: Vec::new(),
                error: Some(error.to_string()),
            },
        };
        let _ = results.send(result);
    }
}
