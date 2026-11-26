//! LaTeX builds.
//!
//! Compilation is an external process: the editor runs `texres file.tex` and
//! reads what it says. It never embeds a TeX distribution, and a missing
//! `texres` is a message in the build panel rather than a broken editor.
//!
//! The build is deliberately a *general* runner — a program, its arguments and
//! a working directory — with `texres` as the LaTeX command. That is what makes
//! it testable on a machine with no TeX: the tests run `sh` and check that its
//! output streams and its errors parse.

pub mod synctex;

pub use synctex::{SyncPoint, SyncSource, SyncTex};

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;

/// Errors from starting a build.
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    /// The program could not be started.
    #[error("could not run {program}: {message}")]
    Spawn {
        /// The program.
        program: String,
        /// Why.
        message: String,
    },
}

/// How to run a build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildOptions {
    /// The program to run.
    pub program: String,
    /// Its arguments.
    pub args: Vec<String>,
    /// Where to run it.
    pub directory: PathBuf,
}

impl BuildOptions {
    /// Where the build writes its PDF.
    ///
    /// The last argument that is not a flag is the `.tex` file; the directory
    /// is `-output-directory DIR` (or `-outdir=…` / `--outdir …`) when the
    /// command has one, and otherwise the working directory.
    pub fn pdf_path(&self) -> PathBuf {
        let directory = self
            .args
            .iter()
            .find_map(|arg| arg.strip_prefix("-outdir=").map(PathBuf::from))
            .or_else(|| {
                self.args
                    .iter()
                    .position(|arg| {
                        arg == "--outdir" || arg == "-output-directory" || arg == "-outdir"
                    })
                    .and_then(|index| self.args.get(index + 1))
                    .map(PathBuf::from)
            })
            .map(|path| {
                if path.is_absolute() {
                    path
                } else {
                    self.directory.join(path)
                }
            })
            .unwrap_or_else(|| self.directory.clone());
        let file = self
            .args
            .iter()
            .rev()
            .find(|arg| !arg.starts_with('-'))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("main.tex"));
        directory.join(file).with_extension("pdf")
    }

    /// The `.tex` file the build reads.
    ///
    /// The last argument that is not a flag. With no file argument, the
    /// conventional `main.tex`.
    pub fn tex_path(&self) -> PathBuf {
        let file = self
            .args
            .iter()
            .rev()
            .find(|arg| !arg.starts_with('-'))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("main.tex"));
        self.directory.join(file)
    }

    /// The command a user would type, for the build panel's header.
    pub fn command_line(&self) -> String {
        let mut line = self.program.clone();
        for arg in &self.args {
            line.push(' ');
            if arg.contains(' ') {
                line.push('"');
                line.push_str(arg);
                line.push('"');
            } else {
                line.push_str(arg);
            }
        }
        line
    }
}

/// Which engine builds a document.
///
/// `texres` chooses one from the document when no engine is named, which is
/// right for most documents; the explicit engines exist because a document may
/// need one and because a machine may only have one installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Engine {
    /// Let `texres` decide.
    #[default]
    Auto,
    /// `pdflatex`, through `texres -pdf`.
    PdfLatex,
    /// `xelatex`, through `texres -xelatex`.
    XeLatex,
    /// `lualatex`, through `texres -lualatex`.
    LuaLatex,
}

impl Engine {
    /// The name used in settings and in the palette's notice.
    pub fn name(self) -> &'static str {
        match self {
            Engine::Auto => "auto",
            Engine::PdfLatex => "pdflatex",
            Engine::XeLatex => "xelatex",
            Engine::LuaLatex => "lualatex",
        }
    }

    /// The engine named `name`, if there is one.
    pub fn from_name(name: &str) -> Option<Engine> {
        match name {
            "auto" => Some(Engine::Auto),
            "pdflatex" => Some(Engine::PdfLatex),
            "xelatex" => Some(Engine::XeLatex),
            "lualatex" => Some(Engine::LuaLatex),
            _ => None,
        }
    }

    /// The next engine, for a command that cycles through them.
    pub fn next(self) -> Engine {
        match self {
            Engine::Auto => Engine::PdfLatex,
            Engine::PdfLatex => Engine::XeLatex,
            Engine::XeLatex => Engine::LuaLatex,
            Engine::LuaLatex => Engine::Auto,
        }
    }

    /// The flag `texres` needs to use this engine.
    fn texres_flag(self) -> Option<&'static str> {
        match self {
            Engine::Auto => None,
            Engine::PdfLatex => Some("-pdf"),
            Engine::XeLatex => Some("-xelatex"),
            Engine::LuaLatex => Some("-lualatex"),
        }
    }
}

