use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene};
use std::cell::RefCell;

use super::text;
use crate::diff_model::{DiffLine, LineKind};

/// What one line of the buffer shows, which decides its background and the
/// line numbers beside it.
#[derive(Clone, Copy, Debug, Default)]
pub enum Row {
    #[default]
    Blank,
    FileHeader,
    HunkHeader,
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

    pub fn background(&self) -> Option<gdk::RGBA> {
        let color = match self {
            Row::Blank => return None,
            Row::FileHeader => text::FILE_HEADER_BACKGROUND,
            Row::HunkHeader => text::HUNK_HEADER_BACKGROUND,
            Row::Line { kind, .. } => match kind {
                LineKind::Added => text::ADDED_BACKGROUND,
                LineKind::Removed => text::REMOVED_BACKGROUND,
                LineKind::Context => return None,
            },
        };
        Some(gdk::RGBA::parse(color).unwrap())
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
                    if let Some(color) = row.background() {
                        let area = graphene::Rect::new(
                            visible.x() as f32,
                            y as f32,
                            visible.width() as f32,
                            height as f32,
                        );
                        snapshot.append_color(&color, &area);
                    }
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
        @implements gtk::Buildable, gtk::ConstraintTarget, gtk::Scrollable;
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
