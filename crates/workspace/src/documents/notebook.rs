//! Reading Jupyter notebooks.
//!
//! A notebook is JSON: a list of cells, each Markdown or code, and for a code
//! cell the outputs its last run produced. The editor shows them; running a
//! cell needs a kernel, which is not implemented.

use serde::Deserialize;

/// A cell of a notebook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// Whether the cell is prose or code.
    pub kind: CellKind,
    /// The cell's source, with the lines joined.
    pub source: String,
    /// What running it produced.
    pub outputs: Vec<Output>,
}

/// What a cell is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellKind {
    /// A Markdown cell.
    Markdown,
    /// A code cell.
    Code,
    /// Anything else, such as a raw cell.
    Other,
}

/// What a code cell produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// Whether this is an error rather than ordinary output.
    pub is_error: bool,
    /// The text.
    pub text: String,
}

/// A parsed notebook.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Notebook {
    /// The cells, in order.
    pub cells: Vec<Cell>,
}

impl Notebook {
    /// Parse a notebook's JSON.
    pub fn parse(json: &str) -> Result<Self, String> {
        let file: RawNotebook =
            serde_json::from_str(json).map_err(|error| format!("not a notebook: {error}"))?;

        let cells = file
            .cells
            .into_iter()
            .map(|cell| {
                let kind = match cell.cell_type.as_str() {
                    "markdown" => CellKind::Markdown,
                    "code" => CellKind::Code,
                    _ => CellKind::Other,
                };
                let outputs = cell
                    .outputs
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|output| {
                        // `stream` carries `text`; `execute_result` and `display_data`
                        // carry `data`; `error` carries `ename` and `evalue`.
                        if let Some(name) = output.ename {
                    let value = output.evalue.unwrap_or_default(
                    );
                    return Some(Output {
                        is_error: true,
                        text: format!("{name}: {value}"),
                    });
                }
                        let text = output
                            .text
                            .map(|text| join(&text))
                            .or_else(|| {
                                output
                                    .data
                                    .and_then(|data| data.text_plain)
                                    .map(|text| join(&text))
                            })?;
                        if text.is_empty() {
                            return None;
                        }
                        Some(Output {
                            is_error: false,
                            text,
                        })
                    })
                    .collect();

                Cell {
                    kind,
                    source: join(&cell.source),
                    outputs,
                }
            })
            .collect();

        Ok(Self { cells })
    }

    /// How many cells are code.
    pub fn code_cells(&self) -> usize {
        self.cells
            .iter()
            .filter(|cell| cell.kind == CellKind::Code)
            .count()
    }
}

/// A notebook string field may be a list of lines or a single string.
fn join(value: &SourceOrText) -> String {
    match value {
        SourceOrText::Lines(lines) => lines.concat(),
        SourceOrText::One(text) => text.clone(),
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SourceOrText {
    Lines(Vec<String>),
    One(String),
}

impl Default for SourceOrText {
    fn default() -> Self {
        SourceOrText::One(String::new())
    }
}

#[derive(Debug, Deserialize)]
struct RawNotebook {
    #[serde(default)]
    cells: Vec<RawCell>,
}

#[derive(Debug, Deserialize)]
struct RawCell {
    #[serde(default)]
    cell_type: String,
    #[serde(default)]
    source: SourceOrText,
    #[serde(default)]
    outputs: Option<Vec<RawOutput>>,
}

#[derive(Debug, Deserialize)]
struct RawOutput {
    #[serde(default)]
    text: Option<SourceOrText>,
    #[serde(default)]
    data: Option<RawData>,
    #[serde(default)]
    ename: Option<String>,
    #[serde(default)]
    evalue: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawData {
    #[serde(default, rename = "text/plain")]
    text_plain: Option<SourceOrText>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTEBOOK: &str = r##"{
      "cells": [
        { "cell_type": "markdown", "source": ["# Title\n", "Some prose.\n"] },
        { "cell_type": "code", "source": "print(1)\n",
          "outputs": [{ "output_type": "stream", "text": ["1\n"] }] },
        { "cell_type": "code", "source": ["1/0\n"],
          "outputs": [{ "output_type": "error", "ename": "ZeroDivisionError", "evalue": "division by zero" }] },
        { "cell_type": "raw", "source": "raw" }
      ],
      "metadata": {},
      "nbformat": 4
    }"##;

    #[test]
    fn cells_are_parsed_with_their_kind_and_source() {
        let notebook = Notebook::parse(NOTEBOOK).unwrap();
        assert_eq!(notebook.cells.len(), 4);
        assert_eq!(notebook.cells[0].kind, CellKind::Markdown);
        assert_eq!(notebook.cells[0].source, "# Title\nSome prose.\n");
        assert_eq!(notebook.cells[1].kind, CellKind::Code);
        assert_eq!(notebook.cells[1].source, "print(1)\n");
        assert_eq!(notebook.cells[3].kind, CellKind::Other);
        assert_eq!(notebook.code_cells(), 2);
    }

    #[test]
    fn outputs_are_kept() {
        let notebook = Notebook::parse(NOTEBOOK).unwrap();
        assert_eq!(notebook.cells[1].outputs.len(), 1);
        assert_eq!(notebook.cells[1].outputs[0].text, "1\n");
        assert!(!notebook.cells[1].outputs[0].is_error);
    }

    #[test]
    fn an_error_output_is_marked() {
        let notebook = Notebook::parse(NOTEBOOK).unwrap();
        let output = &notebook.cells[2].outputs[0];
        assert!(output.is_error);
        assert!(output.text.contains("ZeroDivisionError"));
        assert!(output.text.contains("division by zero"));
    }

    #[test]
    fn a_string_source_and_a_line_list_are_the_same() {
        let one = Notebook::parse(r#"{"cells":[{"cell_type":"markdown","source":"hi"}]}"#).unwrap();
        let lines =
            Notebook::parse(r#"{"cells":[{"cell_type":"markdown","source":["hi"]}]}"#).unwrap();
        assert_eq!(one.cells[0].source, lines.cells[0].source);
    }

    #[test]
    fn something_that_is_not_a_notebook_is_an_error() {
        assert!(Notebook::parse("not json").is_err());
    }

    #[test]
    fn a_notebook_with_no_cells_is_empty() {
        let notebook = Notebook::parse(r#"{"cells":[]}"#).unwrap();
        assert!(notebook.cells.is_empty());
    }
}
