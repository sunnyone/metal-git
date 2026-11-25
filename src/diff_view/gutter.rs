use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, pango};

use super::diff_text_view::{DiffTextView, Expander, Row};
use super::text::{self, RevealFileGap};
use crate::diff::{GapButtons, LineKind, Reveal, REVEAL_STEP};

const PADDING: f32 = 6.0;
const MIN_DIGITS: usize = 3;

// Translucent so that they read well on light and dark themes
const EXPANDER_BACKGROUND: &str = "rgba(90,140,255,0.18)";
const EXPANDER_ICON_COLOR: &str = "rgba(90,130,210,1)";
const EXPANDER_HOVER_BACKGROUND: &str = "#3878e6";
const EXPANDER_HOVER_ICON_COLOR: &str = "white";

/// A button beside a gap, which reveals some of its lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Button {
    /// Reveals lines below the hunk above
    Down,
    /// Reveals lines above the hunk below
    Up,
    /// Reveals the whole gap
    All,
}

impl Button {
    fn reveal(self) -> Reveal {
        match self {
            Button::Down => Reveal::Down(REVEAL_STEP),
            Button::Up => Reveal::Up(REVEAL_STEP),
            Button::All => Reveal::All,
        }
    }

    fn tooltip(self) -> String {
        match self {
            Button::Down => format!("Show {} more lines below the code above", REVEAL_STEP),
            Button::Up => format!("Show {} more lines above the code below", REVEAL_STEP),
            Button::All => "Show the hidden lines".to_string(),
        }
    }

