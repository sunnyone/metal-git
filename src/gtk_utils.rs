use gtk::prelude::*;

#[macro_export]
macro_rules! dialog_when_error {
	($message_template:expr, $e:expr) => (
		if let Err(err) = $e {
			let msg = format!($message_template, err);
			$crate::gtk_utils::message_box_error(&msg)
		}
	)
}

pub fn message_box_error(message: &str) {
    let dialog = gtk::MessageDialog::builder()
        .modal(true)
        .message_type(gtk::MessageType::Error)
        .buttons(gtk::ButtonsType::Ok)
        .text(message)
        .build();
    dialog.connect_response(|dialog, _| {
        dialog.close();
    });
    dialog.present();
}

pub fn icon_button(icon_name: &str, label: &str) -> gtk::Button {
    let content = icon_button_content(icon_name, label);
    gtk::Button::builder()
        .child(&content)
        .tooltip_text(label)
        .build()
}

fn icon_button_content(icon_name: &str, label: &str) -> gtk::Box {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .build();
    content.append(&gtk::Image::from_icon_name(icon_name));
    content.append(&gtk::Label::new(Some(label)));
    content
}

/// The CSS class of the badge on a `BadgeButton`. The window that uses one styles it.
pub const BUTTON_BADGE_CSS_CLASS: &str = "button-badge";

/// An icon button that can show a small badge after its label, like `↑3`.
pub struct BadgeButton {
    button: gtk::Button,
    badge: gtk::Label,
}

impl BadgeButton {
    /// The badge is hidden until `set_badge` gives it a text.
    pub fn new(icon_name: &str, label: &str) -> BadgeButton {
        let content = icon_button_content(icon_name, label);
        let badge = gtk::Label::builder()
            .css_classes([BUTTON_BADGE_CSS_CLASS])
            .valign(gtk::Align::Center)
            .visible(false)
            .build();
        content.append(&badge);

        let button = gtk::Button::builder()
            .child(&content)
            .tooltip_text(label)
            .build();
        BadgeButton { button, badge }
    }

    pub fn button(&self) -> &gtk::Button {
        &self.button
    }

    /// Shows the badge with the text, or hides it with `None`.
    pub fn set_badge(&self, text: Option<&str>) {
        match text {
            Some(text) => {
                self.badge.set_text(text);
                self.badge.set_visible(true);
            }
            None => self.badge.set_visible(false),
        }
    }
}

pub fn text_buffer_insert_with_tag_by_name(buffer: &gtk::TextBuffer,
                                           iter: &mut gtk::TextIter,
                                           text: &str,
                                           tag_name: &str) {

    let start_offset = iter.offset();
    buffer.insert(iter, text);
    let start_iter = buffer.iter_at_offset(start_offset);

    buffer.apply_tag_by_name(tag_name, &start_iter, iter);
}
