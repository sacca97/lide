//! Exporting a Markdown document as HTML.
//!
//! The export is a standalone HTML file with a small embedded stylesheet, so
//! it can be opened in any browser or converted to PDF with an external tool
//! without carrying the editor's own styling along.

use pulldown_cmark::{html, Options, Parser};

use crate::parse::default_options;

/// Render Markdown source to an HTML fragment.
pub fn to_html_fragment(source: &str) -> String {
    to_html_fragment_with(source, default_options())
}

/// Render Markdown source to an HTML fragment with explicit options.
pub fn to_html_fragment_with(source: &str, options: Options) -> String {
    let mut out = String::new();
    html::push_html(&mut out, Parser::new_ext(source, options));
    out
}

/// Render Markdown source to a complete, standalone HTML document.
pub fn to_html_document(source: &str, title: &str) -> String {
    format!(
        "<!DOCTYPE html>\n\
<html lang=\"en\">\n\
<head>\n\
<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
<title>{title}</title>\n\
<style>\n\
{STYLESHEET}\n\
</style>\n\
</head>\n\
<body>\n\
{body}\n\
</body>\n\
</html>\n",
        title = escape_html(title),
        body = to_html_fragment(source),
    )
}

/// Escape the characters that are special in HTML text and attributes.
pub fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

const STYLESHEET: &str = "\
:root { color-scheme: light dark; }
body {
  margin: 0 auto;
  max-width: 48rem;
  padding: 2rem 1.25rem 6rem;
  font-family: system-ui, -apple-system, \"Segoe UI\", sans-serif;
  font-size: 16px;
  line-height: 1.6;
  color: #1f2328;
  background: #ffffff;
}
@media (prefers-color-scheme: dark) {
  body { color: #e6edf3; background: #0d1117; }
  code, pre { background: #161b22 !important; }
  th { background: #161b22 !important; }
}
h1, h2, h3, h4, h5, h6 { line-height: 1.25; margin-top: 1.5em; }
h1 { font-size: 2em; border-bottom: 1px solid #d0d7de; padding-bottom: .3em; }
h2 { font-size: 1.5em; border-bottom: 1px solid #d0d7de; padding-bottom: .3em; }
a { color: #0969da; text-decoration: none; }
a:hover { text-decoration: underline; }
code { background: #eff1f3; padding: .2em .4em; border-radius: 6px; font-size: 85%; }
pre { background: #eff1f3; padding: 1rem; border-radius: 6px; overflow-x: auto; }
pre code { background: none; padding: 0; }
blockquote {
  margin: 0;
  padding: 0 1em;
  color: #59636e;
  border-left: .25em solid #d0d7de;
}
table { border-collapse: collapse; width: 100%; margin: 1em 0; }
th, td { border: 1px solid #d0d7de; padding: .4em .7em; }
th { background: #f6f8fa; }
img { max-width: 100%; }
hr { border: 0; border-top: 1px solid #d0d7de; margin: 1.5em 0; }
.task-list-item { list-style-type: none; }
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_headings_and_paragraphs() {
        let html = to_html_fragment("# Title\n\nHello.\n");
        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("<p>Hello.</p>"));
    }

    #[test]
    fn renders_gfm_tables() {
        let html = to_html_fragment("| a | b |\n| --- | --- |\n| 1 | 2 |\n");
        assert!(html.contains("<table>"));
        assert!(html.contains("<th>a</th>"));
    }

    #[test]
    fn renders_task_lists() {
        let html = to_html_fragment("- [x] done\n");
        assert!(html.contains("checkbox"));
    }

    #[test]
    fn escapes_titles() {
        let document = to_html_document("# Hi", "<script>");
        assert!(document.contains("&lt;script&gt;"));
        assert!(!document.contains("<script>"));
    }

    #[test]
    fn document_is_self_contained() {
        let document = to_html_document("# Hi", "Title");
        assert!(document.starts_with("<!DOCTYPE html>"));
        assert!(document.contains("<style>"));
        assert!(document.contains("</html>"));
    }
}