/// The `texres` command for a `.tex` file, letting `texres` choose the engine.
pub fn texres(tex: &Path) -> BuildOptions {
    texres_with(tex, Engine::Auto)
}

/// The build command for a `.tex` file, writing into `output` when given.
pub fn texres_in(tex: &Path, engine: Engine, output: Option<&Path>) -> BuildOptions {
    let mut options = texres_with(tex, engine);
    if let Some(output) = output {
        let output = output.to_string_lossy().into_owned();
        options
            .args
            .splice(0..0, ["-output-directory".to_string(), output]);
    }
    options
}

/// The build command for a `.tex` file with an explicit engine.
///
/// The default command is exactly `texres file.tex`: `texres` already writes
/// SyncTeX and keeps a build from stopping to ask a question nobody is there to
/// answer. Only an explicit engine adds a flag.
pub fn texres_with(tex: &Path, engine: Engine) -> BuildOptions {
    let directory = tex
        .parent()
        .map(|parent| parent.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let file = tex
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "main.tex".to_string());

    let mut args = Vec::new();
    if let Some(flag) = engine.texres_flag() {
        args.push(flag.to_string());
    }
    args.push(file);

    BuildOptions {
        program: "texres".to_string(),
        args,
        directory,
    }
}

/// A compile error, as the log reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatexError {
    /// The file the error is in.
    pub path: PathBuf,
    /// The line, counted from one.
    pub line: u64,
    /// What the compiler said.
    pub message: String,
}

/// A running build.
pub struct Build {
    /// The command that is running.
    pub options: BuildOptions,
    /// Every line the build has written.
    pub lines: Vec<String>,
    /// Whether the build has finished.
    pub finished: bool,
    /// Whether it succeeded, once it has finished.
    pub success: bool,
    output: mpsc::Receiver<String>,
    child: Option<Child>,
}

impl std::fmt::Debug for Build {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Build")
            .field("command", &self.options.command_line())
            .field("finished", &self.finished)
            .field("success", &self.success)
            .finish_non_exhaustive()
    }
}

impl Build {
    /// Start a build.
    pub fn start(options: BuildOptions) -> Result<Self, BuildError> {
        let mut child = Command::new(&options.program)
            .args(&options.args)
            .current_dir(&options.directory)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| BuildError::Spawn {
                program: options.program.clone(),
                message: error.to_string(),
            })?;

        let (tx, output) = mpsc::channel::<String>();

        // The compiler writes its log to standard output and its complaints to
        // standard error; both matter, and both have to be read or the child
        // blocks on a full pipe.
        for (reader, tag) in [
            (
                child.stdout.take().map(|out| Box::new(out) as Box<dyn std::io::Read + Send>),
                "",
            ),
            (
                child.stderr.take().map(|err| Box::new(err) as Box<dyn std::io::Read + Send>),
                "",
            ),
        ] {
            let Some(reader) = reader else { continue };
            let tx = tx.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(reader).lines().map_while(Result::ok) {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
                let _ = tag;
            });
        }

        Ok(Self {
            options,
            lines: Vec::new(),
            finished: false,
            success: false,
            output,
            child: Some(child),
        })
    }

    /// Collect the output that arrived since the last call.
    ///
    /// Returns `true` when something arrived or the build finished.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        while let Ok(line) = self.output.try_recv() {
            self.lines.push(line);
            changed = true;
        }

        if !self.finished {
            if let Some(child) = self.child.as_mut() {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        self.finished = true;
                        self.success = status.success();
                        self.child = None;
                        changed = true;
                        // Flush whatever is still in the pipe.
                        while let Ok(line) = self.output.try_recv() {
                            self.lines.push(line);
                        }
                    }
                    Ok(None) => {}
                    Err(_) => {
                        self.finished = true;
                        self.success = false;
                        self.child = None;
                        changed = true;
                    }
                }
            }
        }
        changed
    }

    /// Stop the build.
    pub fn cancel(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.finished = true;
        self.success = false;
    }

    /// The errors in the output so far.
    pub fn errors(&self) -> Vec<LatexError> {
        parse_errors(&self.lines, &self.options.directory)
    }

    /// The last few lines, for the panel.
    pub fn tail(&self, count: usize) -> &[String] {
        let start = self.lines.len().saturating_sub(count);
        &self.lines[start..]
    }
}

