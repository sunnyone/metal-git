use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use super::diff_text_view::{DiffTextView, Row, RowRecorder};
use super::gutter::{Columns, LineNumberGutter};
use super::text::{self, FileSlot, Folds, Links, RevealFileGap};
use super::SharedContent;
use crate::diff::FileDiff;

/// One column with old and new line numbers, word emphasis, and files and hunks
/// that fold when their head line is clicked.
pub struct UnifiedView {
    view: DiffTextView,
    scrolled: gtk::ScrolledWindow,
    folds: Rc<Folds>,
    links: Rc<Links>,
    gutter: LineNumberGutter,
    content: SharedContent,
    /// Indexed like the files of `content`
    slots: RefCell<Vec<FileSlot>>,
    reveal: RefCell<Option<RevealFileGap>>,
}

impl UnifiedView {
    pub fn new(content: SharedContent) -> Rc<UnifiedView> {
        let view = text::create_text_view();
        let scrolled = text::scrolled(&view);
        let folds = Folds::new(view.upcast_ref());
        let links = Links::new(view.upcast_ref());
        let gutter = LineNumberGutter::attach(&view, Columns::Both);
        let unified = Rc::new(UnifiedView {
            view,
            scrolled,
            folds,
            links,
            gutter,
            content,
            slots: RefCell::new(Vec::new()),
            reveal: RefCell::new(None),
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

    /// Sets what reveals unchanged lines when a link of a gap is clicked. The
    /// caller changes the file and draws it again with `reload_file`.
    pub fn connect_reveal(&self, reveal: RevealFileGap) {
        self.reveal.replace(Some(reveal));
    }

    pub fn show(&self) {
        let buffer = self.view.buffer();
        self.folds.clear();
        self.links.clear();
        text::forget_file_slots(&buffer, &mut self.slots.borrow_mut());
        buffer.set_text("");
        let mut iter = buffer.start_iter();
        let mut rows = RowRecorder::default();
        let mut slots = Vec::new();

        let mut content = self.content.borrow_mut();
        let content = &mut *content;
        for (file_index, (file, &open)) in content.files.iter_mut().zip(&content.open).enumerate() {
            rows.set(&iter, Row::FileHeader);
            let mut slot = text::insert_file_head(&self.folds, &mut iter, file, open);
            if open {
                file.emphasize();
                file.highlight();
                self.insert_body(&mut iter, &mut rows, file_index, file, &slot.body);
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

    /// Draws the lines of a file again, after they changed, keeping the lines
    /// above in place.
    pub fn reload_file(&self, file_index: usize) {
        let adjustment = self.scrolled.vadjustment();
        let value = adjustment.value();
        let buffer = self.view.buffer();
        let deleted = {
            let mut slots = self.slots.borrow_mut();
            match slots.get(file_index) {
                Some(slot) if slot.loaded => {}
                _ => return,
            }
            let lines =
                text::delete_file_body(&buffer, &slots[file_index], slots.get(file_index + 1));
            slots[file_index].loaded = false;
            lines
        };
        self.view.remove_rows(deleted);
        self.links.forget_deleted();
        self.load(file_index);
        text::restore_scroll(&adjustment, value);
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
        file.highlight();

        let buffer = self.view.buffer();
        let mut iter = text::file_body_iter(&buffer, slot);
        let start_line = iter.line() as usize;
        let start_offset = iter.offset();
        let mut rows = RowRecorder::default();
        self.insert_body(&mut iter, &mut rows, file_index, file, &slot.body);
        let end_line = iter.line() as usize;

        text::finish_file_body(&buffer, &buffer.iter_at_offset(start_offset), &iter);
        self.view
            .insert_rows(start_line, rows.into_rows_between(start_line, end_line));
        self.gutter.refresh();
    }

    /// Inserts the hunks of a file, all carrying the `file_body` tag, with the
    /// gaps between them.
    fn insert_body(
        &self,
        iter: &mut gtk::TextIter,
        rows: &mut RowRecorder,
        file_index: usize,
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

        let reveal = self
            .reveal
            .borrow()
            .as_ref()
            .map(|reveal| text::reveal_gap_of(reveal, file_index));
        let gaps = file.gaps();
        let insert_gap = |iter: &mut gtk::TextIter, rows: &mut RowRecorder, before_hunk| {
            let gap = gaps.iter().find(|gap| gap.before_hunk == before_hunk);
            if let (Some(gap), Some(reveal)) = (gap, &reveal) {
                rows.set(iter, Row::Gap);
                text::insert_gap(
                    &self.links,
                    iter,
                    gap,
                    file.hunks.len(),
                    &[text::FILE_BODY, file_body],
                    reveal,
                );
            }
        };

        for (hunk_index, hunk) in file.hunks.iter().enumerate() {
            insert_gap(iter, rows, hunk_index);
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
        insert_gap(iter, rows, file.hunks.len());
    }
}
