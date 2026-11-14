use gtk::prelude::*;
use std::rc::Rc;

use super::diff_text_view::{DiffTextView, Row, RowRecorder};
use super::gutter::{Columns, LineNumberGutter};
use super::text::{self, Folds};
use crate::diff_model::FileDiff;

/// One column with old and new line numbers, word emphasis, and hunks that
/// fold when their header is clicked.
pub struct UnifiedView {
    view: DiffTextView,
    scrolled: gtk::ScrolledWindow,
    folds: Rc<Folds>,
    gutter: LineNumberGutter,
}

impl UnifiedView {
    pub fn new() -> UnifiedView {
        let view = text::create_text_view();
        let scrolled = text::scrolled(&view);
        let folds = Folds::new(view.upcast_ref());
        let gutter = LineNumberGutter::attach(&view, Columns::Both);
        UnifiedView {
            view,
            scrolled,
            folds,
            gutter,
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
        let mut rows = RowRecorder::default();

        for file in files {
            rows.set(&iter, Row::FileHeader);
            text::insert(
                &buffer,
                &mut iter,
                &text::file_header_text(file),
                &[text::FILE_HEADER],
            );
            if file.binary {
                text::insert(&buffer, &mut iter, "Binary file\n", &[text::NOTE]);
                continue;
            }

            for hunk in &file.hunks {
                rows.set(&iter, Row::HunkHeader);
                let body =
                    self.folds
                        .insert_head(&mut iter, &hunk.header, &[text::HUNK_HEADER], true);

                for line in &hunk.lines {
                    let (line_tag, _) = text::line_tags(line.kind);
                    let mut tags = vec![body.as_str()];
                    tags.extend(line_tag);

                    rows.set(&iter, Row::line(line));
                    text::insert_content(&buffer, &mut iter, line, &tags);
                }
            }
        }

        text::apply_base_font(&buffer);
        self.view.set_rows(rows.into_rows());
        self.gutter.refresh();
        self.scrolled.vadjustment().set_value(0.0);
    }
}