impl Drop for Build {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Find the errors in a compiler log.
///
/// Some tools write `path:line: message` for an error. Otherwise TeX writes
/// `! message` and puts the line on a following `l.NN` line; both forms are
/// understood so that a build reads the same however it was run.
pub fn parse_errors(lines: &[String], directory: &Path) -> Vec<LatexError> {
    let mut errors: Vec<LatexError> = Vec::new();
    let mut pending: Option<(PathBuf, String)> = None;

    for line in lines {
        let trimmed = line.trim_start();

        // `error: message`, with `  --> path:line:column` on a later line.
        // This is the form `texres` prints; it is more precise than the
        // compiler's own `path:line:` form.
        if let Some(message) = trimmed.strip_prefix("error: ") {
            pending = Some((PathBuf::new(), message.trim().to_string()));
            continue;
        }
        if let Some(location) = trimmed.strip_prefix("--> ") {
            if let Some((path, number)) = parse_texres_location(location, directory) {
                if let Some((pending_path, message)) = pending.take() {
                    errors.push(LatexError {
                        path: if pending_path.as_os_str().is_empty() {
                            path
                        } else {
                            pending_path
                        },
                        line: number,
                        message,
                    });
                }
                continue;
            }
        }

        // `path:line: message`
        if let Some(error) = parse_file_line_error(line, directory) {
            errors.push(error);
            pending = None;
            continue;
        }

        // `! message`, with the line number on a later line.
        if let Some(message) = line.strip_prefix("! ") {
            pending = Some((PathBuf::new(), message.to_string()));
            continue;
        }

        // `l.NN ...` gives the line for the message above it.
        if let Some((path, message)) = pending.take() {
            if let Some(rest) = line.trim_start().strip_prefix("l.") {
                let digits: String = rest.chars().take_while(|ch| ch.is_ascii_digit()).collect();
                if let Ok(number) = digits.parse::<u64>() {
                    errors.push(LatexError {
                        path: if path.as_os_str().is_empty() {
                            directory.to_path_buf()
                        } else {
                            path
                        },
                        line: number,
                        message,
                    });
                }
            } else {
                // Not a line number; keep waiting for one.
                pending = Some((path, message));
            }
        }
    }

    // A message with no line number still counts.
    if let Some((path, message)) = pending {
        errors.push(LatexError {
            path: if path.as_os_str().is_empty() {
                directory.to_path_buf()
            } else {
                path
            },
            line: 0,
            message,
        });
    }

    errors
}

/// The location in a `texres` `--> path:line:column` line.
fn parse_texres_location(text: &str, directory: &Path) -> Option<(PathBuf, u64)> {
    let text = text.trim();
    let mut parts = text.rsplitn(3, ':');
    let last = parts.next()?;
    let second = parts.next()?;
    let first = parts.next();

    let (path, number) = match first {
        // `path:line:column`
        Some(first) if last.parse::<u64>().is_ok() => (first, second.parse::<u64>().ok()?),
        // `path:line`
        _ => (second, last.parse::<u64>().ok()?),
    };

    let path = PathBuf::from(path);
    let path = if path.is_absolute() {
        path
    } else {
        directory.join(path)
    };
    Some((path, number))
}

fn parse_file_line_error(line: &str, directory: &Path) -> Option<LatexError> {
    let line = line.trim_start();
    if !line.starts_with("./") && !line.starts_with('/') {
        return None;
    }
    let mut parts = line.splitn(3, ':');
    let path = parts.next()?;
    let number = parts.next()?.trim();
    let message = parts.next()?.trim();
    let number: u64 = number.parse().ok()?;
    if message.is_empty() {
        return None;
    }

    let path = PathBuf::from(path);
    let path = if path.is_absolute() {
        path
    } else {
        // `./main.tex` is relative to where the build ran.
        directory.join(path.file_name().unwrap_or(path.as_os_str()))
    };
    Some(LatexError {
        path,
        line: number,
        message: message.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<String> {
        text.lines().map(|line| line.to_string()).collect()
    }

    #[test]
    fn the_texres_command_is_just_the_file() {
        let options = texres(Path::new("/work/paper/main.tex"));
        assert_eq!(options.program, "texres");
        assert_eq!(options.directory, Path::new("/work/paper"));
        assert_eq!(options.args, ["main.tex"]);
    }

    #[test]
    fn the_default_engine_is_texres_auto() {
        let options = texres_in(Path::new("/work/main.tex"), Engine::default(), None);
        assert_eq!(options.program, "texres");
        assert_eq!(
            options.args,
            ["main.tex"],
            "the default is exactly `texres main.tex`"
        );
    }

    #[test]
    fn each_engine_gets_its_own_flag() {
        let tex = Path::new("/work/main.tex");
        assert!(texres_with(tex, Engine::PdfLatex)
            .args
            .contains(&"-pdf".to_string()));
        assert!(texres_with(tex, Engine::XeLatex)
            .args
            .contains(&"-xelatex".to_string()));
        assert!(texres_with(tex, Engine::LuaLatex)
            .args
            .contains(&"-lualatex".to_string()));
        // The flags are alternatives, not additions.
        assert!(!texres_with(tex, Engine::XeLatex)
            .args
            .contains(&"-pdf".to_string()));
    }

    #[test]
    fn engines_cycle_and_round_trip_through_their_names() {
        let mut engine = Engine::Auto;
        let mut seen = Vec::new();
        for _ in 0..4 {
            seen.push(engine.name());
            assert_eq!(Engine::from_name(engine.name()), Some(engine));
            engine = engine.next();
        }
        assert_eq!(engine, Engine::Auto, "four steps in a cycle of four");
        assert_eq!(seen, vec!["auto", "pdflatex", "xelatex", "lualatex"]);
        assert_eq!(Engine::from_name("tex"), None);
    }

    #[test]
    fn an_output_directory_becomes_texres_flags() {
        let options = texres_in(
            Path::new("/work/main.tex"),
            Engine::PdfLatex,
            Some(Path::new("/work/out")),
        );
        assert_eq!(
            options.args[..2],
            ["-output-directory".to_string(), "/work/out".to_string()]
        );
        assert_eq!(options.pdf_path(), Path::new("/work/out/main.pdf"));
    }

    #[test]
    fn without_an_output_directory_the_pdf_sits_beside_the_source() {
        let options = texres(Path::new("/work/paper/main.tex"));
        assert_eq!(options.pdf_path(), Path::new("/work/paper/main.pdf"));
    }

    #[test]
    fn the_command_line_is_readable() {
        let options = BuildOptions {
            program: "texres".to_string(),
            args: vec!["-pdf".to_string(), "my paper.tex".to_string()],
            directory: PathBuf::from("/work"),
        };
        assert_eq!(options.command_line(), "texres -pdf \"my paper.tex\"");
    }

    #[test]
    fn texres_errors_are_parsed() {
        let log = lines(
            "error: LaTeX Error: File `nope.tex' not found.\n  --> /work/main.tex:3:8\n  |\n3 | \\input{nope.tex}\n  |        ^^^^^^^^^^\npdflatex: compilation failed after 1 error",
        );
        let errors = parse_errors(&log, Path::new("/work"));
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line, 3);
        assert_eq!(errors[0].path, Path::new("/work/main.tex"));
        assert!(errors[0].message.contains("not found"));
    }

    #[test]
    fn a_relative_texres_location_is_joined_to_the_build_directory() {
        let log = lines("error: Undefined control sequence.\n  --> chapters/one.tex:7:2");
        let errors = parse_errors(&log, Path::new("/work/paper"));
        assert_eq!(errors[0].path, Path::new("/work/paper/chapters/one.tex"));
        assert_eq!(errors[0].line, 7);
    }

    #[test]
    fn file_line_errors_are_parsed() {
        let log = lines("./main.tex:12: Undefined control sequence.\nother output");
        let errors = parse_errors(&log, Path::new("/work"));
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line, 12);
        assert!(errors[0].message.contains("Undefined control sequence"));
        assert_eq!(errors[0].path.file_name().unwrap(), "main.tex");
    }

    #[test]
    fn an_absolute_path_is_kept() {
        let log = lines("/work/main.tex:3: Missing $ inserted.");
        let errors = parse_errors(&log, Path::new("/elsewhere"));
        assert_eq!(errors[0].path, Path::new("/work/main.tex"));
    }

    #[test]
    fn classic_tex_errors_get_their_line_from_below() {
        let log = lines(
            "! Undefined control sequence.\nl.42 \\foo\nSome context",
        );
        let errors = parse_errors(&log, Path::new("/work"));
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line, 42);
        assert!(errors[0].message.contains("Undefined control sequence"));
    }