    /// The buttons of a gap from the top, each with its share of the height
    /// of the row.
    fn of(buttons: GapButtons) -> &'static [Button] {
        match buttons {
            GapButtons::Both => &[Button::Down, Button::Up],
            GapButtons::Down => &[Button::Down],
            GapButtons::Up => &[Button::Up],
            GapButtons::All => &[Button::All],
        }
    }
}

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
        pub reveal: RefCell<Option<RevealFileGap>>,
        /// The button the pointer is over
        pub(super) hovered: Cell<Option<(Expander, Button)>>,
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
                row.draw_background(snapshot, &area);
                match row.expander() {
                    Some(expander) if self.has_expanders() => {
                        self.draw_expander(snapshot, expander, &area)
                    }
                    _ => self.draw_numbers(snapshot, row, &area, &color),
                }
            });
        }
    }

    impl LineNumberGutter {
        /// The buttons of gaps are in the gutter of the old side of a split
        /// view, not on both sides, as on GitHub.
        fn has_expanders(&self) -> bool {
            self.columns.get() != Columns::New
        }

        /// The button at `y` in the coordinates of the gutter.
        pub(super) fn button_at(&self, y: f64) -> Option<(Expander, Button)> {
            if !self.has_expanders() {
                return None;
            }
            let view = self.view.upgrade()?;
            let (row, top, height) = view.row_at_gutter_y(y as i32)?;
            let expander = row.expander()?;
            let buttons = Button::of(expander.buttons);
            let index = ((y - top as f64) * buttons.len() as f64 / height as f64) as usize;
            let button = buttons[index.min(buttons.len() - 1)];
            Some((expander, button))
        }

        /// Draws the buttons of a gap one over the other, filling the area.
        fn draw_expander(
            &self,
            snapshot: &gtk::Snapshot,
            expander: Expander,
            area: &graphene::Rect,
        ) {
            let buttons = Button::of(expander.buttons);
            let height = area.height() / buttons.len() as f32;
            for (i, &button) in buttons.iter().enumerate() {
                let rect = graphene::Rect::new(
                    area.x(),
                    area.y() + height * i as f32,
                    area.width(),
                    height,
                );
                let hovered = self.hovered.get() == Some((expander, button));
                let (background, icon) = if hovered {
                    (EXPANDER_HOVER_BACKGROUND, EXPANDER_HOVER_ICON_COLOR)
                } else {
                    (EXPANDER_BACKGROUND, EXPANDER_ICON_COLOR)
                };
                snapshot.append_color(&gdk::RGBA::parse(background).unwrap(), &rect);
                draw_icon(snapshot, button, &rect, &gdk::RGBA::parse(icon).unwrap());
            }
        }

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
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl LineNumberGutter {
    /// Creates a gutter and places it on the left of the view.
    pub fn attach(view: &DiffTextView, columns: Columns) -> LineNumberGutter {
        let gutter: LineNumberGutter = glib::Object::new();
        gutter.imp().view.set(Some(view));
        gutter.imp().columns.set(columns);
        gutter.imp().digits.set(MIN_DIGITS);
        view.set_gutter(gtk::TextWindowType::Left, Some(&gutter));
        gutter.add_controllers();

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

    /// Sets what reveals unchanged lines when a button of a gap is clicked.
    pub fn connect_reveal(&self, reveal: RevealFileGap) {
        self.imp().reveal.replace(Some(reveal));
    }

    /// Makes the buttons of gaps follow the pointer and reveal lines when
    /// clicked.
    fn add_controllers(&self) {
        let click = gtk::GestureClick::new();
        let g = self.downgrade();
        click.connect_released(move |_, _, _, y| {
            let Some(gutter) = g.upgrade() else {
                return;
            };
            let imp = gutter.imp();
            let Some((expander, button)) = imp.button_at(y) else {
                return;
            };
            // Cloned out first, as revealing draws the lines and the rows again
            let reveal = imp.reveal.borrow().clone();
            gutter.set_hovered(None);
            if let Some(reveal) = reveal {
                reveal(expander.file_index, expander.before_hunk, button.reveal());
            }
        });
        self.add_controller(click);

        let motion = gtk::EventControllerMotion::new();
        let g = self.downgrade();
        motion.connect_motion(move |_, _, y| {
            if let Some(gutter) = g.upgrade() {
                let hovered = gutter.imp().button_at(y);
                gutter.set_hovered(hovered);
            }
        });
        let g = self.downgrade();
        motion.connect_leave(move |_| {
            if let Some(gutter) = g.upgrade() {
                gutter.set_hovered(None);
            }
        });
        self.add_controller(motion);

        self.set_has_tooltip(true);
        self.connect_query_tooltip(|gutter, _, y, _, tooltip| {
            match gutter.imp().button_at(y as f64) {
                Some((_, button)) => {
                    tooltip.set_text(Some(&button.tooltip()));
                    true
                }
                None => false,
            }
        });
    }

    fn set_hovered(&self, hovered: Option<(Expander, Button)>) {
        let imp = self.imp();
        if imp.hovered.get() == hovered {
            return;
        }
        imp.hovered.set(hovered);
        self.set_cursor_from_name(hovered.map(|_| "pointer"));
        self.queue_draw();
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

/// Draws the icon of a button in the middle of an area, in the shapes of the
/// icons GitHub uses: an arrow from a line for revealing lines on one side,
/// and arrows apart for revealing them all.
fn draw_icon(snapshot: &gtk::Snapshot, button: Button, area: &graphene::Rect, color: &gdk::RGBA) {
    let cr = snapshot.append_cairo(area);
    let (x, y) = (
        (area.x() + area.width() / 2.0).round() as f64,
        (area.y() + area.height() / 2.0).round() as f64,
    );
    let segments: &[&[(f64, f64)]] = match button {
        Button::Down => &[
            &[(-5.0, -4.0), (5.0, -4.0)],
            &[(-4.0, -1.0), (0.0, 3.0), (4.0, -1.0)],
        ],
        Button::Up => &[
            &[(-5.0, 4.0), (5.0, 4.0)],
            &[(-4.0, 1.0), (0.0, -3.0), (4.0, 1.0)],
        ],
        Button::All => &[
            &[(-4.0, -2.0), (0.0, -6.0), (4.0, -2.0)],
            &[(-4.0, 2.0), (0.0, 6.0), (4.0, 2.0)],
        ],
    };
    for points in segments {
        for (i, (dx, dy)) in points.iter().enumerate() {
            if i == 0 {
                cr.move_to(x + dx, y + dy);
            } else {
                cr.line_to(x + dx, y + dy);
            }
        }
    }
    cr.set_source_rgba(
        color.red() as f64,
        color.green() as f64,
        color.blue() as f64,
        color.alpha() as f64,
    );
    cr.set_line_width(1.5);
    cr.set_line_cap(gtk::cairo::LineCap::Round);
    cr.set_line_join(gtk::cairo::LineJoin::Round);
    let _ = cr.stroke();
}
