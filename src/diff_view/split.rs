use gtk::pango;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use super::diff_text_view::{DiffTextView, Row, RowRecorder};
use super::gutter::{Columns, LineNumberGutter};
use super::text::{self, FileSlot, Folds, Links, RevealFileGap, RevealGap, ToggleViewed};
use super::SharedContent;
use crate::diff::{DiffLine, FileDiff, Gap};

/// One side of a split view.
struct Side {
    view: DiffTextView,
    scrolled: gtk::ScrolledWindow,
    folds: Rc<Folds>,
    links: Rc<Links>,
    gutter: LineNumberGutter,
}

impl Side {
    fn new(columns: Columns, vadjustment: Option<&gtk::Adjustment>) -> Side {
        let view = text::create_text_view();
        let scrolled = text::scrolled(&view);
        // Sharing the adjustment before the gutter is attached, which follows it
        if let Some(vadjustment) = vadjustment {
            scrolled.set_vadjustment(Some(vadjustment));
        }
        let folds = Folds::new(view.upcast_ref());
        let links = Links::new(view.upcast_ref());
        let gutter = LineNumberGutter::attach(&view, columns);
        Side {
            view,
            scrolled,
            folds,
            links,
            gutter,
        }
    }

    fn clear(&self) {
        self.folds.clear();
        self.links.clear();
        self.view.buffer().set_text("");
    }

    /// Inserts a line, or a filler for a line that only the other side has,
    /// and returns the line of the buffer it starts on.
    fn insert_line(
        &self,
        iter: &mut gtk::TextIter,
        rows: &mut RowRecorder,
        line: Option<&DiffLine>,
        bodies: &[&str],
    ) -> i32 {
        let buffer = self.view.buffer();
        let start = iter.line();
        let Some(line) = line else {
            rows.set(iter, Row::Filler);
            text::insert(&buffer, iter, "\n", bodies);
            return start;
        };

        let (line_tag, _) = text::line_tags(line.kind);
        let mut tags = bodies.to_vec();
        tags.extend(line_tag);
        rows.set(iter, Row::line(line));
        text::insert_content(&buffer, iter, line, &tags);
        start
    }

    /// Inserts the line that stands for a gap.
    fn insert_gap(
        &self,
        iter: &mut gtk::TextIter,
        rows: &mut RowRecorder,
        gap: &Gap,
        hunk_count: usize,
        file_body: &str,
        reveal: &RevealGap,
    ) {
        rows.set(iter, Row::Gap);
        text::insert_gap(
            &self.links,
            iter,
            gap,
            hunk_count,
            &[text::FILE_BODY, file_body],
            reveal,
        );
    }

    /// Deletes the lines of a file, so that they can be inserted again.
    fn delete_file_body(&self, slot: &FileSlot, next: Option<&FileSlot>) {
        let deleted = text::delete_file_body(&self.view.buffer(), slot, next);
        self.view.remove_rows(deleted);
        self.links.forget_deleted();
    }

    /// Adds space below a line of the buffer.
    fn pad_line(&self, line: i32, pixels: i32) {
        let buffer = self.view.buffer();
        let name = format!("pad-{}", pixels);
        let table = buffer.tag_table();
        if table.lookup(&name).is_none() {
            let tag = gtk::TextTag::builder()
                .name(&name)
                .pixels_below_lines(pixels)
                .build();
            table.add(&tag);
        }
        let (Some(start), Some(mut end)) = (buffer.iter_at_line(line), buffer.iter_at_line(line))
        else {
            return;
        };
        end.forward_line();
        buffer.apply_tag_by_name(&name, &start, &end);
    }

    /// Starts a file: records its head row and inserts its head line.
    fn insert_file_head(
        &self,
        iter: &mut gtk::TextIter,
        rows: &mut RowRecorder,
        file: &FileDiff,
        (open, viewed): (bool, bool),
        toggle_viewed: impl Fn() + 'static,
    ) -> FileSlot {
        rows.set(iter, Row::FileHeader);
        text::insert_file_head(
            &self.folds,
            &self.links,
            iter,
            file,
            open,
            viewed,
            toggle_viewed,
        )
    }

    fn finish(&self, rows: RowRecorder) {
        text::apply_base_font(&self.view.buffer());
        self.view.set_rows(rows.into_rows());
        self.gutter.refresh();
        self.scrolled.hadjustment().set_value(0.0);
    }

