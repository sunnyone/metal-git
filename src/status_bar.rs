use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// A status bar showing a message, and the progress of a running task with a cancel button.
pub struct StatusBar {
    container: gtk::Box,
    message_label: gtk::Label,
    progress_bar: gtk::ProgressBar,
    cancel_button: gtk::Button,
    cancel_callback: RefCell<Option<Box<dyn Fn()>>>,
}

impl StatusBar {
    pub fn new() -> Rc<StatusBar> {
        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();

        let message_label = gtk::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        container.append(&message_label);

        let progress_bar = gtk::ProgressBar::builder()
            .valign(gtk::Align::Center)
            .width_request(160)
            .visible(false)
            .build();
        container.append(&progress_bar);

        let cancel_button = gtk::Button::builder()
            .label("Cancel")
            .visible(false)
            .build();
        container.append(&cancel_button);

        let status_bar = Rc::new(StatusBar {
            container,
            message_label,
            progress_bar,
            cancel_button,
            cancel_callback: RefCell::new(None),
        });

        let s = Rc::downgrade(&status_bar);
        status_bar.cancel_button.connect_clicked(move |button| {
            let Some(s) = s.upgrade() else { return };
            let callback = s.cancel_callback.borrow();
            if let Some(callback) = callback.as_ref() {
                button.set_sensitive(false);
                callback();
            }
        });

        status_bar
    }

    pub fn widget(&self) -> gtk::Box {
        self.container.clone()
    }

    pub fn set_message(&self, message: &str) {
        self.message_label.set_text(message);
    }

    /// Shows the progress bar. If `on_cancel` is given, a cancel button is also shown.
    pub fn start_task(&self, message: &str, on_cancel: Option<Box<dyn Fn()>>) {
        self.set_message(message);
        self.progress_bar.set_fraction(0.0);
        self.progress_bar.set_visible(true);
        self.cancel_button.set_sensitive(true);
        self.cancel_button.set_visible(on_cancel.is_some());
        self.cancel_callback.replace(on_cancel);
    }

    /// Updates the running task. `None` for `fraction` means the total is unknown.
    pub fn update_task(&self, message: &str, fraction: Option<f64>) {
        self.set_message(message);
        match fraction {
            Some(fraction) => self.progress_bar.set_fraction(fraction.clamp(0.0, 1.0)),
            None => self.progress_bar.pulse(),
        }
    }

    pub fn finish_task(&self, message: &str) {
        self.set_message(message);
        self.progress_bar.set_visible(false);
        self.cancel_button.set_visible(false);
        self.cancel_callback.replace(None);
    }
}
