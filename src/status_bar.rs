use crate::long_running_operation::LongRunningStatus;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// A status bar showing a message, and the status of a long running operation with a cancel button.
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

    /// `callback` runs when the cancel button is clicked.
    pub fn connect_cancel<F: Fn() + 'static>(&self, callback: F) {
        self.cancel_callback.replace(Some(Box::new(callback)));
    }

    pub fn apply(&self, status: &LongRunningStatus) {
        match status {
            LongRunningStatus::Started { message } => {
                self.set_message(message);
                self.progress_bar.set_fraction(0.0);
                self.progress_bar.set_visible(true);
                self.cancel_button.set_sensitive(true);
                self.cancel_button.set_visible(true);
            }
            LongRunningStatus::Progress { message, fraction } => {
                self.set_message(message);
                match fraction {
                    Some(fraction) => self.progress_bar.set_fraction(fraction.clamp(0.0, 1.0)),
                    None => self.progress_bar.pulse(),
                }
            }
            LongRunningStatus::Succeeded { message, .. }
            | LongRunningStatus::Cancelled { message }
            | LongRunningStatus::Failed { message } => {
                self.set_message(message);
                self.progress_bar.set_visible(false);
                self.cancel_button.set_visible(false);
            }
        }
    }
}