    /// Finishes inserting lines from `start`, a line and an offset, to `end`.
    fn finish_load(&self, start: (usize, i32), end: &gtk::TextIter, rows: RowRecorder) {
        let (start_line, start_offset) = start;
        let buffer = self.view.buffer();
        text::finish_file_body(&buffer, &buffer.iter_at_offset(start_offset), end);
        self.view.insert_rows(
            start_line,
            rows.into_rows_between(start_line, end.line() as usize),
        );
        self.gutter.refresh();
    }
}

/// Measures how tall a line is drawn, so that the two sides of a row can be
/// given the same height. A line with characters that fall back to another
/// font, such as Japanese, can be taller than the line beside it.
struct LineHeights {
    view: DiffTextView,
    font: pango::FontDescription,
    /// The height of an empty line or one of only ASCII characters, which the
    /// text font draws on its own
    plain: i32,
}

impl LineHeights {
    fn new(view: &DiffTextView) -> LineHeights {
        let font = text::text_font(view.upcast_ref());
        let mut heights = LineHeights {
            view: view.clone(),
            font,
            plain: 0,
        };
        heights.plain = heights.measure("");
        heights
    }

    fn of(&self, line: Option<&DiffLine>) -> i32 {
        match line {
            Some(line) if !line.content.is_ascii() => self.measure(&line.content),
            _ => self.plain,
        }
    }

    fn measure(&self, content: &str) -> i32 {
        let layout = self.view.create_pango_layout(Some(content));
        layout.set_font_description(Some(&self.font));
        // Rounded to the nearest pixel, as GtkTextView rounds a line's height
        (layout.size().1 + pango::SCALE / 2) / pango::SCALE
    }
}

/// The old version on the left and the new one on the right, row by row.
/// Both sides scroll together vertically and on their own horizontally.
pub struct SplitView {
    root: gtk::Box,
    old: Side,
    new: Side,
    content: SharedContent,
    /// Indexed like the files of `content`, with the old side first. Both sides
    /// add their folds in the same order, so that a fold has the same index on
    /// both.
    slots: RefCell<Vec<(FileSlot, FileSlot)>>,
    reveal: RefCell<Option<RevealFileGap>>,
    toggle_viewed: RefCell<Option<ToggleViewed>>,
}

