//! Tallboy backend.
#![deny(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod apps;
pub mod atomic;
pub mod bookmarks;
pub mod complete;
pub mod dir_settings;
pub mod filemanager1;
pub mod folder;
pub mod guard;
pub mod import;
pub mod jobs;
pub mod meta;
pub mod settings_store;
pub mod terminal;
pub mod thumbs;
pub mod trace;
pub mod trash;
pub mod volumes;
