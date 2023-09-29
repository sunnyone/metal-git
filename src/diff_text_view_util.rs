use git2::Diff;
use gtk::TextBuffer;
use gtk::traits::{TextBufferExt, TextTagTableExt};
use crate::gtk_utils;
use std::str;

pub fn create_diff_text_buffer() -> gtk::TextBuffer {
    let tag_table = gtk::TextTagTable::new();

    let add_tag = gtk::TextTag::builder()
        .name("add")
        .background("#ceead0")
        .foreground("black")
        .font("Normal")
        .build();
    tag_table.add(&add_tag);

    let delete_tag = gtk::TextTag::builder()
        .name("delete")
        .background("#f2c6c6")
        .foreground("black")
        .font("Normal")
        .build();
    tag_table.add(&delete_tag);

    let normal_tag = gtk::TextTag::builder()
        .name("normal")
        .background("white")
        .foreground("black")
        .font("Normal")
        .build();
    tag_table.add(&normal_tag);

    let other_tag = gtk::TextTag::builder()
        .name("other")
        .background("#e6e6e6")
        .foreground("black")
        .font("Normal")
        .build();
    tag_table.add(&other_tag);

    gtk::TextBuffer::builder()
        .tag_table(&tag_table)
        .build()
}

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
