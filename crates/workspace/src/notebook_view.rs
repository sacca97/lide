//! The notebook viewer.
//!
//! A `.ipynb` opens as a column of cells: Markdown cells rendered, code cells
//! with their source and whatever they last printed. It is read-only — running
//! a cell needs a kernel, which the editor does not start.

use std::path::{Path, PathBuf};

use crate::documents::notebook::{CellKind, Notebook};
use rune::{col, row, text, El, ScrollSpec};
use rune_kit::markdown::{Markdown, MarkdownStyle};
use rune_kit::Kit;

/// A notebook document.
pub struct NotebookView {
    /// Where it came from.
    pub path: PathBuf,
    /// The parsed cells.
    pub notebook: Notebook,
    /// Why it could not be read, if it could not.
    pub error: Option<String>,
}

impl std::fmt::Debug for NotebookView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotebookView")
            .field("path", &self.path)
            .field("cells", &self.notebook.cells.len())
            .finish_non_exhaustive()
    }
}

impl NotebookView {
    /// Read and parse a notebook.
    pub fn open(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_path_buf();
        match std::fs::read_to_string(&path) {
            Ok(text) => match Notebook::parse(&text) {
                Ok(notebook) => Self {
                    notebook,
                    path,
                    error: None,
                },
                Err(error) => Self {
                    notebook: Notebook::default(),
                    path,
                    error: Some(error),
                },
            },
            Err(error) => Self {
                notebook: Notebook::default(),
                path,
                error: Some(error.to_string()),
            },
        }
    }

    /// The title shown on the tab.
    pub fn title(&self) -> String {
        self.path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "notebook".to_string())
    }

    /// The path the notebook was opened from.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Draw the notebook.
    pub fn view(&self, k: &Kit) -> El {
        if let Some(error) = &self.error {
            return col()
                .w_full()
                .h_full()
                .padding(16.0)
                .fill(k.c.page)
                .child(text(error.clone()).font(k.ty.ui).color(k.c.text_base));
        }

        let mut body = col().w_full().gap(10.0);

        for (index, cell) in self.notebook.cells.iter().enumerate() {
            let content: El = match cell.kind {
                CellKind::Markdown => {
                    Markdown::new(cell.source.as_str()).render(&MarkdownStyle::from_kit(k))
                }
                CellKind::Code | CellKind::Other => {
                    let mut block = col()
                        .w_full()
                        .gap(4.0)
                        .padding(12.0)
                        .radius(6.0)
                        .fill(k.c.surface_base)
                        .child(
                            text(cell.source.clone())
                                .font(k.ty.mono)
                                .color(k.c.text_base),
                        );

                    for output in &cell.outputs {
                        block = block.child(text(output.text.clone()).font(k.ty.mono).color(
                            if output.is_error {
                                k.c.danger.fg
                            } else {
                                k.c.text_muted
                            },
                        ));
                    }
                    block
                }
            };

            body = body.child(
                col()
                    .key(format!("cell-{index}"))
                    .w_full()
                    .gap(4.0)
                    .child(
                        text(match cell.kind {
                            CellKind::Markdown => "markdown".to_string(),
                            CellKind::Code => format!("code [{}]", index),
                            CellKind::Other => "raw".to_string(),
                        })
                        .font(k.ty.small)
                        .color(k.c.text_weak),
                    )
                    .child(content),
            );
        }

        let code_cells = self.notebook.code_cells();

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
                        text(format!(
                            "{} cells, {code_cells} code",
                            self.notebook.cells.len()
                        ))
                        .font(k.ty.small)
                        .color(k.c.text_weak)
                        .grow(1.0),
                    )
                    .child(text("read-only").font(k.ty.small).color(k.c.text_weak)),
            )
            .child(
                col()
                    .key("notebook-scroll")
                    .w_full()
                    .grow(1.0)
                    .min_h(0.0)
                    .scroll(ScrollSpec {
                        stick_bottom: false,
                    })
                    .padding(16.0)
                    .child(body),
            )
    }
}
