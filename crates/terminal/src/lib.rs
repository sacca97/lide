//! A PTY-backed terminal.
//!
//! Two layers, kept apart as the plan asks:
//!
//! * **Process management** — [`Session`] opens a PTY, starts a shell and
//!   moves bytes. It knows nothing about escape sequences.
//! * **Emulation** — [`Emulator`] wraps `alacritty_terminal`'s `Term`, which
//!   parses what the shell writes and keeps the grid.
//!
//! The split is what makes each testable: the session can be driven with a
//! shell that only prints, and the emulator can be fed bytes with no process
//! at all.

use std::io::{Read, Write};
use std::sync::mpsc;

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::{Dimensions, Grid, Scroll};
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{Config, RenderableCursor, Term};
use alacritty_terminal::vte::ansi::{Color as AnsiColor, CursorShape, NamedColor, Processor};

/// Errors from the terminal.
#[derive(Debug, thiserror::Error)]
pub enum TerminalError {
    /// The PTY could not be opened.
    #[error("could not open a terminal: {0}")]
    Pty(String),
    /// The shell could not be started.
    #[error("could not start {shell}: {message}")]
    Spawn {
        /// The shell that failed.
        shell: String,
        /// Why.
        message: String,
    },
}

/// The size of a terminal, in cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    /// Columns.
    pub cols: usize,
    /// Rows.
    pub rows: usize,
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

/// The shell to run.
///
/// `$SHELL`, else the login shell, else `sh`. The editor never invents a
/// shell the user does not have.
pub fn default_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .filter(|shell| !shell.trim().is_empty())
        .unwrap_or_else(|| "sh".to_string())
}

/// What the emulator's grid looks like from outside.
pub struct Snapshot {
    /// One string per row, trailing blanks trimmed.
    pub rows: Vec<String>,
    /// The shell's colour and weight for the text in each row.
    pub spans: Vec<Vec<rune::Span>>,
    /// Where the cursor is, as `(row, column)`.
    pub cursor: (usize, usize),
    /// Whether the cursor is drawn.
    pub cursor_visible: bool,
}

/// A running shell.
pub struct Session {
    /// The master side, kept so that the PTY can be resized.
    master: Box<dyn portable_pty::MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    /// Output from the shell, already read off the PTY.
    output: mpsc::Receiver<Vec<u8>>,
    /// The child process, kept so that it can be killed and reaped.
    child: Box<dyn portable_pty::Child + Send + Sync>,
    /// Set when the shell has exited.
    pub exited: bool,
    /// The shell that was started.
    pub shell: String,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("shell", &self.shell)
            .field("exited", &self.exited)
            .finish_non_exhaustive()
    }
}

impl Session {
    /// Start `shell` in `directory`, sized `size`.
    pub fn spawn(shell: &str, directory: &str, size: Size) -> Result<Self, TerminalError> {
        Self::spawn_inner(shell, &[], true, directory, size)
    }

    /// Start an arbitrary program in `directory`, sized `size`.
    ///
    /// Used for `ssh host`: the program's arguments are passed as given, and
    /// no login-shell flag is appended.
    pub fn spawn_command(
        program: &str,
        args: &[String],
        directory: &str,
        size: Size,
    ) -> Result<Self, TerminalError> {
        Self::spawn_inner(program, args, false, directory, size)
    }

