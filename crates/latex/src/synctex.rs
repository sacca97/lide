//! SyncTeX: the map between a source line and a place on the PDF page.
//!
//! The TeX distribution ships a `synctex` command line tool that reads the
//! `.synctex.gz` a build writes. This module drives it and parses what it
//! prints; both directions are used, source to PDF and PDF back to source.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Where a source line is on the page.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncPoint {
    /// The page, counted from one.
    pub page: u32,
    /// The horizontal position in PDF points.
    pub x: f32,
    /// The vertical position in PDF points, from the top of the page.
    pub y: f32,
}

/// Where a place on the page came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncSource {
    /// The source file.
    pub path: PathBuf,
    /// The line, counted from one.
    pub line: u64,
    /// The column, counted from zero.
    pub column: u64,
}

/// The SyncTeX tool.
#[derive(Debug, Clone)]
pub struct SyncTex {
    /// The program to run.
    pub program: String,
    /// Where to run it: the directory the build ran in.
    pub directory: PathBuf,
}

impl SyncTex {
    /// The tool for a build directory.
    pub fn new(directory: impl AsRef<Path>) -> Self {
        Self {
            program: "synctex".to_string(),
            directory: directory.as_ref().to_path_buf(),
        }
    }

    /// Where `line` of `tex` lands on `pdf`.
    ///
    /// `None` when SyncTeX knows nothing about that line — which happens when
    /// the build did not write a `.synctex.gz`, or when the line is in a part
    /// of the source that produced no output.
    pub fn forward(
        &self,
        tex: &Path,
        line: u64,
        column: u64,
        pdf: &Path,
    ) -> Result<Option<SyncPoint>, String> {
        let input = format!("{}:{}:{}", line, column, tex.to_string_lossy());
        let output = self.run(&["view", "-i", &input, "-o", &pdf.to_string_lossy()])?;
        Ok(parse_view(&output))
    }

    /// Where a place on `page` of `pdf` came from.
    pub fn inverse(
        &self,
        pdf: &Path,
        page: u32,
        x: f32,
        y: f32,
    ) -> Result<Option<SyncSource>, String> {
        let output = format!("{}:{x}:{y}:{}", page, pdf.to_string_lossy());
        let text = self.run(&["edit", "-o", &output])?;
        Ok(parse_edit(&text, &self.directory))
    }

    fn run(&self, args: &[&str]) -> Result<String, String> {
        let output = Command::new(&self.program)
            .args(args)
            .current_dir(&self.directory)
            .output()
            .map_err(|error| format!("could not run {}: {error}", self.program))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

/// Parse `synctex view` output.
///
/// The tool prints a `SyncTeX result begin` block of `Key:value` lines. Only
/// the first result is used: a source line usually has one place, and the
/// first is the best.
pub fn parse_view(text: &str) -> Option<SyncPoint> {
    let mut page = None;
    let mut x = None;
    let mut y = None;

    for line in text.lines() {
        let line = line.trim();
        if line == "SyncTeX result end" {
            break;
        }
        if let Some(value) = line.strip_prefix("Page:") {
            page = value.trim().parse::<u32>().ok();
        } else if let Some(value) = line.strip_prefix("x:") {
            // `x` and `y` are the *first* result's; later ones are ignored.
            if x.is_none() {
                x = value.trim().parse::<f32>().ok();
            }
        } else if let Some(value) = line.strip_prefix("y:") {
            if y.is_none() {
                y = value.trim().parse::<f32>().ok();
            }
        }
        if page.is_some() && x.is_some() && y.is_some() {
            break;
        }
    }

    Some(SyncPoint {
        page: page?,
        x: x?,
        y: y?,
    })
}

/// Parse `synctex edit` output.
pub fn parse_edit(text: &str, directory: &Path) -> Option<SyncSource> {
    let mut path = None;
    let mut line = None;
    let mut column = Some(0u64);

    for raw in text.lines() {
        let raw = raw.trim();
        if raw == "SyncTeX result end" {
            break;
        }
        if let Some(value) = raw.strip_prefix("Input:") {
            path = Some(value.trim().to_string());
        } else if let Some(value) = raw.strip_prefix("Line:") {
            line = value.trim().parse::<u64>().ok();
        } else if let Some(value) = raw.strip_prefix("Column:") {
            column = value.trim().parse::<u64>().ok().or(Some(0));
        }
        if path.is_some() && line.is_some() {
            break;
        }
    }

    let path = PathBuf::from(path?);
    let path = if path.is_absolute() {
        path
    } else {
        directory.join(path)
    };
    Some(SyncSource {
        path,
        line: line?,
        column: column.unwrap_or(0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW: &str = "\
This is SyncTeX command line utility, version 1.5
SyncTeX result begin
Output:main.pdf
Page:3
x:133.768997
y:134.764618
h:133.768997
v:134.764618
W:266.000000
H:8.000000
SyncTeX result end
";

    const EDIT: &str = "\
This is SyncTeX command line utility, version 1.5
SyncTeX result begin
Input:/work/paper/main.tex
Line:42
Column:0
Offset:0
Context:
SyncTeX result end
";

    #[test]
    fn a_forward_result_gives_a_page_and_a_point() {
        let point = parse_view(VIEW).expect("a point");
        assert_eq!(point.page, 3);
        assert!((point.x - 133.769).abs() < 0.01);
        assert!((point.y - 134.765).abs() < 0.01);
    }

    #[test]
    fn an_inverse_result_gives_a_file_and_a_line() {
        let source = parse_edit(EDIT, Path::new("/work/paper")).expect("a source");
        assert_eq!(source.path, Path::new("/work/paper/main.tex"));
        assert_eq!(source.line, 42);
        assert_eq!(source.column, 0);
    }

    #[test]
    fn a_relative_input_path_is_joined_to_the_build_directory() {
        let text = "SyncTeX result begin\nInput:main.tex\nLine:7\nSyncTeX result end\n";
        let source = parse_edit(text, Path::new("/work")).unwrap();
        assert_eq!(source.path, Path::new("/work/main.tex"));
    }

    #[test]
    fn an_empty_result_is_none() {
        let text = "SyncTeX result begin\nSyncTeX result end\n";
        assert!(parse_view(text).is_none());
        assert!(parse_edit(text, Path::new("/work")).is_none());
    }

    #[test]
    fn the_tool_runs_in_the_build_directory() {
        let synctex = SyncTex::new("/work/paper");
        assert_eq!(synctex.program, "synctex");
        assert_eq!(synctex.directory, Path::new("/work/paper"));
    }
}
