use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene};
use std::cell::RefCell;

use super::text;
use crate::diff::{DiffLine, GapButtons, LineKind};

/// The buttons in the gutter that reveal the lines of a gap of a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Expander {
    pub file_index: usize,
    /// The hunk the gap comes before, as `Gap` gives it
    pub before_hunk: usize,
    pub buttons: GapButtons,
}

/// What one line of the buffer shows, which decides its background and the
/// line numbers beside it.
#[derive(Clone, Copy, Debug, Default)]
pub enum Row {
    #[default]
    Blank,
    /// Stands for a line that only the other side of a split view has
    Filler,
    FileHeader,
    /// With the buttons of the gap before the hunk, if there is one
    HunkHeader(Option<Expander>),
    /// Stands for the unchanged lines left out after the last hunk
    Gap(Expander),
    Line {
        kind: LineKind,
        old_lineno: Option<u32>,
        new_lineno: Option<u32>,
    },
}

impl Row {
    pub fn line(line: &DiffLine) -> Row {
        Row::Line {
            kind: line.kind,
            old_lineno: line.old_lineno,
            new_lineno: line.new_lineno,
        }
    }

    pub fn expander(&self) -> Option<Expander> {
        match *self {
            Row::HunkHeader(expander) => expander,
            Row::Gap(expander) => Some(expander),
            _ => None,
        }
    }

    pub fn background(&self) -> Option<gdk::RGBA> {
        let color = match self {
            Row::Blank => return None,
            Row::Filler => text::FILLER_BACKGROUND,
            Row::FileHeader => text::FILE_HEADER_BACKGROUND,
            Row::HunkHeader(_) => text::HUNK_HEADER_BACKGROUND,
            Row::Gap(_) => text::GAP_BACKGROUND,
            Row::Line { kind, .. } => match kind {
                LineKind::Added => text::ADDED_BACKGROUND,
                LineKind::Removed => text::REMOVED_BACKGROUND,
                LineKind::Context => return None,
            },
        };
        Some(gdk::RGBA::parse(color).unwrap())
    }

    /// Draws the background of a row in an area, and a line across its top
    /// where a file starts.
    pub fn draw_background(&self, snapshot: &gtk::Snapshot, area: &graphene::Rect) {
        if let Some(color) = self.background() {
            snapshot.append_color(&color, area);
        }
        if let Row::FileHeader = self {
            let color = gdk::RGBA::parse(text::FILE_SEPARATOR_COLOR).unwrap();
            let line = graphene::Rect::new(area.x(), area.y(), area.width(), FILE_SEPARATOR_WIDTH);
            snapshot.append_color(&color, &line);
        }
    }
}

const FILE_SEPARATOR_WIDTH: f32 = 2.0;

/// Collects the rows of a buffer while it is filled.
#[derive(Default)]
pub struct RowRecorder {
    rows: Vec<Row>,
}

impl RowRecorder {
    /// Records the row of the line that starts at `iter`. Rows are recorded by
    /// line, as a line can hold a character that the buffer treats as a line
    /// break, such as a lone CR, and the lines after it must keep their rows.
    pub fn set(&mut self, iter: &gtk::TextIter, row: Row) {
        let line = iter.line() as usize;
        if self.rows.len() <= line {
            self.rows.resize(line + 1, Row::Blank);
        }
        self.rows[line] = row;
    }

    pub fn into_rows(self) -> Vec<Row> {
        self.rows
    }