    fn spawn_inner(
        program: &str,
        args: &[String],
        login: bool,
        directory: &str,
        size: Size,
    ) -> Result<Self, TerminalError> {
        let system = portable_pty::native_pty_system();
        let pair = system
            .openpty(portable_pty::PtySize {
                rows: size.rows as u16,
                cols: size.cols as u16,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| TerminalError::Pty(error.to_string()))?;

        let mut command = portable_pty::CommandBuilder::new(program);
        command.cwd(directory);
        command.env("TERM", "xterm-256color");
        for arg in args {
            command.arg(arg);
        }
        if login {
            // A login shell, so that the user's aliases and PATH are there.
            command.arg("-l");
        }

        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| TerminalError::Spawn {
                shell: program.to_string(),
                message: error.to_string(),
            })?;
        // The slave side must be dropped from the parent, or the reader never
        // sees end-of-stream when the shell exits.
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| TerminalError::Pty(error.to_string()))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| TerminalError::Pty(error.to_string()))?;

        let (tx, output) = mpsc::channel();
        std::thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(read) => {
                        if tx.send(buffer[..read].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            master: pair.master,
            writer,
            output,
            child,
            exited: false,
            shell: program.to_string(),
        })
    }

    /// Resize the PTY, so that the shell's idea of the window matches.
    pub fn resize(&mut self, size: Size) {
        let _ = self.master.resize(portable_pty::PtySize {
            rows: size.rows as u16,
            cols: size.cols as u16,
            pixel_width: 0,
            pixel_height: 0,
        });
    }

    /// Write bytes to the shell.
    pub fn write(&mut self, bytes: &[u8]) {
        if self.writer.write_all(bytes).is_err() {
            self.exited = true;
        }
        let _ = self.writer.flush();
    }

    /// Everything the shell has written since the last call.
    ///
    /// Also notices that the shell has gone, which is what makes a terminal
    /// show "the process exited" instead of a frozen screen.
    pub fn drain(&mut self) -> Vec<u8> {
        let mut bytes = Vec::new();
        while let Ok(chunk) = self.output.try_recv() {
            bytes.extend_from_slice(&chunk);
        }
        if !self.exited {
            if let Ok(Some(_status)) = self.child.try_wait() {
                self.exited = true;
            }
        }
        bytes
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // The shell is a child of the editor: leaving it running would leave a
        // stray process behind every time a terminal tab is closed.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The VT emulator, wrapping `alacritty_terminal`.
pub struct Emulator {
    term: Term<Listener>,
    parser: Processor,
    size: Size,
}

/// The emulator's event sink.
///
/// `alacritty_terminal` reports bells, title changes and clipboard requests
/// through it. None of them are needed yet, so it does nothing.
struct Listener;

impl EventListener for Listener {}

impl Emulator {
    /// A new emulator of `size`.
    pub fn new(size: Size) -> Self {
        let term = Term::new(Config::default(), &size, Listener);
        Self {
            term,
            parser: Processor::new(),
            size,
        }
    }

    /// The grid, for callers that need cells rather than text.
    pub fn grid(&self) -> &Grid<Cell> {
        self.term.grid()
    }

    /// The size the emulator is laid out at.
    pub fn size(&self) -> Size {
        self.size
    }

    /// Feed the shell's output to the parser.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.term, bytes);
    }

    /// Resize the grid.
    pub fn resize(&mut self, size: Size) {
        if size == self.size {
            return;
        }
        self.size = size;
        self.term.resize(size);
    }

    /// Scroll the view back into the scrollback, positive by `lines`, and
    /// forward again with a negative count. The grid clamps at both ends.
    pub fn scroll(&mut self, lines: i32) {
        self.term.scroll_display(Scroll::Delta(lines));
    }

    /// Return to the line the shell is writing.
    pub fn scroll_to_bottom(&mut self) {
        self.term.scroll_display(Scroll::Bottom);
    }

    /// How far the view is scrolled back from the bottom, in lines.
    pub fn scrollback(&self) -> usize {
        self.term.grid().display_offset()
    }

    /// Every line the terminal holds, scrollback first, as text.
    pub fn all_text(&self) -> String {
        let history = self.term.grid().history_size() as i32;
        let start = Point::new(Line(-history), Column(0));
        let end = Point::new(
            Line(self.size.rows as i32 - 1),
            Column(self.size.cols - 1),
        );
        self.term.bounds_to_string(start, end)
    }

    /// Scroll so that `line`, counted from the top of [`Emulator::all_text`],
    /// is the first line in the view.
    pub fn scroll_to_line(&mut self, line: usize) {
        let history = self.term.grid().history_size() as i32;
        let desired = (history - line as i32).clamp(0, history);
        let delta = desired - self.term.grid().display_offset() as i32;
        self.scroll(delta);
    }

    /// The text between two `(row, column)` points in the visible grid.
    ///
    /// The end point's character is included; a column past the end of a line
    /// stops at the line. Columns are character offsets, not bytes.
    pub fn selection_text(&self, start: (usize, usize), end: (usize, usize)) -> String {
        let rows = self.snapshot().rows;
        let (start, end) = if start <= end { (start, end) } else { (end, start) };
        let (start_row, start_column) = start;
        let (end_row, end_column) = end;
        if rows.is_empty() || start_row >= rows.len() {
            return String::new();
        }

        let mut lines = Vec::new();
        for (index, row) in rows
            .iter()
            .enumerate()
            .skip(start_row)
            .take(end_row.min(rows.len() - 1) - start_row + 1)
        {
            let text: Vec<char> = row.chars().collect();
            let from = if index == start_row {
                start_column.min(text.len())
            } else {
                0
            };
            let to = if index == end_row {
                (end_column + 1).min(text.len())
            } else {
                text.len()
            };
            if from < to {
                lines.push(text[from..to].iter().collect::<String>());
            }
        }
        lines.join("\n")
    }

    /// The lines holding `query`, case-insensitively, counted from the top of
    /// [`Emulator::all_text`].
    pub fn find(&self, query: &str) -> Vec<usize> {
        let needle = query.to_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }
        self.all_text()
            .lines()
            .enumerate()
            .filter(|(_, line)| line.to_lowercase().contains(&needle))
            .map(|(index, _)| index)
            .collect()
    }

    /// The grid, as text, with the shell's colours.
    pub fn snapshot(&self) -> Snapshot {
        let content = self.term.renderable_content();
        let cursor: RenderableCursor = content.cursor;
        let offset = content.display_offset as i32;
        let mut rows = vec![String::new(); self.size.rows];
        let mut spans = vec![Vec::new(); self.size.rows];

        // One styled run per change of colour or weight; gaps use the theme's
        // ordinary text, not the shell's background.
        let mut current: (Option<rune::Color>, Option<rune::Color>, bool) = (None, None, false);
        let mut span_start = 0usize;
        let mut span_line = 0usize;

        let close = |spans: &mut Vec<rune::Span>,
                     start: usize,
                     end: usize,
                     (fg, bg, bold): (Option<rune::Color>, Option<rune::Color>, bool)| {
            if end <= start {
                return;
            }
            if fg.is_none() && bg.is_none() && !bold {
                return;
            }
            let mut span = rune::Span::new(start, end);
            if let Some(color) = fg {
                span = span.color(color);
            }
            if let Some(color) = bg {
                span = span.bg(color, 0.0);
            }
            if bold {
                span = span.bold();
            }
            spans.push(span);
        };

        for cell in content.display_iter {
            // A line in the scrollback is negative in grid coordinates; the
            // display offset turns it back into a row of the view.
            let line = cell.point.line.0 + offset;
            if line < 0 {
                continue;
            }
            let line = line as usize;
            if line >= rows.len() {
                continue;
            }

            if span_line != line {
                close(&mut spans[span_line], span_start, rows[span_line].len(), current);
                span_line = line;
                span_start = 0;
                current = (None, None, false);
            }

            let fg = ansi_color(cell.fg);
            let bg = match cell.bg {
                AnsiColor::Named(NamedColor::Background) => None,
                other => ansi_color(other),
            };
            let bold = cell.flags.contains(Flags::BOLD);
            if (fg, bg, bold) != current {
                close(&mut spans[span_line], span_start, rows[line].len(), current);
                span_start = rows[line].len();
                current = (fg, bg, bold);
            }

            let row = &mut rows[line];
            let column = cell.point.column.0;
            // Pad up to this column: the grid skips cells whose glyph a wide
            // character already covered.
            while row.chars().count() < column {
                row.push(' ');
            }
            let c = cell.c;
            if c != ' ' && c != '\0' && row.chars().count() == column {
                row.push(c);
            }
        }

        for row in &mut rows {
            let trimmed = row.trim_end().len();
            row.truncate(trimmed);
        }
        close(&mut spans[span_line], span_start, rows[span_line].len(), current);
        for (line, row) in rows.iter().enumerate() {
            spans[line].retain_mut(|span| {
                if span.start as usize >= row.len() {
                    return false;
                }
                span.end = span.end.min(row.len() as u32);
                true
            });
        }

        Snapshot {
            rows,
            spans,
            cursor: (
                (cursor.point.line.0 + offset).max(0) as usize,
                cursor.point.column.0,
            ),
            // The shell's cursor is on the last line; scrolled back it is not
            // where the reader is looking.
            cursor_visible: !matches!(cursor.shape, CursorShape::Hidden) && offset == 0,
        }
    }
}

