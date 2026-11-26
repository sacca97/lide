//! Project tasks.
//!
//! A task is a command the project already knows how to run — `cargo test`,
//! `make`, `cmake --build` — discovered from the files in the project rather
//! than configured. Nothing here invents a task the project cannot run: if
//! there is no `Cargo.toml`, there is no `cargo test`.
//!
//! The runner is `lide_latex::Build`, which is a general "program, arguments,
//! directory, stream the output" runner that happens to have been written for
//! LaTeX first. Its name is the only LaTeX thing about it.

use std::path::{Path, PathBuf};

/// A command the project can run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    /// What the palette shows.
    pub label: String,
    /// The program.
    pub program: String,
    /// Its arguments.
    pub args: Vec<String>,
}

impl Task {
    /// The runner options for this task.
    pub fn options(&self, root: &Path) -> lide_latex::BuildOptions {
        lide_latex::BuildOptions {
            program: self.program.clone(),
            args: self.args.clone(),
            directory: root.to_path_buf(),
        }
    }
}

/// The tasks a project offers.
pub fn discover(root: &Path) -> Vec<Task> {
    let mut tasks = Vec::new();

    if root.join("Cargo.toml").exists() {
        tasks.push(task("cargo build", "cargo", &["build"]));
        tasks.push(task("cargo test", "cargo", &["test"]));
        tasks.push(task("cargo run", "cargo", &["run"]));
    }

    if root.join("package.json").exists() {
        // Only `test`, which every Node project has; the scripts in
        // `package.json` are the project's own business.
        tasks.push(task("npm test", "npm", &["test"]));
    }

    if root.join("Makefile").exists() || root.join("makefile").exists() {
        tasks.push(task("make", "make", &[]));
    }

    if root.join("CMakeLists.txt").exists() {
        tasks.push(task("cmake --build build", "cmake", &["--build", "build"]));
    }

    if root.join("pyproject.toml").exists() || root.join("setup.py").exists() {
        tasks.push(task("pytest", "pytest", &[]));
    }

    if root.join("go.mod").exists() {
        tasks.push(task("go test ./...", "go", &["test", "./..."]));
    }

    tasks
}

/// The task that builds a LaTeX project, when there is one.
pub fn latex_task(root: &Path, engine: lide_latex::Engine) -> Option<Task> {
    let mut found = Vec::new();
    collect_tex(root, &mut found, 0);
    let tex = found
        .iter()
        .find(|path| {
            path.file_name()
                .map(|name| name == "main.tex")
                .unwrap_or(false)
        })
        .or_else(|| found.first())?;
    let options = lide_latex::texres_with(tex, engine);
    Some(Task {
        label: format!("{} {}", options.program, tex.file_name()?.to_string_lossy()),
        program: options.program,
        args: options.args,
    })
}

fn collect_tex(directory: &Path, found: &mut Vec<PathBuf>, depth: usize) {
    if depth > 3 || found.len() > 32 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == "target" || name == "node_modules" {
            continue;
        }
        if path.is_dir() {
            collect_tex(&path, found, depth + 1);
        } else if path
            .extension()
            .map(|extension| extension == "tex")
            .unwrap_or(false)
        {
            found.push(path);
        }
    }
}

fn task(label: &str, program: &str, args: &[&str]) -> Task {
    Task {
        label: label.to_string(),
        program: program.to_string(),
        args: args.iter().map(|arg| arg.to_string()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("lide-tasks-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn a_cargo_project_gets_its_commands() {
        let directory = scratch("cargo");
        std::fs::write(directory.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();

        let tasks = discover(&directory);
        let labels: Vec<&str> = tasks.iter().map(|task| task.label.as_str()).collect();
        assert!(labels.contains(&"cargo build"));
        assert!(labels.contains(&"cargo test"));
        assert!(labels.contains(&"cargo run"));

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_plain_folder_offers_nothing() {
        let directory = scratch("empty");
        assert!(discover(&directory).is_empty());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_makefile_offers_make() {
        let directory = scratch("make");
        std::fs::write(directory.join("Makefile"), "all:\n\techo hi\n").unwrap();
        let tasks = discover(&directory);
        assert!(tasks.iter().any(|task| task.program == "make"));
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn several_ecosystems_coexist() {
        let directory = scratch("mixed");
        std::fs::write(directory.join("Cargo.toml"), "").unwrap();
        std::fs::write(directory.join("package.json"), "{}").unwrap();
        let tasks = discover(&directory);
        assert!(tasks.iter().any(|task| task.program == "cargo"));
        assert!(tasks.iter().any(|task| task.program == "npm"));
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_task_carries_the_root_it_runs_in() {
        let directory = scratch("root");
        std::fs::write(directory.join("Cargo.toml"), "").unwrap();
        let tasks = discover(&directory);
        let options = tasks[0].options(&directory);
        assert_eq!(options.directory, directory);
        assert_eq!(options.program, "cargo");
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_latex_project_gets_a_build_task() {
        let directory = scratch("latex");
        std::fs::write(directory.join("main.tex"), "\\documentclass{article}\n").unwrap();
        let task = latex_task(&directory, lide_latex::Engine::Auto).expect("a task");
        assert_eq!(task.program, "texres");
        assert_eq!(task.args, ["main.tex"]);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn the_latex_task_follows_the_chosen_engine() {
        let directory = scratch("latex-engine");
        std::fs::write(directory.join("main.tex"), "\\documentclass{article}\n").unwrap();

        let task = latex_task(&directory, lide_latex::Engine::XeLatex).expect("a task");
        assert!(task.args.contains(&"-xelatex".to_string()));
        assert!(!task.args.contains(&"-pdf".to_string()));

        let _ = std::fs::remove_dir_all(&directory);
    }
}
