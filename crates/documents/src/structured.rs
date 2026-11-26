//! Structured configuration files: checking them, and formatting the ones
//! that can be formatted without losing anything.
//!
//! JSON has no comments and its object order is not semantic, so a formatter
//! can rewrite it freely. XML has comments and mixed content, so it is
//! formatted only when no element mixes text with child elements. TOML and
//! YAML carry comments and key order that a parse-and-print would silently
//! lose, so they are validated but not reformatted.

use std::fmt;
use std::path::Path;

/// A structured file format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// JSON.
    Json,
    /// TOML.
    Toml,
    /// YAML.
    Yaml,
    /// XML.
    Xml,
}

impl Kind {
    /// The name shown to the user.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Json => "JSON",
            Kind::Toml => "TOML",
            Kind::Yaml => "YAML",
            Kind::Xml => "XML",
        }
    }

    /// The format a path's extension names, if it is one of these.
    pub fn for_path(path: &Path) -> Option<Kind> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "json" => Some(Kind::Json),
            "toml" => Some(Kind::Toml),
            "yaml" | "yml" => Some(Kind::Yaml),
            "xml" => Some(Kind::Xml),
            _ => None,
        }
    }

    /// Whether [`format`] can rewrite this kind without losing comments or
    /// order.
    pub fn can_format(self) -> bool {
        matches!(self, Kind::Json | Kind::Xml)
    }
}

/// A syntax error, with the position a reader needs to find it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// The line, counted from one. Zero when the parser gave no position.
    pub line: usize,
    /// The column, counted from one. Zero when the parser gave no position.
    pub column: usize,
    /// What the parser said.
    pub message: String,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            f.write_str(&self.message)
        } else {
            write!(f, "line {}, column {}: {}", self.line, self.column, self.message)
        }
    }
}

/// Check `text` as `kind`.
pub fn validate(text: &str, kind: Kind) -> Result<(), Problem> {
    match kind {
        Kind::Json => serde_json::from_str::<serde_json::Value>(text)
            .map(|_| ())
            .map_err(|error| Problem {
                line: error.line(),
                column: error.column(),
                message: error.to_string(),
            }),
        Kind::Toml => toml::from_str::<toml::Value>(text)
            .map(|_| ())
            .map_err(|error| {
                let (line, column) = match error.span() {
                    Some(span) => line_column(text, span.start),
                    None => (0, 0),
                };
                Problem {
                    line,
                    column,
                    message: error.to_string(),
                }
            }),
        Kind::Yaml => yaml_rust2::YamlLoader::load_from_str(text)
            .map(|_| ())
            .map_err(|error| Problem {
                line: error.marker().line() + 1,
                column: error.marker().col() + 1,
                message: error.info().to_string(),
            }),
        Kind::Xml => {
            let mut reader = quick_xml::Reader::from_str(text);
            loop {
                match reader.read_event() {
                    Ok(quick_xml::events::Event::Eof) => return Ok(()),
                    Ok(_) => {}
                    Err(error) => {
                        let (line, column) =
                            line_column(text, reader.error_position() as usize);
                        return Err(Problem {
                            line,
                            column,
                            message: error.to_string(),
                        });
                    }
                }
            }
        }
    }
}

/// Rewrite `text` with conventional indentation.
///
/// JSON objects keep their key order. XML keeps comments, declarations and
/// text, but refuses a document in which an element mixes text with child
/// elements: the whitespace there is content, and inserting newlines would
/// change it.
pub fn format(text: &str, kind: Kind) -> Result<String, Problem> {
    match kind {
        Kind::Json => {
            let value: serde_json::Value =
                serde_json::from_str(text).map_err(|error| Problem {
                    line: error.line(),
                    column: error.column(),
                    message: error.to_string(),
                })?;
            let mut out = serde_json::to_string_pretty(&value).map_err(|error| Problem {
                line: 0,
                column: 0,
                message: error.to_string(),
            })?;
            out.push('\n');
            Ok(out)
        }
        Kind::Xml => format_xml(text),
        Kind::Toml | Kind::Yaml => Err(Problem {
            line: 0,
            column: 0,
            message: format!(
                "{} formatting would drop comments and order, so it is not offered",
                kind.name()
            ),
        }),
    }
}