    #[test]
    fn an_error_with_no_line_still_counts() {
        let log = lines("! Emergency stop.");
        let errors = parse_errors(&log, Path::new("/work"));
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line, 0);
    }

    #[test]
    fn ordinary_output_is_not_an_error() {
        let log = lines(
            "This is pdfTeX\nentering extended mode\nOutput written on main.pdf (1 page).",
        );
        assert!(parse_errors(&log, Path::new("/work")).is_empty());
    }

    #[test]
    fn several_errors_are_collected() {
        let log = lines(
            "./a.tex:1: first\n./a.tex:2: second\n./b.tex:9: third",
        );
        let errors = parse_errors(&log, Path::new("/work"));
        assert_eq!(errors.len(), 3);
        assert_eq!(errors[2].line, 9);
    }
}

/// A build against a real process.
#[cfg(test)]
mod process_tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn run(script: &str) -> Build {
        Build::start(BuildOptions {
            program: "sh".to_string(),
            args: vec!["-c".to_string(), script.to_string()],
            directory: std::env::temp_dir(),
        })
        .expect("sh should start")
    }

    fn wait(build: &mut Build) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            build.poll();
            if build.finished {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    #[test]
    fn output_streams_into_the_log() {
        let mut build = run("echo first; echo second");
        assert!(wait(&mut build), "the build should finish");
        assert!(build.success);
        assert!(build.lines.iter().any(|line| line.contains("first")));
        assert!(build.lines.iter().any(|line| line.contains("second")));
    }

    #[test]
    fn a_failing_build_is_reported_as_a_failure() {
        let mut build = run("exit 3");
        assert!(wait(&mut build));
        assert!(!build.success, "a non-zero exit is a failed build");
    }

    #[test]
    fn errors_in_the_output_are_found() {
        let mut build = run("echo './main.tex:7: Undefined control sequence.'; exit 1");
        assert!(wait(&mut build));
        let errors = build.errors();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line, 7);
    }

    #[test]
    fn the_tail_is_the_end_of_the_log() {
        let mut build = run("for i in 1 2 3 4 5; do echo line-$i; done");
        assert!(wait(&mut build));
        let tail = build.tail(2);
        assert_eq!(tail.len(), 2);
        assert!(tail[1].contains("line-5"));
    }

    #[test]
    fn cancelling_stops_the_build() {
        let mut build = run("sleep 30");
        std::thread::sleep(Duration::from_millis(50));
        build.cancel();
        assert!(build.finished);
        assert!(!build.success);
    }

    #[test]
    fn texres_builds_a_minimal_document_when_it_is_installed() {
        if std::process::Command::new("texres")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("skipping: texres is not installed");
            return;
        }

        let directory = std::env::temp_dir().join("lide-texres-build");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let tex = directory.join("main.tex");
        std::fs::write(
            &tex,
            "\\documentclass{article}\n\\begin{document}\nHello texres.\n\\end{document}\n",
        )
        .unwrap();

        let options = texres(&tex);
        assert_eq!(options.program, "texres");
        assert_eq!(options.args, ["main.tex"]);

        let mut build = Build::start(options).expect("texres should start");
        let deadline = Instant::now() + Duration::from_secs(60);
        while Instant::now() < deadline {
            build.poll();
            if build.finished {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(build.finished, "texres should finish");
        assert!(
            build.success,
            "texres failed:\n{}",
            build.lines.join("\n")
        );
        assert!(directory.join("main.pdf").exists(), "a PDF was written");

        let _ = std::fs::remove_dir_all(&directory);
    }
}
