extern crate gtk;
extern crate glib;

use gtk::prelude::*;
use std::ffi::{CString, CStr};
use glib::translate::ToGlibPtr;

#[macro_export]
macro_rules! dialog_when_error {
	($message_template:expr, $e:expr) => (
		if let Err(err) = $e {
			let msg = format!($message_template, err);
			::gtk_utils::message_box_error(&msg)
		}
	)
}

pub fn message_box_error(message: &str) {
    let dialog = gtk::MessageDialog::new(None::<&gtk::Window>,
                                         gtk::DialogFlags::empty(),
                                         gtk::MessageType::Error,
                                         gtk::ButtonsType::Ok,
                                         message);
    dialog.run();
}

pub fn message_box_info(message: &str) {
    let dialog = gtk::MessageDialog::new(None::<&gtk::Window>,
                                         gtk::DialogFlags::empty(),
                                         gtk::MessageType::Info,
                                         gtk::ButtonsType::Ok,
                                         message);
    dialog.run();
}

pub fn text_buffer_insert_with_tag_by_name(buffer: &gtk::TextBuffer,
                                           iter: &mut gtk::TextIter,
                                           text: &str,
                                           tag_name: &str) {

    let start_offset = iter.get_offset();
    buffer.insert(iter, text);
    let start_iter = buffer.get_iter_at_offset(start_offset);

    buffer.apply_tag_by_name(tag_name, &start_iter, &iter);
}

// TODO: create glib_utils?
pub fn escape_markup_text(str: &str) -> String {
    let cstr = CString::new(str).unwrap();
    unsafe {
        let text_ptr = glib_sys::g_markup_escape_text(cstr.as_ptr(), -1);
        let escaped = CStr::from_ptr(text_ptr).to_str().unwrap().to_owned();
        glib_sys::g_free(text_ptr as *mut _);
        escaped
    }
}
