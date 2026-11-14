use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use super::diff_text_view::{DiffTextView, Row, RowRecorder};
use super::gutter::{Columns, LineNumberGutter};
use super::text::{self, FileSlot, Folds};
use super::SharedContent;
use crate::diff::FileDiff;

/// One column with old and new line numbers, word emphasis, and files and hunks
/// that fold when their head line is clicked.
pub struct UnifiedView {
    view: DiffTextView,
    scrolled: gtk::ScrolledWindow,
    folds: Rc<Folds>,
    gutter: LineNumberGutter,
    content: SharedContent,
    /// Indexed like the files of `content`
    slots: RefCell<Vec<FileSlot>>,
}

impl UnifiedView {
    pub fn new(content: SharedContent) -> Rc<UnifiedView> {
        let view = text::create_text_view();
        let scrolled = text::scrolled(&view);
        let folds = Folds::new(view.upcast_ref());
        let gutter = LineNumberGutter::attach(&view, Columns::Both);
        let unified = Rc::new(UnifiedView {
            view,
            scrolled,
            folds,
            gutter,
            content,
            slots: RefCell::new(Vec::new()),
        });

        let w = Rc::downgrade(&unified);
        unified.folds.connect_toggled(move |index, open| {
            if let Some(unified) = w.upgrade() {
                unified.fold_toggled(index, open);
            }
        });
        unified
    }

    pub fn widget(&self) -> gtk::Widget {
        self.scrolled.clone().upcast()
    }

    pub fn show(&self) {
        let buffer = self.view.buffer();
        self.folds.clear();
        text::forget_file_slots(&buffer, &mut self.slots.borrow_mut());
        buffer.set_text("");
        let mut iter = buffer.start_iter();
        let mut rows = RowRecorder::default();
        let mut slots = Vec::new();

        let mut content = self.content.borrow_mut();
        let content = &mut *content;
        for (file, &open) in content.files.iter_mut().zip(&content.open) {
            rows.set(&iter, Row::FileHeader);
            let mut slot = text::insert_file_head(&self.folds, &mut iter, file, open);
            if open {
                file.emphasize();
                self.insert_body(&mut iter, &mut rows, file, &slot.body);
                slot.loaded = true;
            }
            slots.push(slot);
        }

        text::apply_base_font(&buffer);
        self.view.set_rows(rows.into_rows());
        self.slots.replace(slots);
        self.gutter.refresh();
        self.scrolled.vadjustment().set_value(0.0);
    }

    /// Opens a file if it is closed and scrolls its head line to the top.
    pub fn scroll_to_file(&self, file_index: usize) {
        let Some(fold) = self.slots.borrow().get(file_index).map(|s| s.fold) else {
            return;
        };
        self.folds.set_open(fold, true);
        self.file_toggled(file_index, true);

        text::scroll_to_file_head(self.view.upcast_ref(), &self.slots.borrow()[file_index]);
    }

    fn fold_toggled(&self, fold: usize, open: bool) {
        let file_index = self.slots.borrow().iter().position(|s| s.fold == fold);
        if let Some(file_index) = file_index {
            self.file_toggled(file_index, open);
        }
    }

    fn file_toggled(&self, file_index: usize, open: bool) {
        self.content.borrow_mut().open[file_index] = open;
        if open {
            self.load(file_index);
        }
    }

    /// Inserts the lines of a file that was closed when it was shown.
    fn load(&self, file_index: usize) {
        let mut slots = self.slots.borrow_mut();
        let slot = &mut slots[file_index];
        if slot.loaded {
            return;
        }
        slot.loaded = true;

        let mut content = self.content.borrow_mut();
        let file = &mut content.files[file_index];
        file.emphasize();

        let buffer = self.view.buffer();
        let mut iter = text::file_body_iter(&buffer, slot);
        let start_line = iter.line() as usize;
        let start_offset = iter.offset();
        let mut rows = RowRecorder::default();
        self.insert_body(&mut iter, &mut rows, file, &slot.body);
        let end_line = iter.line() as usize;

        text::finish_file_body(&buffer, &buffer.iter_at_offset(start_offset), &iter);
        self.view
            .insert_rows(start_line, rows.into_rows_between(start_line, end_line));
        self.gutter.refresh();
    }

    /// Inserts the hunks of a file, all carrying the `file_body` tag.
    fn insert_body(
        &self,
        iter: &mut gtk::TextIter,
        rows: &mut RowRecorder,
        file: &FileDiff,
        file_body: &str,
    ) {
        let buffer = self.view.buffer();
        if let Some(note) = text::body_note(file) {
            text::insert(
                &buffer,
                iter,
                note,
                &[text::NOTE, text::FILE_BODY, file_body],
            );
            return;
        }

        for hunk in &file.hunks {
            rows.set(iter, Row::HunkHeader);
            let body = self.folds.insert_head(
                iter,
                &hunk.header,
                &[text::HUNK_HEADER, text::FILE_BODY, file_body],
                true,
            );

            for line in &hunk.lines {
                let (line_tag, _) = text::line_tags(line.kind);
                let mut tags = vec![text::FILE_BODY, file_body, body.as_str()];
                tags.extend(line_tag);

                rows.set(iter, Row::line(line));
                text::insert_content(&buffer, iter, line, &tags);
            }
        }
    }
}
