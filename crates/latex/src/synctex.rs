//! SyncTeX: the map between a source line and a place on the PDF page.
//!
//! The work is done by the reference parser, `synctex_parser.c` from TeX Live,
//! vendored in `third_party/synctex` and built by `build.rs`. It is the code
//! Okular, Zathura, TeXworks and most other viewers use, so its idea of the
//! nearest node, of how TeX's units become PDF points and of how a box is
//! closed on another line than its text is the one everybody else has.

use std::ffi::{c_char, c_float, c_int, c_long, CStr, CString};
use std::path::{Path, PathBuf};

#[repr(C)]
struct Scanner {
    _private: [u8; 0],
}

#[repr(C)]
struct Node {
    _private: [u8; 0],
}

extern "C" {
    fn synctex_scanner_new_with_output_file(
        output: *const c_char,
        build_directory: *const c_char,
        parse: c_int,
    ) -> *mut Scanner;
    fn synctex_scanner_free(scanner: *mut Scanner) -> c_int;
    fn synctex_display_query(
        scanner: *mut Scanner,
        name: *const c_char,
        line: c_int,
        column: c_int,
        page_hint: c_int,
    ) -> c_long;
    fn synctex_edit_query(scanner: *mut Scanner, page: c_int, h: c_float, v: c_float) -> c_long;
    fn synctex_scanner_next_result(scanner: *mut Scanner) -> *mut Node;
    fn synctex_scanner_get_name(scanner: *mut Scanner, tag: c_int) -> *const c_char;
    fn synctex_node_tag(node: *mut Node) -> c_int;
    fn synctex_node_line(node: *mut Node) -> c_int;
    fn synctex_node_column(node: *mut Node) -> c_int;
    fn synctex_node_page(node: *mut Node) -> c_int;
    fn synctex_node_visible_h(node: *mut Node) -> c_float;
    fn synctex_node_visible_v(node: *mut Node) -> c_float;
}

/// A parsed SyncTeX file, freed when dropped.
struct Handle(*mut Scanner);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the pointer came from the scanner constructor and is freed once.
        unsafe { synctex_scanner_free(self.0) };
    }
}

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

/// The SyncTeX data for a build.
#[derive(Debug, Clone)]
pub struct SyncTex {
    /// Where the build ran.
    pub directory: PathBuf,
}

fn c_path(path: &Path) -> Result<CString, String> {
    CString::new(path.to_string_lossy().as_bytes())
        .map_err(|_| format!("{} is not a usable path", path.display()))
}

impl SyncTex {
    /// The map for a build directory.
    pub fn new(directory: impl AsRef<Path>) -> Self {
        Self {
            directory: directory.as_ref().to_path_buf(),
        }
    }

    fn open(&self, pdf: &Path) -> Result<Handle, String> {
        let output = c_path(pdf)?;
        let directory = c_path(&self.directory)?;
        // SAFETY: both strings outlive the call; a null result is an error.
        let scanner =
            unsafe { synctex_scanner_new_with_output_file(output.as_ptr(), directory.as_ptr(), 1) };
        if scanner.is_null() {
            return Err(format!(
                "no readable .synctex.gz or .synctex for {}; rebuild the document",
                pdf.display()
            ));
        }
        Ok(Handle(scanner))
    }

    /// Where `line` of `tex` lands on `pdf`.
    ///
    /// `None` when the file has no record for that line or a nearby line.
    pub fn forward(
        &self,
        tex: &Path,
        line: u64,
        column: u64,
        pdf: &Path,
    ) -> Result<Option<SyncPoint>, String> {
        // The `Input:` line records the path as the build spelled it, which can
        // differ from the workspace's own spelling: pdflatex writes a relative
        // `./main.tex`, and macOS's temporary directory is a symlink, so the
        // canonical path starts with /private. The parser matches names
        // textually, so try the spellings in turn: as given, canonicalised,
        // then the bare file name.
        let mut names = vec![tex.to_path_buf()];
        if let Ok(canonical) = std::fs::canonicalize(tex) {
            if !names.contains(&canonical) {
                names.push(canonical);
            }
        }
        if let Some(file) = tex.file_name().map(PathBuf::from) {
            if !names.contains(&file) {
                names.push(file);
            }
        }
        for name in &names {
            if let Some(point) = self.forward_named(name, line, column, pdf)? {
                return Ok(Some(point));
            }
        }
        Ok(None)
    }

    /// Where each of `lines` of `tex` lands on `pdf`, in one pass over the
    /// sidecar. The path spellings are tried as in [`Self::forward`]; the
    /// spelling that answers the most lines wins.
    pub fn forward_many(
        &self,
        tex: &Path,
        lines: &[u64],
        pdf: &Path,
    ) -> Result<Vec<(u64, Option<SyncPoint>)>, String> {
        let mut names = vec![tex.to_path_buf()];
        if let Ok(canonical) = std::fs::canonicalize(tex) {
            if !names.contains(&canonical) {
                names.push(canonical);
            }
        }
        if let Some(file) = tex.file_name().map(PathBuf::from) {
            if !names.contains(&file) {
                names.push(file);
            }
        }
        let handle = self.open(pdf)?;
        // The spelling that answers the most lines wins: results and its count.
        type Answers = Vec<(u64, Option<SyncPoint>)>;
        let mut best: Option<(Answers, usize)> = None;
        for name in &names {
            let c_name = c_path(name)?;
            let mut results = Vec::with_capacity(lines.len());
            let mut found = 0usize;
            // SAFETY: the scanner and the name are valid for these calls, and
            // each node is read before the next query.
            for &line in lines {
                let point = unsafe {
                    let count = synctex_display_query(
                        handle.0,
                        c_name.as_ptr(),
                        line.min(i32::MAX as u64) as c_int,
                        0,
                        -1,
                    );
                    if count <= 0 {
                        None
                    } else {
                        let node = synctex_scanner_next_result(handle.0);
                        if node.is_null() {
                            None
                        } else {
                            found += 1;
                            Some(SyncPoint {
                                page: synctex_node_page(node).max(1) as u32,
                                x: synctex_node_visible_h(node),
                                y: synctex_node_visible_v(node),
                            })
                        }
                    }
                };
                results.push((line, point));
            }
            if best.as_ref().is_none_or(|(_, best_found)| found > *best_found) {
                best = Some((results, found));
            }
            if found == lines.len() {
                break;
            }
        }
        Ok(best.map(|(results, _)| results).unwrap_or_default())
    }