impl SplitView {
    pub fn new(content: SharedContent) -> Rc<SplitView> {
        let old = Side::new(Columns::Old, None);
        let new = Side::new(Columns::New, Some(&old.scrolled.vadjustment()));
        // One scroll bar on the right is enough for both
        old.scrolled
            .set_vscrollbar_policy(gtk::PolicyType::External);

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .homogeneous(true)
            .hexpand(true)
            .vexpand(true)
            .build();
        root.append(&old.scrolled);
        root.append(&new.scrolled);

        let split = Rc::new(SplitView {
            root,
            old,
            new,
            content,
            slots: RefCell::new(Vec::new()),
            reveal: RefCell::new(None),
            toggle_viewed: RefCell::new(None),
        });

        let w = Rc::downgrade(&split);
        split.old.folds.connect_toggled(move |index, open| {
            if let Some(split) = w.upgrade() {
                split.new.folds.set_open(index, open);
                split.fold_toggled(index, open);
            }
        });
        let w = Rc::downgrade(&split);
        split.new.folds.connect_toggled(move |index, open| {
            if let Some(split) = w.upgrade() {
                split.old.folds.set_open(index, open);
                split.fold_toggled(index, open);
            }
        });
        split
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    /// Sets what reveals unchanged lines when a link of a gap is clicked. The
    /// caller changes the file and draws it again with `reload_file`.
    pub fn connect_reveal(&self, reveal: RevealFileGap) {
        self.reveal.replace(Some(reveal));
    }

    /// Sets what runs when the "Viewed" box of a file is clicked. The caller
    /// shows the change with `set_file_viewed`.
    pub fn connect_toggle_viewed(&self, toggle_viewed: ToggleViewed) {
        self.toggle_viewed.replace(Some(toggle_viewed));
    }

    /// Shows whether a file is viewed on both sides, and closes it when it is,
    /// or opens it.
    pub fn set_file_viewed(&self, file_index: usize, viewed: bool) {
        let Some(fold) = self
            .slots
            .borrow()
            .get(file_index)
            .map(|(old_slot, new_slot)| {
                text::set_viewed_check(&self.old.view.buffer(), old_slot, viewed);
                text::set_viewed_check(&self.new.view.buffer(), new_slot, viewed);
                old_slot.fold
            })
        else {
            return;
        };
        self.old.folds.set_open(fold, !viewed);
        self.new.folds.set_open(fold, !viewed);
        self.file_toggled(file_index, !viewed);
    }

    /// Draws the lines of a file again on both sides, after they changed,
    /// keeping the lines above in place.
    pub fn reload_file(&self, file_index: usize) {
        let adjustment = self.old.scrolled.vadjustment();
        let value = adjustment.value();
        {
            let mut slots = self.slots.borrow_mut();
            match slots.get(file_index) {
                Some((slot, _)) if slot.loaded => {}
                _ => return,
            }
            let (old_slot, new_slot) = &slots[file_index];
            let next = slots.get(file_index + 1);
            self.old.delete_file_body(old_slot, next.map(|s| &s.0));
            self.new.delete_file_body(new_slot, next.map(|s| &s.1));
            let (old_slot, new_slot) = &mut slots[file_index];
            old_slot.loaded = false;
            new_slot.loaded = false;
        }
        self.load(file_index);
        text::restore_scroll(&adjustment, value);
    }

    pub fn show(&self) {
        let mut slots = self.slots.borrow_mut();
        for (old_slot, new_slot) in slots.drain(..) {
            text::forget_file_slot(&self.old.view.buffer(), &old_slot);
            text::forget_file_slot(&self.new.view.buffer(), &new_slot);
        }
        drop(slots);
        self.old.clear();
        self.new.clear();
        let old_buffer = self.old.view.buffer();
        let new_buffer = self.new.view.buffer();
        let mut old_iter = old_buffer.start_iter();
        let mut new_iter = new_buffer.start_iter();
        let mut old_rows = RowRecorder::default();
        let mut new_rows = RowRecorder::default();
        let heights = (
            LineHeights::new(&self.old.view),
            LineHeights::new(&self.new.view),
        );
        let mut slots = Vec::new();

        let mut content = self.content.borrow_mut();
        let content = &mut *content;
        let toggle_viewed = self.toggle_viewed.borrow().clone();
        for (file_index, (file, &open)) in content.files.iter_mut().zip(&content.open).enumerate() {
            let state = (open, content.viewed[file_index]);
            let mut old_slot = self.old.insert_file_head(
                &mut old_iter,
                &mut old_rows,
                file,
                state,
                text::toggle_viewed_of(&toggle_viewed, file_index),
            );
            let mut new_slot = self.new.insert_file_head(
                &mut new_iter,
                &mut new_rows,
                file,
                state,
                text::toggle_viewed_of(&toggle_viewed, file_index),
            );
            if open {
                file.emphasize();
                self.insert_body(
                    (&mut old_iter, &mut new_iter),
                    (&mut old_rows, &mut new_rows),
                    &heights,
                    file_index,
                    file,
                    (&old_slot.body, &new_slot.body),
                );
                old_slot.loaded = true;
                new_slot.loaded = true;
            }
            slots.push((old_slot, new_slot));
        }

        self.old.finish(old_rows);
        self.new.finish(new_rows);
        self.slots.replace(slots);
        self.old.scrolled.vadjustment().set_value(0.0);
    }

    /// Opens a file if it is closed and scrolls its head line to the top.
    pub fn scroll_to_file(&self, file_index: usize) {
        let Some(fold) = self.slots.borrow().get(file_index).map(|s| s.0.fold) else {
            return;
        };
        self.old.folds.set_open(fold, true);
        self.new.folds.set_open(fold, true);
        self.file_toggled(file_index, true);

        // The new side follows, as the sides share their vertical adjustment
        text::scroll_to_file_head(
            self.old.view.upcast_ref(),
            &self.slots.borrow()[file_index].0,
        );
    }

    fn fold_toggled(&self, fold: usize, open: bool) {
        let file_index = self.slots.borrow().iter().position(|s| s.0.fold == fold);
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
        let (old_slot, new_slot) = &mut slots[file_index];
        if old_slot.loaded {
            return;
        }
        old_slot.loaded = true;
        new_slot.loaded = true;

        let mut content = self.content.borrow_mut();
        let file = &mut content.files[file_index];
        file.emphasize();

        let old_buffer = self.old.view.buffer();
        let new_buffer = self.new.view.buffer();
        let mut old_iter = text::file_body_iter(&old_buffer, old_slot);
        let mut new_iter = text::file_body_iter(&new_buffer, new_slot);
        let old_start = (old_iter.line() as usize, old_iter.offset());
        let new_start = (new_iter.line() as usize, new_iter.offset());
        let mut old_rows = RowRecorder::default();
        let mut new_rows = RowRecorder::default();
        let heights = (
            LineHeights::new(&self.old.view),
            LineHeights::new(&self.new.view),
        );
        self.insert_body(
            (&mut old_iter, &mut new_iter),
            (&mut old_rows, &mut new_rows),
            &heights,
            file_index,
            file,
            (&old_slot.body, &new_slot.body),
        );

        self.old.finish_load(old_start, &old_iter, old_rows);
        self.new.finish_load(new_start, &new_iter, new_rows);
    }

    /// Inserts the hunks of a file on both sides, all carrying the body tag of
    /// the file on their side.
    fn insert_body(
        &self,
        iters: (&mut gtk::TextIter, &mut gtk::TextIter),
        rows: (&mut RowRecorder, &mut RowRecorder),
        heights: &(LineHeights, LineHeights),
        file_index: usize,
        file: &FileDiff,
        file_bodies: (&str, &str),
    ) {
        let (old_iter, new_iter) = iters;
        let (old_rows, new_rows) = rows;
        let (old_heights, new_heights) = heights;
        let (old_file_body, new_file_body) = file_bodies;
        let old_buffer = self.old.view.buffer();
        let new_buffer = self.new.view.buffer();

        if let Some(note) = text::body_note(file) {
            text::insert(
                &old_buffer,
                old_iter,
                note,
                &[text::NOTE, text::FILE_BODY, old_file_body],
            );
            text::insert(
                &new_buffer,
                new_iter,
                note,
                &[text::NOTE, text::FILE_BODY, new_file_body],
            );
            return;
        }

        let reveal = self
            .reveal
            .borrow()
            .as_ref()
            .map(|reveal| text::reveal_gap_of(reveal, file_index));
        let gaps = file.gaps();
        let insert_gap = |old_iter: &mut gtk::TextIter,
                          new_iter: &mut gtk::TextIter,
                          old_rows: &mut RowRecorder,
                          new_rows: &mut RowRecorder,
                          before_hunk| {
            let gap = gaps.iter().find(|gap| gap.before_hunk == before_hunk);
            if let (Some(gap), Some(reveal)) = (gap, &reveal) {
                let hunk_count = file.hunks.len();
                self.old
                    .insert_gap(old_iter, old_rows, gap, hunk_count, old_file_body, reveal);
                self.new
                    .insert_gap(new_iter, new_rows, gap, hunk_count, new_file_body, reveal);
            }
        };

        for (hunk_index, hunk) in file.hunks.iter().enumerate() {
            insert_gap(old_iter, new_iter, old_rows, new_rows, hunk_index);
            old_rows.set(old_iter, Row::HunkHeader);
            new_rows.set(new_iter, Row::HunkHeader);
            let old_body = self.old.folds.insert_head(
                old_iter,
                &hunk.header,
                &[text::HUNK_HEADER, text::FILE_BODY, old_file_body],
                true,
            );
            let new_body = self.new.folds.insert_head(
                new_iter,
                &hunk.header,
                &[text::HUNK_HEADER, text::FILE_BODY, new_file_body],
                true,
            );

            for (old, new) in hunk.side_by_side() {
                let old_line = self.old.insert_line(
                    old_iter,
                    old_rows,
                    old,
                    &[text::FILE_BODY, old_file_body, &old_body],
                );
                let new_line = self.new.insert_line(
                    new_iter,
                    new_rows,
                    new,
                    &[text::FILE_BODY, new_file_body, &new_body],
                );

                let old_height = old_heights.of(old);
                let new_height = new_heights.of(new);
                if old_height < new_height {
                    self.old.pad_line(old_line, new_height - old_height);
                } else if new_height < old_height {
                    self.new.pad_line(new_line, old_height - new_height);
                }
            }
        }
        insert_gap(old_iter, new_iter, old_rows, new_rows, file.hunks.len());
    }
}
