//! Reading comma-separated values.
//!
//! RFC 4180, which is what spreadsheets actually write: fields separated by a
//! delimiter, quoted fields that may contain the delimiter, quotes and line
//! breaks, and `""` inside a quoted field for a literal quote.
//!
//! The parser is here rather than in the view so that it can be tested
//! without a window.

/// The delimiter a file uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delimiter {
    /// `,` — a `.csv` file.
    Comma,
    /// `\t` — a `.tsv` file.
    Tab,
}

impl Delimiter {
    /// The character itself.
    pub fn as_char(self) -> char {
        match self {
            Delimiter::Comma => ',',
            Delimiter::Tab => '\t',
        }
    }

    /// The delimiter for a file name.
    pub fn for_path(path: &str) -> Delimiter {
        if path.to_ascii_lowercase().ends_with(".tsv") {
            Delimiter::Tab
        } else {
            Delimiter::Comma
        }
    }
}

/// A parsed table.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Table {
    /// The rows, each a list of fields. Rows may have different lengths, which
    /// is what a ragged file has and what the view pads.
    pub rows: Vec<Vec<String>>,
}

impl Table {
    /// Parse `text`.
    pub fn parse(text: &str, delimiter: Delimiter) -> Self {
        let delimiter = delimiter.as_char();
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut row: Vec<String> = Vec::new();
        let mut field = String::new();
        let mut chars = text.chars().peekable();
        let mut quoted = false;
        // Whether the field being built has any content or quotes at all, so
        // that a trailing empty field is kept and a trailing newline is not.
        let mut started = false;

        while let Some(ch) = chars.next() {
            if quoted {
                match ch {
                    '"' => {
                        if chars.peek() == Some(&'"') {
                            chars.next();
                            field.push('"');
                        } else {
                            quoted = false;
                        }
                    }
                    _ => field.push(ch),
                }
                continue;
            }

            match ch {
                '"' if field.is_empty() && !started => {
                    quoted = true;
                    started = true;
                }
                c if c == delimiter => {
                    row.push(std::mem::take(&mut field));
                    started = false;
                }
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                    started = false;
                }
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                    started = false;
                }
                other => {
                    field.push(other);
                    started = true;
                }
            }
        }

        // A last row that does not end in a newline.
        if started || !field.is_empty() || !row.is_empty() {
            row.push(field);
            rows.push(row);
        }

        Self { rows }
    }

    /// The widest row, in fields.
    pub fn columns(&self) -> usize {
        self.rows.iter().map(|row| row.len()).max().unwrap_or(0)
    }

    /// Whether there is nothing to show.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The body rows in display order, after the header, filtered and sorted.
    ///
    /// A filter keeps a row when any of its fields contains the needle,
    /// case-insensitively. Sorting compares two numbers numerically and
    /// anything else as lowercased text, so a column of sizes does not sort
    /// "10" before "9".
    pub fn visible_rows(&self, sort: Option<(usize, bool)>, filter: &str) -> Vec<usize> {
        let needle = filter.trim().to_lowercase();
        let mut rows: Vec<usize> = (1..self.rows.len())
            .filter(|index| {
                needle.is_empty()
                    || self.rows[*index]
                        .iter()
                        .any(|field| field.to_lowercase().contains(&needle))
            })
            .collect();

        if let Some((column, ascending)) = sort {
            rows.sort_by(|left, right| {
                let left = self.field(*left, column);
                let right = self.field(*right, column);
                match (left.parse::<f64>(), right.parse::<f64>()) {
                    (Ok(left), Ok(right)) => left
                        .partial_cmp(&right)
                        .unwrap_or(std::cmp::Ordering::Equal),
                    _ => left.to_lowercase().cmp(&right.to_lowercase()),
                }
            });
            if !ascending {
                rows.reverse();
            }
        }
        rows
    }

    /// A field, padding short rows with empty strings.
    pub fn field(&self, row: usize, column: usize) -> &str {
        self.rows
            .get(row)
            .and_then(|row| row.get(column))
            .map(|field| field.as_str())
            .unwrap_or("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Table {
        Table::parse(text, Delimiter::Comma)
    }

    #[test]
    fn a_simple_table_parses() {
        let table = parse("a,b,c\n1,2,3\n");
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[0], vec!["a", "b", "c"]);
        assert_eq!(table.rows[1], vec!["1", "2", "3"]);
        assert_eq!(table.columns(), 3);
    }

    #[test]
    fn a_quoted_field_may_contain_the_delimiter() {
        let table = parse("\"hello, world\",2\n");
        assert_eq!(table.rows[0][0], "hello, world");
        assert_eq!(table.rows[0][1], "2");
    }

    #[test]
    fn a_quoted_field_may_contain_a_newline() {
        let table = parse("\"line one\nline two\",x\n");
        assert_eq!(table.rows.len(), 1);
        assert_eq!(table.rows[0][0], "line one\nline two");
    }

    #[test]
    fn doubled_quotes_are_one_quote() {
        let table = parse("\"he said \"\"hi\"\"\",1\n");
        assert_eq!(table.rows[0][0], "he said \"hi\"");
    }

    #[test]
    fn crlf_line_endings_are_handled() {
        let table = parse("a,b\r\nc,d\r\n");
        assert_eq!(table.rows, vec![vec!["a", "b"], vec!["c", "d"]]);
    }

    #[test]
    fn a_trailing_newline_does_not_add_an_empty_row() {
        let table = parse("a,b\n");
        assert_eq!(table.rows.len(), 1);
    }

    #[test]
    fn empty_fields_are_kept() {
        let table = parse("a,,c\n");
        assert_eq!(table.rows[0], vec!["a", "", "c"]);
    }

    #[test]
    fn tabs_parse_with_the_tab_delimiter() {
        let table = Table::parse("a\tb\n1\t2\n", Delimiter::Tab);
        assert_eq!(table.rows[0], vec!["a", "b"]);
    }

    #[test]
    fn the_delimiter_follows_the_extension() {
        assert_eq!(Delimiter::for_path("a.csv"), Delimiter::Comma);
        assert_eq!(Delimiter::for_path("a.tsv"), Delimiter::Tab);
        assert_eq!(Delimiter::for_path("a.TSV"), Delimiter::Tab);
    }

    #[test]
    fn a_ragged_table_pads_with_empty_fields() {
        let table = parse("a,b,c\n1\n");
        assert_eq!(table.columns(), 3);
        assert_eq!(table.field(1, 2), "");
    }

    #[test]
    fn rows_sort_numerically_and_keep_their_header_out() {
        let table = parse("name,size\nbig,10\nsmall,2\nmedium,9\n");
        let rows = table.visible_rows(Some((1, true)), "");
        let names: Vec<&str> = rows
            .iter()
            .map(|index| table.field(*index, 0))
            .collect();
        assert_eq!(names, vec!["small", "medium", "big"]);
        let rows = table.visible_rows(Some((1, false)), "");
        let names: Vec<&str> = rows
            .iter()
            .map(|index| table.field(*index, 0))
            .collect();
        assert_eq!(names, vec!["big", "medium", "small"]);
    }

    #[test]
    fn a_filter_matches_any_field_case_insensitively() {
        let table = parse("name,lang\nRune,Rust\nlide,Rust\nZed,C\n");
        let rows = table.visible_rows(None, "rust");
        assert_eq!(rows.len(), 2, "the two Rust rows");
        assert!(rows.iter().all(|index| table.field(*index, 1) == "Rust"));
        assert!(table.visible_rows(None, "absent").is_empty());
        assert_eq!(table.visible_rows(None, "").len(), 3);
    }

    #[test]
    fn an_empty_file_is_an_empty_table() {
        assert!(parse("").is_empty());
        assert_eq!(parse("").columns(), 0);
    }
}
