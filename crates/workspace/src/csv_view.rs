//! The CSV viewer.
//!
//! A `.csv` or `.tsv` file opens as a table rather than as text: the first row
//! is the header, the columns are sized to their content, and a row that is
//! shorter than the rest is padded rather than breaking the layout.
//!
//! Clicking a header sorts by that column; typing in the header filters the
//! body. Both are view state: the file is not rewritten.
//!
//! The parse is [`lide_documents::csv`]'s, so it can be tested without a
//! window.

use std::path::{Path, PathBuf};

use lide_documents::csv::{Delimiter, Table};
use rune::{col, el, row, text, text_input, El, ScrollSpec};
use rune_edit::TextEdit;
use rune_kit::Kit;

/// A table document.
pub struct CsvView {
    /// Where it came from.
    pub path: PathBuf,
    /// The parsed table.
    pub table: Table,
    /// Why it could not be read, if it could not.
    pub error: Option<String>,
    /// The column the body is sorted by, and whether it is ascending.
    pub sort: Option<(usize, bool)>,
    /// The text the body is filtered by.
    pub filter: TextEdit,
}

impl std::fmt::Debug for CsvView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CsvView")
            .field("path", &self.path)
            .field("rows", &self.table.rows.len())
            .field("columns", &self.table.columns())
            .finish_non_exhaustive()
    }
}

impl CsvView {
    /// Read and parse a table.
    pub fn open(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_path_buf();
        let delimiter = Delimiter::for_path(&path.to_string_lossy());

        match std::fs::read_to_string(&path) {
            Ok(text) => Self {
                table: Table::parse(&text, delimiter),
                path,
                error: None,
                sort: None,
                filter: TextEdit::new(""),
            },
            Err(error) => Self {
                table: Table::default(),
                path,
                error: Some(error.to_string()),
                sort: None,
                filter: TextEdit::new(""),
            },
        }
    }

    /// The title shown on the tab.
    pub fn title(&self) -> String {
        self.path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "table".to_string())
    }

    /// The path the table was opened from.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The number of rows and columns.
    pub fn size(&self) -> (usize, usize) {
        (self.table.rows.len(), self.table.columns())
    }

    /// Draw the table.
    pub fn view(&self, k: &Kit, pane: usize) -> El {
        if let Some(error) = &self.error {
            return col()
                .w_full()
                .h_full()
                .padding(16.0)
                .fill(k.c.page)
                .child(text(error.clone()).font(k.ty.ui).color(k.c.text_base));
        }

        let (rows, columns) = self.size();
        // The table stops at a sane number of rows: a spreadsheet with a
        // hundred thousand of them is a spreadsheet, not an editor's job.
        const MAX_ROWS: usize = 500;
        const MAX_COLUMNS: usize = 40;

        let visible = self.table.visible_rows(self.sort, &self.filter.text);
        let shown = visible.len().min(MAX_ROWS);

        // One width per column, from its longest visible value, so the header
        // and every row line up whatever they hold.
        let widths: Vec<f32> = (0..columns.min(MAX_COLUMNS))
            .map(|column| {
                let longest = std::iter::once(0usize)
                    .chain(visible.iter().take(MAX_ROWS).copied())
                    .map(|row| self.table.field(row, column).chars().count())
                    .max()
                    .unwrap_or(0);
                (longest as f32 * 6.8 + 28.0).clamp(96.0, 360.0)
            })
            .collect();
        // Lines go right and below each cell, so two cells never double one.
        let line = [0.0, 1.0, 1.0, 0.0];

        let mut header = row().w_full().items_start().gap(0.0);
        for (column, width) in widths.iter().enumerate() {
            let value = self.table.field(0, column);
            let active = self.sort.filter(|(sorted, _)| *sorted == column);
            let label = match active {
                Some((_, true)) => format!("{value} ▲"),
                Some((_, false)) => format!("{value} ▼"),
                None => value.to_string(),
            };
            header = header.child(
                el().key(format!("header-{column}"))
                    .w(*width)
                    .px(10.0)
                    .py(5.0)
                    .border_sides(line, k.c.border_base)
                    .fill(k.c.surface_base)
                    .hover(k.c.hover)
                    .on_press(move |s: &mut crate::Workspace| {
                        let csv = csv_at(s, pane);
                        csv.sort = match csv.sort {
                            Some((sorted, true)) if sorted == column => Some((column, false)),
                            _ => Some((column, true)),
                        };
                    })
                    .child(
                        text(label)
                            .font(k.ty.text_xsmall_medium)
                            .color(k.c.text_base)
                            .truncate(),
                    ),
            );
        }

        let mut body = col().w_full().gap(0.0).child(header);
        for (position, row_index) in visible.iter().take(MAX_ROWS).enumerate() {
            let mut cells = row().w_full().items_start().gap(0.0);
            for (column, width) in widths.iter().enumerate() {
                let value = self.table.field(*row_index, column);
                cells = cells.child(
                    el().key(format!("cell-{position}-{column}"))
                        .w(*width)
                        .px(10.0)
                        .py(5.0)
                        .border_sides(line, k.c.border_muted)
                        .fill(k.c.page)
                        .child(
                            text(value.to_string())
                                .font(k.ty.text_xsmall)
                                .color(k.c.text_muted)
                                .truncate(),
                        ),
                );
            }
            body = body.child(cells);
        }

        let mut note = String::new();
        if visible.len() > MAX_ROWS {
            note = format!("showing {MAX_ROWS} of {} rows", visible.len());
        } else if columns > MAX_COLUMNS {
            note = format!("showing {MAX_COLUMNS} of {columns} columns");
        } else if !self.filter.text.trim().is_empty() {
            note = format!("{shown} of {} rows", rows.saturating_sub(1));
        }

        col()
            .w_full()
            .h_full()
            .fill(k.c.page)
            .child(
                row()
                    .w_full()
                    .h(40.0)
                    .px(10.0)
                    .gap(10.0)
                    .center_y()
                    .fill(k.c.bg_base)
                    .child(
                        text(format!("{rows} rows, {columns} columns"))
                            .font(k.ty.small)
                            .color(k.c.text_weak),
                    )
                    .child(
                        text_input(&self.filter, k.ty.ui)
                            .key(format!("csv-filter-{pane}"))
                            .on_edit(move |s: &mut crate::Workspace| &mut csv_at(s, pane).filter)
                            .placeholder("Filter", k.c.text_weak)
                            .w(180.0)
                            .px(6.0)
                            .py(3.0)
                            .radius(5.0)
                            .fill(k.c.surface_base),
                    )
                    .child(text(note).font(k.ty.small).color(k.c.text_weak)),
            )
            .child(
                col()
                    .key("csv-scroll")
                    .w_full()
                    .grow(1.0)
                    .min_h(0.0)
                    .scroll(ScrollSpec {
                        stick_bottom: false,
                    })
                    .padding(8.0)
                    .child(body),
            )
    }
}

/// The table in `pane`, for the header and filter handlers.
fn csv_at(s: &mut crate::Workspace, pane: usize) -> &mut CsvView {
    let index = if pane == 0 {
        s.active
    } else {
        s.split_document.unwrap_or(s.active)
    };
    match s.documents.get_mut(index) {
        Some(crate::Document::Csv(csv)) => csv,
        _ => unreachable!("the pane shows a table"),
    }
}
