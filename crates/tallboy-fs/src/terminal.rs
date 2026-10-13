//! Terminal selection and argv-only command execution.
use std::{
    ffi::{OsStr, OsString},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, Command},
};
/// A terminal's executable and flags; no shell snippets are accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Program name.
    pub program: String,
    /// Arguments before the command's argv.
    pub exec_args: Vec<OsString>,
    /// Hold-open option, if supported.
    pub hold_args: Vec<OsString>,
}
/// Execution failure.
#[derive(Debug, thiserror::Error)]
pub enum TerminalError {
    /// Tokenization failed.
    #[error("Command syntax: {0}")]
    Parse(#[from] glib::Error),
    /// Program could not start.
    #[error("Command execution: {0}")]
    Io(#[from] std::io::Error),
    /// No command.
    #[error("A command is required")]
    Empty,
    /// None installed.
    #[error("No terminal emulator found; configure one in Preferences → Advanced.")]
    Missing,
    /// Remote location has no cwd.
    #[error("This location has no native directory for a terminal")]
    Remote,
    /// Hold-open cannot be implemented without terminal support.
    #[error("This terminal does not support keeping its window open")]
    NoHold,
    /// Invalid terminal list.
    #[error("Invalid terminal list line: {0}")]
    List(String),
}
/// Shipped defaults; Goal B may load the same format from data/terminals.list.
pub const DEFAULT_LIST: &str = "foot|-e|--hold\nkitty|-e|--hold\nalacritty|-e|--hold\nwezterm|start --|\nghostty|-e|--wait-after-command\nptyxis|--|\ngnome-terminal|--|\nkonsole|-e|--hold\nxfce4-terminal|-x|--hold\nlxterminal|-e|\nxterm|-e|-hold\n";
/// Parse `program|exec argv|hold argv`, ignoring empty lines and comments.
pub fn parse_list(text: &str) -> Result<Vec<Entry>, TerminalError> {
    let mut entries = vec![];
    for line in text
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
    {
        let parts = line.split('|').collect::<Vec<_>>();
        if parts.len() != 3 || parts[0].contains([' ', '/', '\0']) {
            return Err(TerminalError::List(line.into()));
        }
        let parse = |s: &str| {
            if s.is_empty() {
                Ok(vec![])
            } else {
                glib::shell_parse_argv(s).map(|v| v.into_iter().collect())
            }
        };
        entries.push(Entry {
            program: parts[0].into(),
            exec_args: parse(parts[1])?,
            hold_args: parse(parts[2])?,
        });
    }
    Ok(entries)
}
/// True when a program resolves to an executable in PATH (or an absolute path).
pub fn installed(program: &OsStr) -> bool {
    let p = Path::new(program);
    let executable = |p: PathBuf| {
        std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    };
    if p.components().count() > 1 {
        return executable(p.into());
    }
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| executable(dir.join(program))))
}
/// Selected terminal argv plus its known execution/hold flags.
#[derive(Debug, Clone)]
pub struct Terminal {
    /// Configured terminal command including user flags.
    pub argv: Vec<OsString>,
    /// Arguments preceding a command.
    pub exec_args: Vec<OsString>,
    /// Hold-open arguments.
    pub hold_args: Vec<OsString>,
}
/// Resolve setting, then TERMINAL, then first installed known terminal.
/// A configured missing executable yields Missing rather than silently choosing another.
pub fn resolve(
    setting: &str,
    environment: Option<&str>,
    entries: &[Entry],
    is_installed: impl Fn(&OsStr) -> bool,
) -> Result<Terminal, TerminalError> {
    let command = if !setting.trim().is_empty() {
        Some(setting)
    } else {
        environment.filter(|s| !s.trim().is_empty())
    };
    let argv: Vec<OsString> = if let Some(c) = command {
        glib::shell_parse_argv(c)?.into_iter().collect()
    } else {
        let entry = entries
            .iter()
            .find(|e| is_installed(OsStr::new(&e.program)))
            .ok_or(TerminalError::Missing)?;
        vec![entry.program.clone().into()]
    };
    let first = argv.first().ok_or(TerminalError::Empty)?;
    if !is_installed(first) {
        return Err(TerminalError::Missing);
    }
    let name = Path::new(first).file_name().unwrap_or(first);
    let entry = entries.iter().find(|e| OsStr::new(&e.program) == name);
    Ok(Terminal {
        argv,
        exec_args: entry.map_or_else(|| vec!["-e".into()], |e| e.exec_args.clone()),
        hold_args: entry.map_or(vec![], |e| e.hold_args.clone()),
    })
}
impl Terminal {
    /// Construct terminal argv for a command; paths are appended as individual arguments.
    pub fn command_argv(
        &self,
        command: &[OsString],
        hold: bool,
    ) -> Result<Vec<OsString>, TerminalError> {
        if command.is_empty() {
            return Err(TerminalError::Empty);
        }
        let mut argv = self.argv.clone();
        if hold {
            argv.extend(self.hold_args.clone());
        }
        argv.extend(self.exec_args.clone());
        if hold && self.hold_args.is_empty() {
            argv.push(std::env::current_exe()?.into_os_string());
            argv.push("--tallboy-hold-command".into());
        }
        argv.extend_from_slice(command);
        Ok(argv)
    }
    /// Open a terminal at a native location without running a command.
    pub fn open(&self, directory: &gio::File) -> Result<Child, TerminalError> {
        spawn(&self.argv, directory)
    }
    /// Execute argv in this terminal, optionally keeping it open.
    pub fn execute(
        &self,
        command: &[OsString],
        directory: &gio::File,
        hold: bool,
    ) -> Result<Child, TerminalError> {
        spawn(&self.command_argv(command, hold)?, directory)
    }
}
/// Run an argument vector in a native directory; never invokes a shell.
pub fn spawn(argv: &[OsString], directory: &gio::File) -> Result<Child, TerminalError> {
    use gio::prelude::*;
    let cwd = directory.path().ok_or(TerminalError::Remote)?;
    let program = argv.first().ok_or(TerminalError::Empty)?;
    Ok(Command::new(program)
        .args(&argv[1..])
        .current_dir(cwd)
        .spawn()?)
}
/// Run a user's tokenized command, directly or in a selected terminal.
pub fn run(
    command: &str,
    directory: &gio::File,
    terminal: Option<&Terminal>,
    hold: bool,
) -> Result<Child, TerminalError> {
    let argv: Vec<OsString> = glib::shell_parse_argv(command)?.into_iter().collect();
    match terminal {
        Some(t) => t.execute(&argv, directory, hold),
        None => spawn(&argv, directory),
    }
}

/// Hidden single-binary command supervisor. Main must call this before GApplication.
/// Executes argv directly, waits for it, then waits for Enter to close the terminal.
/// Returns None for ordinary invocations; this avoids a shell for terminals without --hold.
pub fn supervise_if_requested(args: &[OsString]) -> Option<Result<i32, TerminalError>> {
    if args.first().map(OsString::as_os_str) != Some(OsStr::new("--tallboy-hold-command")) {
        return None;
    }
    Some((|| {
        let program = args.get(1).ok_or(TerminalError::Empty)?;
        let status = Command::new(program).args(&args[2..]).status()?;
        eprintln!("Command finished. Press Enter to close this terminal.");
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        Ok(status.code().unwrap_or(1))
    })())
}
