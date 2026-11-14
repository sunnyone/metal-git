use gtk::prelude::*;
use std::rc::Rc;

use super::text::{self, Folds};
use crate::diff_model::{FileDiff, LineKind};

/// One column with old and new line numbers, word emphasis, and hunks that
/// fold when their header is clicked.
pub struct UnifiedView {
    view: gtk::TextView,
    scrolled: gtk::ScrolledWindow,
    folds: Rc<Folds>,
}

impl UnifiedView {
    pub fn new() -> UnifiedView {
        let view = text::create_text_view();
        let scrolled = text::scrolled(&view);
        let folds = Folds::new(&view);
        UnifiedView {
            view,
            scrolled,
            folds,
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.scrolled.clone().upcast()
    }

    pub fn show(&self, files: &[FileDiff]) {
        let buffer = self.view.buffer();
        self.folds.clear();
        buffer.set_text("");
        let mut iter = buffer.start_iter();

        for file in files {
            text::insert(
                &buffer,
                &mut iter,
                &text::file_header_text(file),
                &[text::FILE_HEADER],
            );
            if file.binary {
                text::insert(&buffer, &mut iter, "      Binary file\n", &[text::NOTE]);
                continue;
            }

            for hunk in &file.hunks {
                let body =
                    self.folds
                        .insert_head(&mut iter, &hunk.header, &[text::HUNK_HEADER], true);

                for line in &hunk.lines {
                    let (line_tag, _) = text::line_tags(line.kind);
                    let mut tags = vec![body.as_str()];
                    tags.extend(line_tag);

                    let mut gutter_tags = tags.clone();
                    gutter_tags.push(text::LINENO);
                    let sign = match line.kind {
                        LineKind::Added => "+ ",
                        LineKind::Removed => "- ",
                        LineKind::Context => "  ",
                    };
                    let gutter = format!(
                        "{}{}{}",
                        text::lineno(line.old_lineno),
                        text::lineno(line.new_lineno),
                        sign
                    );
                    text::insert(&buffer, &mut iter, &gutter, &gutter_tags);
                    text::insert_content(&buffer, &mut iter, line, &tags);
                }
            }
        }

        text::apply_base_font(&buffer);
        self.scrolled.vadjustment().set_value(0.0);
    }
}