/// Turn an ANSI colour into a paintable one.
///
/// The terminal's own foreground/background are left to the theme. Named and
/// indexed colours use the conventional sixteen-colour palette plus the
/// xterm colour cube; true colour is passed through.
fn ansi_color(color: AnsiColor) -> Option<rune::Color> {
    match color {
        AnsiColor::Spec(rgb) => Some(rgb_to_color(rgb.r, rgb.g, rgb.b)),
        AnsiColor::Indexed(index) => Some(indexed_color(index)),
        AnsiColor::Named(named) => named_color(named),
    }
}

/// The standard sixteen-colour palette, shared by named and indexed colours.
fn palette_color(index: usize) -> rune::Color {
    const PALETTE: [&str; 16] = [
        "#000000", "#cd3131", "#0dbc79", "#e5e510", "#2472c8", "#bc3fbc", "#11a8cd",
        "#e5e5e5", "#666666", "#f14c4c", "#23d18b", "#f5f543", "#3b8eea", "#d670d6",
        "#29b8db", "#ffffff",
    ];
    rune::color::hex(PALETTE[index.min(15)]).expect("a literal colour")
}

fn named_color(named: NamedColor) -> Option<rune::Color> {
    match named {
        NamedColor::Black => Some(palette_color(0)),
        NamedColor::Red => Some(palette_color(1)),
        NamedColor::Green => Some(palette_color(2)),
        NamedColor::Yellow => Some(palette_color(3)),
        NamedColor::Blue => Some(palette_color(4)),
        NamedColor::Magenta => Some(palette_color(5)),
        NamedColor::Cyan => Some(palette_color(6)),
        NamedColor::White => Some(palette_color(7)),
        NamedColor::BrightBlack => Some(palette_color(8)),
        NamedColor::BrightRed => Some(palette_color(9)),
        NamedColor::BrightGreen => Some(palette_color(10)),
        NamedColor::BrightYellow => Some(palette_color(11)),
        NamedColor::BrightBlue => Some(palette_color(12)),
        NamedColor::BrightMagenta => Some(palette_color(13)),
        NamedColor::BrightCyan => Some(palette_color(14)),
        NamedColor::BrightWhite => Some(palette_color(15)),
        // Foreground, background, cursor and the dim variants are resolved by
        // the window's theme, not by the shell's palette.
        _ => None,
    }
}