/// The line and column of a byte offset, counted from one.
fn line_column(text: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(text.len());
    let before = &text[..offset];
    let line = before.matches('\n').count() + 1;
    let column = before.len() - before.rfind('\n').map(|index| index + 1).unwrap_or(0) + 1;
    (line, column)
}

/// One element while the formatter checks for mixed content.
struct Frame {
    /// Whether a child element has started inside it.
    has_child: bool,
    /// Whether it has text of its own.
    has_text: bool,
}

fn format_xml(text: &str) -> Result<String, Problem> {
    check_mixed_content(text)?;

    let mut reader = quick_xml::Reader::from_str(text);
    let mut writer = quick_xml::Writer::new_with_indent(Vec::new(), b' ', 2);
    let mut stack: Vec<Frame> = Vec::new();

    loop {
        let event = reader.read_event().map_err(|error| {
            let (line, column) = line_column(text, reader.error_position() as usize);
            Problem {
                line,
                column,
                message: error.to_string(),
            }
        })?;

        match &event {
            quick_xml::events::Event::Eof => break,
            quick_xml::events::Event::Start(_) => {
                if let Some(parent) = stack.last_mut() {
                    parent.has_child = true;
                }
                stack.push(Frame {
                    has_child: false,
                    has_text: false,
                });
            }
            quick_xml::events::Event::End(_) => {
                stack.pop();
            }
            quick_xml::events::Event::Empty(_) => {
                if let Some(parent) = stack.last_mut() {
                    parent.has_child = true;
                }
            }
            quick_xml::events::Event::Text(fragment) => {
                let content = String::from_utf8_lossy(fragment);
                if !content.trim().is_empty() {
                    if let Some(frame) = stack.last_mut() {
                        frame.has_text = true;
                    }
                } else if stack.last().is_some_and(|frame| frame.has_child) {
                    // Layout whitespace between child elements: the writer
                    // supplies its own.
                    continue;
                }
            }
            quick_xml::events::Event::CData(_)
            | quick_xml::events::Event::GeneralRef(_) => {
                if let Some(frame) = stack.last_mut() {
                    frame.has_text = true;
                }
            }
            _ => {}
        }

        writer.write_event(event).map_err(|error| Problem {
            line: 0,
            column: 0,
            message: error.to_string(),
        })?;
    }

    let bytes = writer.into_inner();
    let mut out = String::from_utf8(bytes).map_err(|error| Problem {
        line: 0,
        column: 0,
        message: error.to_string(),
    })?;
    out.push('\n');
    Ok(out)
}

