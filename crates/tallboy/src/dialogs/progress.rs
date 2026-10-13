//! The progress window of a file operation (PROMPT §8.10).

use gtk::{pango, prelude::*};
use tallboy_core::format;

pub struct ProgressDialog {
    window: gtk::Window,
    current: gtk::Label,
    destination: gtk::Label,
    destination_row: gtk::Box,
    bar: gtk::ProgressBar,
    counts: gtk::Label,
    pause: gtk::Button,
    stop: gtk::Button,
    errors: gtk::TextView,
    errors_area: gtk::Expander,
}

fn field(caption: &str) -> (gtk::Box, gtk::Label) {
    let name = gtk::Label::new(Some(caption));
    name.set_xalign(0.0);
    let value = gtk::Label::new(None);
    value.set_xalign(0.0);
    value.set_hexpand(true);
    value.set_ellipsize(pango::EllipsizeMode::Middle);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.append(&name);
    row.append(&value);
    (row, value)
}

impl ProgressDialog {
    /// Builds the window without showing it: a job that finishes within
    /// half a second never shows one.
    pub fn new(app: &gtk::Application, title: &str) -> Self {
        // Not transient for a main window: the operation outlives it.
        let window = gtk::Window::builder()
            .application(app)
            .title(title)
            .default_width(460)
            .resizable(false)
            .build();
        let heading = gtk::Label::new(Some(title));
        heading.set_xalign(0.0);
        heading.add_css_class("heading");
        let (current_row, current) = field("Current file:");
        let (destination_row, destination) = field("To:");
        destination_row.set_visible(false);
        let bar = gtk::ProgressBar::new();
        let counts = gtk::Label::new(Some("Preparing..."));
        counts.set_xalign(0.0);

        let errors = gtk::TextView::new();
        errors.set_editable(false);
        errors.set_cursor_visible(false);
        errors.set_wrap_mode(gtk::WrapMode::WordChar);
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_min_content_height(120);
        scroller.set_has_frame(true);
        scroller.set_child(Some(&errors));
        let errors_area = gtk::Expander::new(Some("Errors occurred:"));
        errors_area.set_child(Some(&scroller));
        errors_area.set_visible(false);

        let pause = gtk::Button::with_mnemonic("_Pause");
        let stop = gtk::Button::with_mnemonic("_Cancel");
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        buttons.set_halign(gtk::Align::End);
        buttons.append(&pause);
        buttons.append(&stop);

        let body = gtk::Box::new(gtk::Orientation::Vertical, 8);
        body.add_css_class("dialog-body");
        for widget in [
            heading.upcast_ref::<gtk::Widget>(),
            current_row.upcast_ref(),
            destination_row.upcast_ref(),
            bar.upcast_ref(),
            counts.upcast_ref(),
            errors_area.upcast_ref(),
            buttons.upcast_ref(),
        ] {
            body.append(widget);
        }
        window.set_child(Some(&body));
        Self {
            window,
            current,
            destination,
            destination_row,
            bar,
            counts,
            pause,
            stop,
            errors,
            errors_area,
        }
    }

    pub fn window(&self) -> &gtk::Window {
        &self.window
    }

    pub fn pause_button(&self) -> &gtk::Button {
        &self.pause
    }

    /// Cancel while the job runs, Close once it has finished.
    pub fn stop_button(&self) -> &gtk::Button {
        &self.stop
    }

    pub fn present(&self) {
        self.window.present();
    }

    pub fn set_current(&self, source: &str, destination: Option<&str>) {
        self.current.set_text(source);
        self.destination_row.set_visible(destination.is_some());
        if let Some(destination) = destination {
            self.destination.set_text(destination);
        }
    }

    pub fn set_paused(&self, paused: bool) {
        self.pause
            .set_label(if paused { "_Resume" } else { "_Pause" });
    }

    /// Updates the bar and the "x of y (speed, time remaining)" line.
    pub fn set_progress(&self, done: (u64, u64), total: (u64, u64), bytes_per_sec: u64, si: bool) {
        let (done_files, done_bytes) = done;
        let (total_files, total_bytes) = total;
        // By bytes when there are any; folders and links have none.
        let fraction = if total_bytes > 0 {
            done_bytes as f64 / total_bytes as f64
        } else if total_files > 0 {
            done_files as f64 / total_files as f64
        } else {
            0.0
        };
        self.bar.set_fraction(fraction.clamp(0.0, 1.0));
        let text = if total_bytes > 0 {
            format!(
                "{} of {} ({}, {} remaining)",
                format::size(done_bytes.min(total_bytes), si),
                format::size(total_bytes, si),
                format::speed(bytes_per_sec, si),
                format::remaining(total_bytes.saturating_sub(done_bytes), bytes_per_sec),
            )
        } else {
            format!("{done_files} of {total_files} items")
        };
        self.counts.set_text(&text);
    }

    pub fn add_error(&self, text: &str) {
        let buffer = self.errors.buffer();
        let mut end = buffer.end_iter();
        buffer.insert(&mut end, text);
        buffer.insert(&mut end, "\n");
        self.errors_area.set_visible(true);
        self.errors_area.set_expanded(true);
    }

    /// The job is over. With nothing to report the window closes; otherwise
    /// it stays open on its error list until the user closes it.
    pub fn finish(&self, outcome: &str, keep_open: bool) {
        if !keep_open {
            self.window.close();
            return;
        }
        self.counts.set_text(outcome);
        self.pause.set_visible(false);
        self.stop.set_label("_Close");
        self.present();
    }
}