fn indexed_color(index: u8) -> rune::Color {
    match index {
        0..=15 => palette_color(index as usize),
        16..=231 => {
            let index = index as u32 - 16;
            let value = |n: u32| if n == 0 { 0 } else { n * 40 + 55 };
            let r = index / 36;
            let g = (index / 6) % 6;
            let b = index % 6;
            rgb_to_color(value(r) as u8, value(g) as u8, value(b) as u8)
        }
        232..=255 => {
            let gray = (index as u32 - 232) * 10 + 8;
            rgb_to_color(gray as u8, gray as u8, gray as u8)
        }
    }
}

fn rgb_to_color(r: u8, g: u8, b: u8) -> rune::Color {
    rune::Color {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
}

/// The bytes a key produces in a terminal.
///
/// This is the small, well-defined part of terminal input: printable text,
/// the control characters, and the cursor keys. Everything an application
/// might negotiate beyond that (bracketed paste, the Kitty keyboard
/// protocol) is not implemented.
pub fn key_bytes(key: &Key) -> Option<Vec<u8>> {
    Some(match key {
        Key::Text(text) => text.as_bytes().to_vec(),
        Key::Enter => b"\r".to_vec(),
        Key::Tab => b"\t".to_vec(),
        Key::Backspace => vec![0x7f],
        Key::Escape => vec![0x1b],
        Key::Up => b"\x1b[A".to_vec(),
        Key::Down => b"\x1b[B".to_vec(),
        Key::Right => b"\x1b[C".to_vec(),
        Key::Left => b"\x1b[D".to_vec(),
        Key::Home => b"\x1b[H".to_vec(),
        Key::End => b"\x1b[F".to_vec(),
        Key::PageUp => b"\x1b[5~".to_vec(),
        Key::PageDown => b"\x1b[6~".to_vec(),
        Key::Delete => b"\x1b[3~".to_vec(),
        Key::Control(letter) => {
            let upper = letter.to_ascii_uppercase();
            if upper.is_ascii() {
                vec![(upper as u8) & 0x1f]
            } else {
                return None;
            }
        }
        Key::Unsupported => return None,
    })
}

/// A key, in the terms the terminal cares about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    /// Text to insert.
    Text(String),
    /// Return.
    Enter,
    /// Tab.
    Tab,
    /// Backspace.
    Backspace,
    /// Escape.
    Escape,
    /// Arrow up.
    Up,
    /// Arrow down.
    Down,
    /// Arrow left.
    Left,
    /// Arrow right.
    Right,
    /// Home.
    Home,
    /// End.
    End,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
    /// Delete.
    Delete,
    /// A letter typed with Control held.
    Control(char),
    /// Anything the terminal does not handle.
    Unsupported,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_emulator_is_blank() {
        let emulator = Emulator::new(Size { cols: 40, rows: 5 });
        let snapshot = emulator.snapshot();
        assert_eq!(snapshot.rows.len(), 5);
        assert!(snapshot.rows.iter().all(|row| row.is_empty()));
    }

    #[test]
    fn text_lands_in_the_grid() {
        let mut emulator = Emulator::new(Size { cols: 40, rows: 5 });
        emulator.feed(b"hello");
        let snapshot = emulator.snapshot();
        assert_eq!(snapshot.rows[0], "hello");
    }

    #[test]
    fn a_newline_starts_a_row() {
        let mut emulator = Emulator::new(Size { cols: 40, rows: 5 });
        emulator.feed(b"one\r\ntwo");
        let snapshot = emulator.snapshot();
        assert_eq!(snapshot.rows[0], "one");
        assert_eq!(snapshot.rows[1], "two");
    }

    #[test]
    fn an_escape_sequence_moves_the_cursor() {
        let mut emulator = Emulator::new(Size { cols: 40, rows: 5 });
        // Move to row 3, column 5, then write.
        emulator.feed(b"\x1b[3;5Hx");
        let snapshot = emulator.snapshot();
        assert_eq!(snapshot.rows[2], "    x");
        assert_eq!(snapshot.cursor.0, 2);
    }

    #[test]
    fn the_grid_keeps_the_shells_colours() {
        let mut emulator = Emulator::new(Size { cols: 40, rows: 5 });
        emulator.feed(b"\x1b[31mred\x1b[0m plain");
        let snapshot = emulator.snapshot();
        assert_eq!(snapshot.rows[0], "red plain");
        let span = &snapshot.spans[0][0];
        assert_eq!((span.start, span.end), (0, 3));
        assert!(span.color.is_some());
        assert_eq!(span.color.unwrap(), named_color(NamedColor::Red).unwrap());
    }

    #[test]
    fn bold_becomes_a_weight_span() {
        let mut emulator = Emulator::new(Size { cols: 40, rows: 5 });
        emulator.feed(b"\x1b[1mbold\x1b[0m");
        let snapshot = emulator.snapshot();
        assert_eq!(snapshot.spans[0][0].weight, Some(600.0));
    }

    #[test]
    fn colours_are_parsed_without_disturbing_the_text() {
        let mut emulator = Emulator::new(Size { cols: 40, rows: 5 });
        emulator.feed(b"\x1b[31mred\x1b[0m plain");
        assert_eq!(emulator.snapshot().rows[0], "red plain");
    }

    #[test]
    fn resizing_changes_the_grid() {
        let mut emulator = Emulator::new(Size { cols: 40, rows: 5 });
        emulator.resize(Size { cols: 20, rows: 3 });
        assert_eq!(emulator.size(), Size { cols: 20, rows: 3 });
        assert_eq!(emulator.snapshot().rows.len(), 3);
    }

    #[test]
    fn the_scrollback_holds_lines_that_scrolled_off() {
        let mut emulator = Emulator::new(Size { cols: 40, rows: 4 });
        let text = (0..10)
            .map(|index| format!("line{index}"))
            .collect::<Vec<_>>()
            .join("\r\n");
        emulator.feed(text.as_bytes());

        let bottom = emulator.snapshot();
        assert_eq!(bottom.rows[0], "line6", "the last four lines are visible");
        assert_eq!(bottom.rows[3], "line9");
        assert_eq!(emulator.scrollback(), 0);

        emulator.scroll(3);
        assert_eq!(emulator.scrollback(), 3);
        let back = emulator.snapshot();
        assert_eq!(back.rows[0], "line3");
        assert_eq!(back.rows[3], "line6");
        assert!(!back.cursor_visible, "the shell's cursor is not in the history");

        emulator.scroll_to_bottom();
        assert_eq!(emulator.scrollback(), 0);
        assert_eq!(emulator.snapshot().rows[0], "line6");
    }

    #[test]
    fn a_command_runs_with_its_arguments() {
        let mut session = Session::spawn_command(
            "/bin/sh",
            &["-c".to_string(), "printf ssh-args".to_string()],
            "/tmp",
            Size { cols: 40, rows: 5 },
        )
        .expect("the shell starts");
        let mut output = Vec::new();
        for _ in 0..200 {
            output.extend(session.drain());
            if String::from_utf8_lossy(&output).contains("ssh-args") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(String::from_utf8_lossy(&output).contains("ssh-args"));
    }

    #[test]
    fn a_selection_reads_the_cells_between_its_points() {
        let mut emulator = Emulator::new(Size { cols: 40, rows: 5 });
        emulator.feed(b"alpha beta\r\ngamma delta\r\nthird");

        assert_eq!(emulator.selection_text((0, 0), (0, 4)), "alpha");
        assert_eq!(
            emulator.selection_text((0, 6), (1, 4)),
            "beta\ngamma"
        );
        // Reversed points mean the same range.
        assert_eq!(
            emulator.selection_text((1, 4), (0, 6)),
            "beta\ngamma"
        );
        // The end character is included, and a column past the line stops.
        assert_eq!(emulator.selection_text((0, 0), (0, 99)), "alpha beta");
        assert_eq!(emulator.selection_text((2, 0), (2, 99)), "third");
    }

    #[test]
    fn the_scrollback_can_be_searched_and_reached() {
        let mut emulator = Emulator::new(Size { cols: 40, rows: 4 });
        let text = (0..10)
            .map(|index| format!("line{index}"))
            .collect::<Vec<_>>()
            .join("\r\n");
        emulator.feed(text.as_bytes());

        let matches = emulator.find("LINE4");
        assert_eq!(matches, vec![4], "one line, case-insensitively");

        emulator.scroll_to_line(4);
        assert_eq!(emulator.snapshot().rows[0], "line4");
        assert!(emulator.find("absent").is_empty());
        assert!(emulator.find("").is_empty());
    }

    #[test]
    fn keys_become_the_bytes_a_shell_expects() {
        assert_eq!(key_bytes(&Key::Enter).as_deref(), Some(&b"\r"[..]));
        assert_eq!(key_bytes(&Key::Backspace).as_deref(), Some(&[0x7f][..]));
        assert_eq!(key_bytes(&Key::Up).as_deref(), Some(&b"\x1b[A"[..]));
        assert_eq!(key_bytes(&Key::Control('c')).as_deref(), Some(&[0x03][..]));
        assert_eq!(
            key_bytes(&Key::Text("hi".into())).as_deref(),
            Some(&b"hi"[..])
        );
        assert!(key_bytes(&Key::Unsupported).is_none());
    }

    #[test]
    fn control_letters_fold_to_their_control_codes() {
        assert_eq!(key_bytes(&Key::Control('a')).unwrap(), vec![1]);
        assert_eq!(key_bytes(&Key::Control('d')).unwrap(), vec![4]);
        assert_eq!(key_bytes(&Key::Control('z')).unwrap(), vec![26]);
    }

    #[test]
    fn the_default_shell_is_never_empty() {
        assert!(!default_shell().trim().is_empty());
    }
}

/// A session against a real shell.
#[cfg(test)]
mod session_tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn a_shell_runs_and_prints() {
        let directory = std::env::temp_dir();
        let mut emulator = Emulator::new(Size { cols: 80, rows: 24 });
        let mut session = Session::spawn(
            "sh",
            &directory.to_string_lossy(),
            Size { cols: 80, rows: 24 },
        )
        .expect("a shell should start");

        session.write(b"echo lide-terminal-test\n");

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut found = false;
        while Instant::now() < deadline {
            let bytes = session.drain();
            if !bytes.is_empty() {
                emulator.feed(&bytes);
            }
            let snapshot = emulator.snapshot();
            if snapshot
                .rows
                .iter()
                .any(|row| row.contains("lide-terminal-test"))
            {
                found = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(found, "the shell's output should reach the grid");
    }

    #[test]
    fn exiting_the_shell_is_noticed() {
        let directory = std::env::temp_dir();
        let mut session = Session::spawn(
            "sh",
            &directory.to_string_lossy(),
            Size { cols: 80, rows: 24 },
        )
        .expect("a shell should start");

        session.write(b"exit\n");

        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            let _ = session.drain();
            if session.exited {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(session.exited, "the session should notice the shell exiting");
    }
}

/// A terminal as the editor holds it: a session and its emulator.
pub struct TerminalView {
    /// The shell.
    pub session: Session,
    /// The grid.
    pub emulator: Emulator,
    /// The title shown on the tab.
    pub title: String,
    /// The directory the shell started in.
    pub directory: std::path::PathBuf,
    /// The pointer selection, as `(start, end)` row and column pairs in the
    /// visible grid.
    pub selection: Option<((usize, usize), (usize, usize))>,
}

impl std::fmt::Debug for TerminalView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalView")
            .field("shell", &self.session.shell)
            .field("exited", &self.session.exited)
            .finish_non_exhaustive()
    }
}

impl TerminalView {
    /// Start a shell in `directory`.
    pub fn open(shell: &str, directory: &str, size: Size) -> Result<Self, TerminalError> {
        let session = Session::spawn(shell, directory, size)?;
        let title = shell
            .rsplit('/')
            .next()
            .unwrap_or("terminal")
            .to_string();
        Ok(Self {
            session,
            emulator: Emulator::new(size),
            title,
            directory: std::path::PathBuf::from(directory),
            selection: None,
        })
    }

    /// Start an SSH session to `host`.
    pub fn open_ssh(host: &str, directory: &str, size: Size) -> Result<Self, TerminalError> {
        let session = Session::spawn_command("ssh", &[host.to_string()], directory, size)?;
        Ok(Self {
            session,
            emulator: Emulator::new(size),
            title: format!("ssh {host}"),
            directory: std::path::PathBuf::from(directory),
            selection: None,
        })
    }

    /// The title shown on the tab.
    pub fn title(&self) -> String {
        if self.session.exited {
            format!("{} (exited)", self.title)
        } else {
            self.title.clone()
        }
    }

    /// Feed everything the shell has written into the grid.
    ///
    /// Returns `true` when something arrived.
    pub fn poll(&mut self) -> bool {
        let bytes = self.session.drain();
        if bytes.is_empty() {
            return false;
        }
        self.emulator.feed(&bytes);
        // New output wins over a reader who scrolled back, as a terminal does.
        self.emulator.scroll_to_bottom();
        true
    }

    /// Send a key to the shell.
    pub fn write_key(&mut self, key: &Key) {
        if let Some(bytes) = key_bytes(key) {
            self.session.write(&bytes);
        }
    }

    /// Write text to the shell.
    pub fn write(&mut self, text: &str) {
        self.session.write(text.as_bytes());
    }

    /// Resize the PTY and the grid together.
    pub fn resize(&mut self, size: Size) {
        self.emulator.resize(size);
        self.session.resize(size);
    }

    /// The visible grid, as text, for copying.
    pub fn text(&self) -> String {
        self.emulator.snapshot().rows.join("\n")
    }

    /// What copy takes: the selection when there is one, the grid otherwise.
    pub fn selected_text(&self) -> String {
        match self.selection {
            Some((start, end)) => self.emulator.selection_text(start, end),
            None => self.text(),
        }
    }

    /// Select the cells between two points, in either order.
    pub fn select(&mut self, start: (usize, usize), end: (usize, usize)) {
        self.selection = Some((start, end));
    }

    /// Search the scrollback and the screen, and jump to the first match.
    pub fn search(&mut self, query: &str) -> Vec<usize> {
        let matches = self.emulator.find(query);
        if let Some(&first) = matches.first() {
            self.emulator.scroll_to_line(first);
        }
        matches
    }

    /// Jump to a match found by [`TerminalView::search`].
    pub fn scroll_to_line(&mut self, line: usize) {
        self.emulator.scroll_to_line(line);
    }

    /// Scroll the view, and return to the bottom when the shell writes again.
    pub fn scroll(&mut self, lines: i32) {
        self.emulator.scroll(lines);
    }
}