    /// The rows of the lines from `start` to before `end`.
    pub fn into_rows_between(self, start: usize, end: usize) -> Vec<Row> {
        let mut rows = self.rows;
        rows.resize(end.max(start), Row::Blank);
        rows.drain(..start);
        rows
    }
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct DiffTextView {
        /// Indexed by the line of the buffer
        pub rows: RefCell<Vec<Row>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DiffTextView {
        const NAME: &'static str = "MetalGitDiffTextView";
        type Type = super::DiffTextView;
        type ParentType = gtk::TextView;
    }

    impl ObjectImpl for DiffTextView {}
    impl WidgetImpl for DiffTextView {}

    impl TextViewImpl for DiffTextView {
        /// Draws the line backgrounds across the whole width. Paragraph
        /// backgrounds of text tags would leave out empty lines.
        fn snapshot_layer(&self, layer: gtk::TextViewLayer, snapshot: gtk::Snapshot) {
            if layer == gtk::TextViewLayer::BelowText {
                let obj = self.obj();
                let visible = obj.visible_rect();
                obj.for_each_visible_row(|row, y, height| {
                    let area = graphene::Rect::new(
                        visible.x() as f32,
                        y as f32,
                        visible.width() as f32,
                        height as f32,
                    );
                    row.draw_background(&snapshot, &area);
                });
            }
            self.parent_snapshot_layer(layer, snapshot);
        }
    }
}

glib::wrapper! {
    /// A text view of a diff that knows what each of its lines shows.
    pub struct DiffTextView(ObjectSubclass<imp::DiffTextView>)
        @extends gtk::TextView, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Scrollable;
}

impl DiffTextView {
    pub fn new(buffer: &gtk::TextBuffer) -> DiffTextView {
        glib::Object::builder().property("buffer", buffer).build()
    }

    /// Replaces the rows, indexed by the line of the buffer.
    pub fn set_rows(&self, rows: Vec<Row>) {
        self.imp().rows.replace(rows);
        self.queue_draw();
    }

    /// Inserts the rows of lines inserted before the line `at`.
    pub fn insert_rows(&self, at: usize, rows: Vec<Row>) {
        let mut current = self.imp().rows.borrow_mut();
        if current.len() < at {
            current.resize(at, Row::Blank);
        }
        current.splice(at..at, rows);
        drop(current);
        self.queue_draw();
    }

    /// Removes the rows of deleted lines.
    pub fn remove_rows(&self, lines: std::ops::Range<usize>) {
        let mut current = self.imp().rows.borrow_mut();
        let end = lines.end.min(current.len());
        let start = lines.start.min(end);
        current.drain(start..end);
        drop(current);
        self.queue_draw();
    }

    /// The row at `y` in the coordinates of the left gutter, with its top in
    /// those coordinates and its height.
    pub fn row_at_gutter_y(&self, y: i32) -> Option<(Row, i32, i32)> {
        let (_, by) = self.window_to_buffer_coords(gtk::TextWindowType::Left, 0, y);
        let (iter, _) = self.line_at_y(by);
        let (top, height) = self.line_yrange(&iter);
        if by < top || by >= top + height {
            return None;
        }
        let row = *self.imp().rows.borrow().get(iter.line() as usize)?;
        let (_, top) = self.buffer_to_window_coords(gtk::TextWindowType::Left, 0, top);
        Some((row, top, height))
    }

    pub fn max_lineno(&self) -> u32 {
        self.imp()
            .rows
            .borrow()
            .iter()
            .filter_map(|row| match *row {
                Row::Line {
                    old_lineno,
                    new_lineno,
                    ..
                } => old_lineno.max(new_lineno),
                _ => None,
            })
            .max()
            .unwrap_or(0)
    }

    /// Calls `f` with each row in view, its top in buffer coordinates and its
    /// height. Folded lines are skipped as they have no height.
    pub fn for_each_visible_row(&self, mut f: impl FnMut(&Row, i32, i32)) {
        let rows = self.imp().rows.borrow();
        let visible = self.visible_rect();
        let bottom = visible.y() + visible.height();
        let (mut iter, _) = self.line_at_y(visible.y());
        loop {
            let (y, height) = self.line_yrange(&iter);
            if y >= bottom {
                break;
            }
            if height > 0 {
                if let Some(row) = rows.get(iter.line() as usize) {
                    f(row, y, height);
                }
            }
            if !iter.forward_line() {
                break;
            }
        }
    }
}
