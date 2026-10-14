//! Command-line interpretation, resolving paths in the caller's cwd.
use crate::location::{Location, LocationError};
use std::{ffi::OsString, os::unix::ffi::OsStrExt, path::Path};
/// A command-line failure with its nonzero exit status.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct InvocationError {
    /// Human-readable failure.
    pub message: String,
    /// Process exit code.
    pub exit_code: i32,
}
impl From<LocationError> for InvocationError {
    fn from(e: LocationError) -> Self {
        Self {
            message: e.to_string(),
            exit_code: 2,
        }
    }
}
/// A request for the primary application instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Open a location; caller decides navigation or launch.
    Open {
        /// Validated location.
        location: Location,
        /// Force a new window.
        new_window: bool,
    },
    /// Preferences page (0–4).
    Preferences(u32),
    /// One-shot config import.
    Import,
}
/// Parsed startup flags and ordered requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    /// Validated profile, suitable for directory names.
    pub profile: String,
    /// Stay alive with no window on first invocation.
    pub daemon: bool,
    /// Application requests.
    pub requests: Vec<Request>,
}
fn fail(message: impl Into<String>) -> InvocationError {
    InvocationError {
        message: message.into(),
        exit_code: 2,
    }
}
/// Parse arguments excluding `argv[0]`. `--` permits filenames beginning with '-'.
/// Deferred desktop and search options return explicit errors before requests execute.
pub fn parse(
    args: &[String],
    cwd: &Path,
    home: &Path,
    first_instance: bool,
) -> Result<Invocation, InvocationError> {
    let mut profile = "default".to_owned();
    let mut daemon = false;
    let mut new_window = false;
    let mut files = Vec::new();
    let mut requests = Vec::new();
    let mut options = true;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if options && arg == "--" {
            options = false;
            i += 1;
            continue;
        }
        if !options || !arg.starts_with('-') || arg == "-" {
            files.push(arg.clone());
            i += 1;
            continue;
        }
        let (key, inline) = arg
            .split_once('=')
            .map_or((arg.as_str(), None), |(k, v)| (k, Some(v)));
        let mut value = || -> Result<String, InvocationError> {
            if let Some(v) = inline {
                Ok(v.into())
            } else {
                i += 1;
                args.get(i)
                    .cloned()
                    .ok_or_else(|| fail(format!("{key} requires a value")))
            }
        };
        match key {
            "-p" | "--profile" => {
                profile = value()?;
                if profile.is_empty()
                    || !profile
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
                {
                    return Err(fail("Profile must contain letters, digits, '_' or '-'."));
                }
            }
            "-d" | "--daemon-mode" => daemon = true,
            "-n" | "--new-win" => new_window = true,
            "--no-desktop" => {}
            "--role" => {
                value()?;
            }
            "--show-pref" => {
                let n = value()?
                    .parse::<u32>()
                    .ok()
                    .filter(|n| (1..=5).contains(n))
                    .ok_or_else(|| fail("Preferences page must be 1–5."))?;
                requests.push(Request::Preferences(n));
            }
            "--import-pcmanfm" => requests.push(Request::Import),
            "-f" | "--find-files" => {
                return Err(fail("Find Files is not supported in this version."));
            }
            "--desktop" | "--desktop-off" | "--desktop-pref" | "--one-screen" | "-w"
            | "--set-wallpaper" | "--wallpaper-mode" => {
                return Err(fail("The desktop manager is not built in this version."));
            }
            _ => return Err(fail(format!("Unknown option: {key}"))),
        }
        i += 1;
    }
    for input in files {
        requests.push(Request::Open {
            location: Location::parse(&input, cwd, home)?,
            new_window,
        });
    }
    if requests.is_empty() && !(daemon && first_instance) {
        requests.push(Request::Open {
            location: Location::from_path(cwd)?,
            new_window: true,
        });
    }
    Ok(Invocation {
        profile,
        daemon,
        requests,
    })
}
/// Parse process/GApplication arguments without assuming native filenames are UTF-8.
/// Non-UTF-8 native paths become escaped file URIs before the regular option parser.
/// Use this at the CLI boundary; `parse` remains convenient for text-only inputs.
pub fn parse_os(
    args: &[OsString],
    cwd: &Path,
    home: &Path,
    first_instance: bool,
) -> Result<Invocation, InvocationError> {
    let mut options = true;
    let mut text = Vec::with_capacity(args.len());
    for arg in args {
        if let Some(arg) = arg.to_str() {
            if arg == "--" {
                options = false;
            }
            text.push(arg.to_owned());
        } else {
            let bytes = arg.as_os_str().as_bytes();
            if options && bytes.starts_with(b"-") {
                return Err(fail(
                    "An option name must be UTF-8; use -- before a filename beginning with '-'.",
                ));
            }
            let path = if let Some(suffix) = bytes.strip_prefix(b"~/") {
                home.join(std::ffi::OsStr::from_bytes(suffix))
            } else {
                let path = Path::new(arg);
                if path.is_absolute() {
                    path.to_owned()
                } else {
                    cwd.join(path)
                }
            };
            text.push(Location::from_path(&path)?.to_uri().to_owned());
        }
    }
    parse(&text, cwd, home, first_instance)
}
/// Profile-specific GApplication id. Invalid profiles are rejected by parse.
pub fn application_id(profile: &str) -> String {
    if profile == "default" {
        "io.github.codedsleep.Trek".into()
    } else {
        format!(
            "io.github.codedsleep.Trek.p_{}",
            profile.replace('_', "_5f").replace('-', "_2d")
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn run(args: &[&str]) -> Result<Invocation, InvocationError> {
        parse(
            &args.iter().map(|s| (*s).into()).collect::<Vec<_>>(),
            Path::new("/caller"),
            Path::new("/home/u"),
            true,
        )
    }
    #[test]
    fn caller_paths() {
        let r = run(&["-n", "relative", "~"]).unwrap();
        assert_eq!(
            r.requests,
            vec![
                Request::Open {
                    location: Location::Uri("file:///caller/relative".into()),
                    new_window: true
                },
                Request::Open {
                    location: Location::Uri("file:///home/u".into()),
                    new_window: true
                }
            ]
        );
        assert!(run(&["-d"]).unwrap().requests.is_empty());
        assert_eq!(run(&["--", "-file"]).unwrap().requests.len(), 1);
    }
    #[test]
    fn raw_process_filenames() {
        use std::os::unix::ffi::OsStringExt;
        let args = [
            OsString::from_vec(b"file-\xff".to_vec()),
            OsString::from("--"),
            OsString::from_vec(b"-file-\xfe".to_vec()),
            OsString::from_vec(b"~/file-\xfd".to_vec()),
        ];
        let parsed = parse_os(&args, Path::new("/caller"), Path::new("/home/u"), true).unwrap();
        let expected = [
            b"/caller/file-\xff".to_vec(),
            b"/caller/-file-\xfe".to_vec(),
            b"/home/u/file-\xfd".to_vec(),
        ];
        for (request, expected) in parsed.requests.iter().zip(expected) {
            let Request::Open { location, .. } = request else {
                panic!("Expected Open");
            };
            assert_eq!(location.path().unwrap().as_os_str().as_bytes(), expected);
        }
        assert!(parse_os(&[args[2].clone()], Path::new("/"), Path::new("/"), true).is_err());
    }
    #[test]
    fn deferred() {
        for option in [
            "--desktop",
            "--desktop-off",
            "--desktop-pref",
            "--one-screen",
            "--set-wallpaper=x",
            "--wallpaper-mode=fit",
            "-w",
        ] {
            let e = run(&[option]).unwrap_err();
            assert_eq!(
                e.message,
                "The desktop manager is not built in this version."
            );
            assert_ne!(e.exit_code, 0);
        }
        assert!(
            run(&["--find-files"])
                .unwrap_err()
                .message
                .contains("not supported")
        );
        assert!(run(&["--profile=../x"]).is_err());
        assert!(run(&["--show-pref=0"]).is_err());
        assert!(run(&["--show-pref=6"]).is_err());
    }
    #[test]
    fn preferences_pages_are_one_based() {
        // Upstream and PROMPT §8.25: page 1 is General, page 5 Advanced.
        for page in 1..=5 {
            let invocation = run(&[&format!("--show-pref={page}")]).unwrap();
            assert_eq!(invocation.requests, [Request::Preferences(page)]);
        }
    }
    #[test]
    fn options() {
        assert_eq!(
            run(&["--profile=x", "--show-pref=2", "--import-pcmanfm"])
                .unwrap()
                .requests,
            vec![Request::Preferences(2), Request::Import]
        );
        assert!(run(&["--profile"]).is_err());
        assert_ne!(application_id("default"), application_id("x"));
    }
}