    /// [`Self::forward`] for one spelling of the source path.
    fn forward_named(
        &self,
        tex: &Path,
        line: u64,
        column: u64,
        pdf: &Path,
    ) -> Result<Option<SyncPoint>, String> {
        let handle = self.open(pdf)?;
        let name = c_path(tex)?;
        // SAFETY: the scanner and the name are valid for these calls, and the
        // nodes are only read before the next query.
        unsafe {
            let count = synctex_display_query(
                handle.0,
                name.as_ptr(),
                line.min(i32::MAX as u64) as c_int,
                column.min(i32::MAX as u64) as c_int,
                -1,
            );
            if count <= 0 {
                return Ok(None);
            }
            let node = synctex_scanner_next_result(handle.0);
            if node.is_null() {
                return Ok(None);
            }
            Ok(Some(SyncPoint {
                page: synctex_node_page(node).max(1) as u32,
                x: synctex_node_visible_h(node),
                y: synctex_node_visible_v(node),
            }))
        }
    }

    /// Where a place on `page` of `pdf` came from.
    ///
    /// The point is in PDF points, measured from the top of the page.
    pub fn inverse(
        &self,
        pdf: &Path,
        page: u32,
        x: f32,
        y: f32,
    ) -> Result<Option<SyncSource>, String> {
        let handle = self.open(pdf)?;
        // SAFETY: as above; the name is copied before the scanner is dropped.
        unsafe {
            let count = synctex_edit_query(handle.0, page as c_int, x, y);
            if count <= 0 {
                return Ok(None);
            }
            let node = synctex_scanner_next_result(handle.0);
            if node.is_null() {
                return Ok(None);
            }
            let name = synctex_scanner_get_name(handle.0, synctex_node_tag(node));
            if name.is_null() {
                return Ok(None);
            }
            let name = PathBuf::from(CStr::from_ptr(name).to_string_lossy().into_owned());
            let absolute = name.is_absolute();
            let path = if absolute {
                name.clone()
            } else {
                self.directory.join(&name)
            };
            // TeX writes `dir/./file`; collecting the components drops the dot.
            let path: PathBuf = path.components().collect();
            // A relative `Input:` is resolved against the directory the
            // scanner was opened with. When the sidecar sits in a build
            // subdirectory (`-output-directory build`), the source lives one
            // level up, so fall back to that spelling when the file is not
            // where the scanner put it.
            let path = if path.exists() || absolute {
                path
            } else {
                self.directory
                    .parent()
                    .map(|parent| parent.join(&name))
                    .map(|candidate| candidate.components().collect::<PathBuf>())
                    .filter(|candidate| candidate.exists())
                    .unwrap_or(path)
            };
            Ok(Some(SyncSource {
                path,
                line: synctex_node_line(node).max(1) as u64,
                column: synctex_node_column(node).max(0) as u64,
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> PathBuf {
        let directory = std::env::temp_dir().join(format!("lide-synctex-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&directory);
        directory
    }

    const SIMPLE: &str = "\
SyncTeX Version:1
Input:1:/work/paper/main.tex
Output:pdf
Magnification:1000
Unit:1
X Offset:0
Y Offset:0
Content:
{1
[1,1:0,0:39158276,55380990,0
(1,2:3552215,18442772:15795512,444328,141880
x1,2:4571361,18442772
x1,1:5429155,18442772
x1,1:6453622,18442772
)
(1,1:3552215,33984329:15795512,541848,9432
x1,1:3945431,33984329
x1,1:6086078,33984329
)
]
}1
Postamble:
Count:2
Post scriptum:
";

    fn build(name: &str) -> (SyncTex, PathBuf) {
        let directory = scratch();
        let pdf = directory.join(format!("{name}.pdf"));
        std::fs::write(&pdf, b"%PDF-1.4\n").unwrap();
        std::fs::write(directory.join(format!("{name}.synctex")), SIMPLE).unwrap();
        (SyncTex::new(&directory), pdf)
    }

    #[test]
    fn a_missing_file_is_an_error_not_a_panic() {
        let directory = scratch();
        let result = SyncTex::new(&directory).inverse(&directory.join("none.pdf"), 1, 10.0, 10.0);
        assert!(result.is_err());
    }

    #[test]
    fn forward_finds_a_line_on_the_page() {
        let (synctex, pdf) = build("forward");
        let point = synctex
            .forward(Path::new("/work/paper/main.tex"), 1, 0, &pdf)
            .expect("the file parses")
            .expect("line 1 is mapped");
        assert_eq!(point.page, 1);
        assert!(point.y > 0.0 && point.x > 0.0);
    }

    #[test]
    fn inverse_goes_back_to_the_source() {
        let (synctex, pdf) = build("inverse");
        let source = synctex
            .inverse(&pdf, 1, 90.0, 285.0)
            .expect("the file parses")
            .expect("the point is on text");
        assert_eq!(source.path, Path::new("/work/paper/main.tex"));
        assert!(source.line == 1 || source.line == 2);
    }
}
