//! tallboy — a behavioural port of PCManFM to Rust and GTK 4.
//!
//! This crate only wires widgets to `tallboy-core` (decisions) and
//! `tallboy-fs` (GIO); it holds no file-operation logic of its own.

mod actions;
mod app;
mod bookmarks;
mod cli;
mod clipboard;
mod debug;
mod dialogs;
mod filemanager1;
mod listeners;
mod logging;
mod menubar;
mod open;
mod ops;
mod settings;
mod sidepane;
mod tab;
mod theme;
mod view;
mod volumes;
mod window;

use std::ffi::OsString;

fn main() -> gtk::glib::ExitCode {
    debug::mark_start();
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    // Keep-open terminal supervisor: must run before GApplication sees argv.
    if let Some(result) = tallboy_fs::terminal::supervise_if_requested(&args) {
        match result {
            Ok(code) => std::process::exit(code),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
    }
    logging::init();
    app::run(&args)
}