/// Refuse mixed content before formatting, because inserting indentation into
/// `<p>Hello <b>world</b></p>` changes the text a reader sees.
fn check_mixed_content(text: &str) -> Result<(), Problem> {
    let mut reader = quick_xml::Reader::from_str(text);
    let mut stack: Vec<Frame> = Vec::new();

    loop {
        let event = reader.read_event().map_err(|error| {
            let (line, column) = line_column(text, reader.error_position() as usize);
            Problem {
                line,
                column,
                message: error.to_string(),
            }
        })?;
        match &event {
            quick_xml::events::Event::Eof => break,
            quick_xml::events::Event::Start(_) => {
                if let Some(parent) = stack.last_mut() {
                    parent.has_child = true;
                }
                stack.push(Frame {
                    has_child: false,
                    has_text: false,
                });
            }
            quick_xml::events::Event::End(_) => {
                if let Some(frame) = stack.pop() {
                    if frame.has_child && frame.has_text {
                        return Err(Problem {
                            line: 0,
                            column: 0,
                            message:
                                "an element mixes text with child elements; formatting would change it"
                                    .to_string(),
                        });
                    }
                }
            }
            quick_xml::events::Event::Empty(_) => {
                if let Some(parent) = stack.last_mut() {
                    parent.has_child = true;
                }
            }
            quick_xml::events::Event::Text(fragment) => {
                if !String::from_utf8_lossy(fragment).trim().is_empty() {
                    if let Some(frame) = stack.last_mut() {
                        frame.has_text = true;
                    }
                }
            }
            quick_xml::events::Event::CData(_)
            | quick_xml::events::Event::GeneralRef(_) => {
                if let Some(frame) = stack.last_mut() {
                    frame.has_text = true;
                }
            }
            _ => {}
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_extension_chooses_the_format() {
        assert_eq!(Kind::for_path(Path::new("a.json")), Some(Kind::Json));
        assert_eq!(Kind::for_path(Path::new("a.toml")), Some(Kind::Toml));
        assert_eq!(Kind::for_path(Path::new("a.yml")), Some(Kind::Yaml));
        assert_eq!(Kind::for_path(Path::new("a.xml")), Some(Kind::Xml));
        assert_eq!(Kind::for_path(Path::new("a.txt")), None);
        assert!(Kind::Json.can_format());
        assert!(!Kind::Yaml.can_format(), "a formatter would drop comments");
    }

    #[test]
    fn valid_json_passes() {
        assert!(validate(r#"{"a": [1, 2]}"#, Kind::Json).is_ok());
    }

    #[test]
    fn a_json_error_has_its_line_and_column() {
        let problem = validate("{\n  \"a\": 1,\n  \"b\": ,\n}", Kind::Json).unwrap_err();
        assert_eq!(problem.line, 3);
        assert!(problem.column > 1);
        assert!(problem.to_string().starts_with("line 3, column "));
    }

    #[test]
    fn valid_toml_passes_and_an_error_points_at_the_span() {
        assert!(validate("[a]\nb = 1\n", Kind::Toml).is_ok());
        let problem = validate("[a]\nb = \n", Kind::Toml).unwrap_err();
        assert_eq!(problem.line, 2);
        assert!(problem.column > 0);
    }

    #[test]
    fn valid_yaml_passes_and_an_error_points_at_the_line() {
        assert!(validate("a: 1\nb: [two, 3]\n", Kind::Yaml).is_ok());
        let problem = validate("a: [1, 2\nb: 3\n", Kind::Yaml).unwrap_err();
        assert!(problem.line >= 1);
        assert!(!problem.message.is_empty());
    }

    #[test]
    fn valid_xml_passes_and_an_error_points_at_the_line() {
        assert!(validate("<a><b>text</b></a>", Kind::Xml).is_ok());
        let problem = validate("<a>\n  <b>\n</a>\n", Kind::Xml).unwrap_err();
        assert!(problem.line >= 2);
    }

    #[test]
    fn json_is_pretty_printed_with_its_key_order() {
        let formatted = format(r#"{"b":1,"a":[1,2]}"#, Kind::Json).unwrap();
        assert_eq!(
            formatted,
            "{\n  \"b\": 1,\n  \"a\": [\n    1,\n    2\n  ]\n}\n"
        );
    }

    #[test]
    fn xml_children_are_indented_and_comments_survive() {
        let formatted =
            format("<root><a x=\"1\">text</a><!-- note --><b/></root>", Kind::Xml).unwrap();
        assert_eq!(
            formatted,
            "<root>\n  <a x=\"1\">text</a>\n  <!-- note -->\n  <b/>\n</root>\n"
        );
    }

    #[test]
    fn xml_with_mixed_content_is_left_alone() {
        let problem = format("<p>Hello <b>world</b>.</p>", Kind::Xml).unwrap_err();
        assert!(problem.message.contains("mixes text"), "{}", problem.message);
    }

    #[test]
    fn toml_and_yaml_say_why_they_are_not_formatted() {
        assert!(format("a = 1\n", Kind::Toml).is_err());
        assert!(format("a: 1\n", Kind::Yaml).is_err());
    }
}
