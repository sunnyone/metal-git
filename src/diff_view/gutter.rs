use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, pango};

use super::diff_text_view::{DiffTextView, Row};
use super::text;
use crate::diff_model::LineKind;

const PADDING: f32 = 6.0;
const MIN_DIGITS: usize = 3;

/// Which line numbers the gutter shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Columns {
    /// The old and the new line numbers, for a unified view
    #[default]
    Both,
    Old,
    New,
}

impl Columns {
    fn count(self) -> f32 {
        match self {
            Columns::Both => 2.0,
            Columns::Old | Columns::New => 1.0,
        }
    }
}

mod imp {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    pub struct LineNumberGutter {
        pub view: glib::WeakRef<DiffTextView>,
        pub columns: Cell<Columns>,
        pub font: RefCell<Option<pango::FontDescription>>,
        pub digits: Cell<usize>,
        pub digit_width: Cell<f32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LineNumberGutter {
        const NAME: &'static str = "MetalGitLineNumberGutter";
        type Type = super::LineNumberGutter;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for LineNumberGutter {}

    impl WidgetImpl for LineNumberGutter {
        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            match orientation {
                gtk::Orientation::Horizontal => {
                    let width = self.preferred_width().ceil() as i32;
                    (width, width, -1, -1)
                }
                _ => (0, 0, -1, -1),
            }
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let Some(view) = self.view.upgrade() else {
                return;
            };
            let width = WidgetExt::width(&*self.obj()) as f32;
            let color = gdk::RGBA::parse(text::LINENO_COLOR).unwrap();

            view.for_each_visible_row(|row, y, height| {
                let (_, top) = view.buffer_to_window_coords(gtk::TextWindowType::Left, 0, y);
                let area = graphene::Rect::new(0.0, top as f32, width, height as f32);
                if let Some(background) = row.background() {
                    snapshot.append_color(&background, &area);
                }
                self.draw_numbers(snapshot, row, &area, &color);
            });
        }
    }

    impl LineNumberGutter {
        /// Columns of line numbers and one of the sign, each a fixed number of
        /// digits wide.
        fn preferred_width(&self) -> f32 {
            let digits = self.digits.get() as f32;
            let columns = self.columns.get().count();
            PADDING * (columns + 2.0) + self.digit_width.get() * (digits * columns + 1.0)
        }

        fn draw_numbers(
            &self,
            snapshot: &gtk::Snapshot,
            row: &Row,
            area: &graphene::Rect,
            color: &gdk::RGBA,
        ) {
            let Row::Line {
                kind,
                old_lineno,
                new_lineno,
            } = *row
            else {
                return;
            };

            let numbers = match self.columns.get() {
                Columns::Both => vec![old_lineno, new_lineno],
                Columns::Old => vec![old_lineno],
                Columns::New => vec![new_lineno],
            };
            let column = self.digit_width.get() * self.digits.get() as f32;
            let mut right = 0.0;
            for n in numbers {
                right += PADDING + column;
                if let Some(n) = n {
                    self.draw_text(snapshot, &n.to_string(), right, true, area, color);
                }
            }

            let sign = match kind {
                LineKind::Added => "+",
                LineKind::Removed => "-",
                LineKind::Context => "",
            };
            self.draw_text(snapshot, sign, right + PADDING, false, area, color);
        }

        /// Draws text centered vertically in the area, either ending at `x` or
        /// starting from it.
        fn draw_text(
            &self,
            snapshot: &gtk::Snapshot,
            text: &str,
            x: f32,
            align_right: bool,
            area: &graphene::Rect,
            color: &gdk::RGBA,
        ) {
            let layout = self.obj().create_pango_layout(Some(text));
            layout.set_font_description(self.font.borrow().as_ref());
            let (text_width, text_height) = layout.pixel_size();
            let left = if align_right {
                x - text_width as f32
            } else {
                x
            };
            let top = area.y() + (area.height() - text_height as f32) / 2.0;

            snapshot.save();
            snapshot.translate(&graphene::Point::new(left, top));
            snapshot.append_layout(&layout, color);
            snapshot.restore();
        }
    }
}

glib::wrapper! {
    /// Line numbers drawn beside a diff, in columns of a fixed width, so that
    /// they stay aligned whatever font the text of each line is drawn in.
    pub struct LineNumberGutter(ObjectSubclass<imp::LineNumberGutter>)
        @extends gtk::Widget,
        @implements gtk::Buildable, gtk::ConstraintTarget;
}

impl LineNumberGutter {
    /// Creates a gutter and places it on the left of the view.
    pub fn attach(view: &DiffTextView, columns: Columns) -> LineNumberGutter {
        let gutter: LineNumberGutter = glib::Object::new();
        gutter.imp().view.set(Some(view));
        gutter.imp().columns.set(columns);
        gutter.imp().digits.set(MIN_DIGITS);
        view.set_gutter(gtk::TextWindowType::Left, Some(&gutter));

        // Lines move when the view scrolls and when hunks fold or open, which
        // changes the height of the content
        if let Some(adjustment) = view.vadjustment() {
            let g = gutter.downgrade();
            adjustment.connect_value_changed(move |_| {
                if let Some(gutter) = g.upgrade() {
                    gutter.queue_draw();
                }
            });
            let g = gutter.downgrade();
            adjustment.connect_changed(move |_| {
                if let Some(gutter) = g.upgrade() {
                    gutter.queue_draw();
                }
            });
        }
        gutter
    }

    /// Sizes the columns for the rows the view now has.
    pub fn refresh(&self) {
        let imp = self.imp();
        let Some(view) = imp.view.upgrade() else {
            return;
        };

        imp.digits
            .set(view.max_lineno().to_string().len().max(MIN_DIGITS));

        let font = text::text_font(view.upcast_ref());
        let layout = self.create_pango_layout(Some("0"));
        layout.set_font_description(Some(&font));
        imp.digit_width.set(layout.pixel_size().0 as f32);
        imp.font.replace(Some(font));

        self.queue_resize();
        self.queue_draw();
    }
}
