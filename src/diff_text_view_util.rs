use git2::Diff;
use gtk::TextBuffer;
use gtk::traits::TextBufferExt;
use crate::gtk_utils;
use std::str;

pub fn print_diff_to_text_view(diff: &Diff, buffer: &TextBuffer) {
    buffer.set_text("");

    let mut iter = buffer.start_iter();
    let _ = diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        let o = line.origin();
        let tag_name = match o {
            ' ' => "normal",
            '+' => "add",
            '-' => "delete",
            _ => "other",
        };

        let mut str = String::new();
        if o == '+' || o == '-' || o == ' ' {
            str.push(line.origin());
        }
        str.push_str(str::from_utf8(line.content()).unwrap());

        gtk_utils::text_buffer_insert_with_tag_by_name(&buffer, &mut iter, &str, tag_name);
        true
    });
}
