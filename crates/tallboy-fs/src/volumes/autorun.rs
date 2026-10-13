//! Automount plans and remembered autorun choices; execution remains explicit.
use super::Entry;
use gio::prelude::*;
use tallboy_core::config::{AutorunChoice, Config};
/// Volume ids eligible for startup or newly-added-volume automatic mounting.
/// Internal disks and volumes that the backend excludes from automount are untouched.
pub fn automount_plan(entries: &[Entry], config: &Config, startup: bool) -> Vec<String> {
    entries
        .iter()
        .filter(|e| {
            !e.mounted
                && !e.internal
                && e.should_automount
                && e.can_mount
                && (if startup {
                    config.mount_on_startup
                } else {
                    config.mount_removable && e.removable
                })
        })
        .map(|e| e.id.clone())
        .collect()
}
/// Probe mounted-media content types asynchronously.
pub async fn content_types(mount: &gio::Mount) -> Result<Vec<String>, glib::Error> {
    Ok(mount
        .guess_content_type_future(false)
        .await?
        .iter()
        .map(ToString::to_string)
        .collect())
}
/// Applications registered to handle a media content type.
pub fn applications(content_type: &str) -> Vec<gio::AppInfo> {
    gio::AppInfo::all_for_type(content_type)
}
/// Retrieve a choice while enforcing confirmation for Unix software media.
pub fn choice(config: &Config, content_type: &str) -> Option<AutorunChoice> {
    config
        .autorun_choices
        .get(content_type)
        .map(|c| AutorunChoice::parse(content_type, &c.encode(content_type)))
}
/// Save a choice; Unix software can never be remembered without confirmation.
pub fn remember(config: &mut Config, content_type: &str, choice: AutorunChoice) {
    config.autorun_choices.insert(
        content_type.into(),
        AutorunChoice::parse(content_type, &choice.encode(content_type)),
    );
}
/// Whether an autorun prompt is enabled, after respecting the global toggle.
pub fn prompt_enabled(config: &Config) -> bool {
    config.autorun
}
