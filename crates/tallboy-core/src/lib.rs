//! Tallboy backend.
#![deny(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod config;
pub mod enable;
pub mod format;
pub mod glob;
pub mod helpers;
pub mod history;
pub mod invocation;
pub mod keymap;
pub mod location;
pub mod naming;
pub mod open;
pub mod perms;
pub mod sort;
pub mod transfer;
