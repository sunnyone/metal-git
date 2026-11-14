use gtk::pango;
use gtk::prelude::*;
use std::rc::Rc;

use super::diff_text_view::{DiffTextView, Row, RowRecorder};
use super::gutter::{Columns, LineNumberGutter};
use super::text::{self, Folds};
use crate::diff_model::{DiffLine, FileDiff};

/// One side of a split view.
struct Side {
    view: DiffTextView,
    scrolled: gtk::ScrolledWindow,
    folds: Rc<Folds>,
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
        let gutter = LineNumberGutter::attach(&view, columns);
        Side {
            view,
            scrolled,
            folds,
            gutter,
        }
    }

    fn clear(&self) {
        self.folds.clear();
        self.view.buffer().set_text("");
    }

    /// Inserts a line, or a filler for a line that only the other side has,
    /// and returns the line of the buffer it starts on.
    fn insert_line(
        &self,
        iter: &mut gtk::TextIter,
        rows: &mut RowRecorder,
        line: Option<&DiffLine>,
        body: &str,
    ) -> i32 {
        let buffer = self.view.buffer();
        let start = iter.line();
        let Some(line) = line else {
            rows.set(iter, Row::Filler);
            text::insert(&buffer, iter, "\n", &[body]);
            return start;
        };

        let (line_tag, _) = text::line_tags(line.kind);
        let mut tags = vec![body];
        tags.extend(line_tag);
        rows.set(iter, Row::line(line));
        text::insert_content(&buffer, iter, line, &tags);
        start
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

    fn finish(&self, rows: RowRecorder) {
        text::apply_base_font(&self.view.buffer());
        self.view.set_rows(rows.into_rows());
        self.gutter.refresh();
        self.scrolled.hadjustment().set_value(0.0);
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
}

impl SplitView {
    pub fn new() -> SplitView {
        let old = Side::new(Columns::Old, None);
        let new = Side::new(Columns::New, Some(&old.scrolled.vadjustment()));
        // One scroll bar on the right is enough for both
        old.scrolled
            .set_vscrollbar_policy(gtk::PolicyType::External);

        let w = Rc::downgrade(&new.folds);
        old.folds.connect_toggled(move |index, open| {
            if let Some(folds) = w.upgrade() {
                folds.set_open(index, open);
            }
        });
        let w = Rc::downgrade(&old.folds);
        new.folds.connect_toggled(move |index, open| {
            if let Some(folds) = w.upgrade() {
                folds.set_open(index, open);
            }
        });

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .homogeneous(true)
            .hexpand(true)
            .vexpand(true)
            .build();
        root.append(&old.scrolled);
        root.append(&new.scrolled);

        SplitView { root, old, new }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    pub fn show(&self, files: &[FileDiff]) {
        self.old.clear();
        self.new.clear();
        let old_buffer = self.old.view.buffer();
        let new_buffer = self.new.view.buffer();
        let mut old_iter = old_buffer.start_iter();
        let mut new_iter = new_buffer.start_iter();
        let mut old_rows = RowRecorder::default();
        let mut new_rows = RowRecorder::default();
        let old_heights = LineHeights::new(&self.old.view);
        let new_heights = LineHeights::new(&self.new.view);

        for file in files {
            let header = text::file_header_text(file);
            old_rows.set(&old_iter, Row::FileHeader);
            new_rows.set(&new_iter, Row::FileHeader);
            text::insert(&old_buffer, &mut old_iter, &header, &[text::FILE_HEADER]);
            text::insert(&new_buffer, &mut new_iter, &header, &[text::FILE_HEADER]);
            if file.binary {
                text::insert(&old_buffer, &mut old_iter, "Binary file\n", &[text::NOTE]);
                text::insert(&new_buffer, &mut new_iter, "Binary file\n", &[text::NOTE]);
                continue;
            }

            for hunk in &file.hunks {
                old_rows.set(&old_iter, Row::HunkHeader);
                new_rows.set(&new_iter, Row::HunkHeader);
                // Both sides add their folds in the same order, so that a fold
                // has the same index on both
                let old_body = self.old.folds.insert_head(
                    &mut old_iter,
                    &hunk.header,
                    &[text::HUNK_HEADER],
                    true,
                );
                let new_body = self.new.folds.insert_head(
                    &mut new_iter,
                    &hunk.header,
                    &[text::HUNK_HEADER],
                    true,
                );

                for (old, new) in hunk.side_by_side() {
                    let old_line =
                        self.old
                            .insert_line(&mut old_iter, &mut old_rows, old, &old_body);
                    let new_line =
                        self.new
                            .insert_line(&mut new_iter, &mut new_rows, new, &new_body);

                    let old_height = old_heights.of(old);
                    let new_height = new_heights.of(new);
                    if old_height < new_height {
                        self.old.pad_line(old_line, new_height - old_height);
                    } else if new_height < old_height {
                        self.new.pad_line(new_line, old_height - new_height);
                    }
                }
            }
        }

        self.old.finish(old_rows);
        self.new.finish(new_rows);
        self.old.scrolled.vadjustment().set_value(0.0);
    }
}
