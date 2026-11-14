use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use super::diff_text_view::DiffTextView;
use crate::diff_model::{DiffLine, FileDiff, FileStatus, LineKind};

const BASE: &str = "base";
pub const FILE_HEADER: &str = "file-header";
pub const HUNK_HEADER: &str = "hunk-header";
pub const ADDED: &str = "added";
pub const REMOVED: &str = "removed";
pub const ADDED_EMPHASIS: &str = "added-emphasis";
pub const REMOVED_EMPHASIS: &str = "removed-emphasis";
pub const NOTE: &str = "note";

// Translucent so that they read well on light and dark themes
pub const FILE_HEADER_BACKGROUND: &str = "rgba(127,127,127,0.22)";
pub const HUNK_HEADER_BACKGROUND: &str = "rgba(90,140,255,0.12)";
pub const ADDED_BACKGROUND: &str = "rgba(46,160,67,0.16)";
pub const REMOVED_BACKGROUND: &str = "rgba(248,81,73,0.16)";
pub const FILLER_BACKGROUND: &str = "rgba(127,127,127,0.10)";
pub const LINENO_COLOR: &str = "rgba(127,127,127,0.85)";

const OPEN_MARKER: &str = "▾";
const CLOSED_MARKER: &str = "▸";

fn create_tag_table() -> gtk::TextTagTable {
    let table = gtk::TextTagTable::new();
    let tags = [
        // Defined first so that every other tag overrides it
        gtk::TextTag::new(Some(BASE)),
        gtk::TextTag::builder()
            .name(FILE_HEADER)
            .weight(700)
            .pixels_above_lines(10)
            .pixels_below_lines(4)
            .build(),
        gtk::TextTag::builder()
            .name(HUNK_HEADER)
            .foreground("rgba(90,130,210,1)")
            .build(),
        // Only mark the lines; DiffTextView draws their backgrounds
        gtk::TextTag::new(Some(ADDED)),
        gtk::TextTag::new(Some(REMOVED)),
        gtk::TextTag::builder()
            .name(NOTE)
            .foreground("rgba(127,127,127,1)")
            .style(gtk::pango::Style::Italic)
            .build(),
        // Defined last so that they draw over the line backgrounds
        gtk::TextTag::builder()
            .name(ADDED_EMPHASIS)
            .background("rgba(46,160,67,0.45)")
            .build(),
        gtk::TextTag::builder()
            .name(REMOVED_EMPHASIS)
            .background("rgba(248,81,73,0.45)")
            .build(),
    ];
    for tag in &tags {
        table.add(tag);
    }
    table
}

pub fn create_text_view() -> DiffTextView {
    let buffer = gtk::TextBuffer::builder()
        .tag_table(&create_tag_table())
        .build();
    let view = DiffTextView::new(&buffer);
    view.set_editable(false);
    view.set_cursor_visible(false);
    view.set_monospace(true);
    view.set_top_margin(4);
    view.set_bottom_margin(4);

    if let Some(family) = latin_monospace_family(view.upcast_ref()) {
        let base = buffer.tag_table().lookup(BASE).unwrap();
        base.set_family(Some(&family));
    }
    view
}

/// The font that the generic monospace family resolves to for Latin text.
///
/// Digits, spaces and punctuation belong to no script, so Pango draws them in
/// the font of the text around them, or in the font of the locale on a line
/// with nothing else, such as an empty line. In a Japanese locale that is a
/// Japanese font with other widths, which shifts indentation and columns.
/// Naming the Latin font keeps them in one font, while the characters it does
/// not have still fall back to the other fonts.
fn latin_monospace_family(view: &gtk::TextView) -> Option<String> {
    let context = view.pango_context();
    let mut desc = gtk::pango::FontDescription::new();
    desc.set_family("monospace");
    let fontset = context.load_fontset(&desc, &gtk::pango::Language::from_string("en"))?;
    let font = fontset.font('0' as u32);
    font.describe().family().map(|f| f.to_string())
}

/// The font the text of the view is drawn in, apart from the characters that
/// fall back to other fonts.
pub fn text_font(view: &gtk::TextView) -> gtk::pango::FontDescription {
    let mut font = view.pango_context().font_description().unwrap_or_default();
    if let Some(family) = latin_monospace_family(view) {
        font.set_family(&family);
    }
    font
}

/// Gives the whole buffer the base font. Call it after filling the buffer.
pub fn apply_base_font(buffer: &gtk::TextBuffer) {
    buffer.apply_tag_by_name(BASE, &buffer.start_iter(), &buffer.end_iter());
}

pub fn scrolled(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .hexpand(true)
        .vexpand(true)
        .child(child)
        .build()
}

pub fn insert(buffer: &gtk::TextBuffer, iter: &mut gtk::TextIter, text: &str, tags: &[&str]) {
    buffer.insert_with_tags_by_name(iter, text, tags);
}

pub fn line_tags(kind: LineKind) -> (Option<&'static str>, &'static str) {
    match kind {
        LineKind::Added => (Some(ADDED), ADDED_EMPHASIS),
        LineKind::Removed => (Some(REMOVED), REMOVED_EMPHASIS),
        LineKind::Context => (None, ADDED_EMPHASIS),
    }
}

/// Inserts the content of a line and its line break, emphasizing its changed words.
/// `tags` apply to the whole line.
pub fn insert_content(
    buffer: &gtk::TextBuffer,
    iter: &mut gtk::TextIter,
    line: &DiffLine,
    tags: &[&str],
) {
    let (_, emphasis_tag) = line_tags(line.kind);
    let mut emphasized_tags = tags.to_vec();
    emphasized_tags.push(emphasis_tag);

    let mut pos = 0;
    for range in &line.emphasis {
        insert(buffer, iter, &line.content[pos..range.start], tags);
        insert(buffer, iter, &line.content[range.clone()], &emphasized_tags);
        pos = range.end;
    }
    insert(buffer, iter, &line.content[pos..], tags);
    insert(buffer, iter, "\n", tags);
}

pub fn file_header_text(file: &FileDiff) -> String {
    let status = match file.status {
        FileStatus::Added => "Added",
        FileStatus::Deleted => "Deleted",
        FileStatus::Modified => "Modified",
        FileStatus::Renamed => "Renamed",
        FileStatus::Copied => "Copied",
        FileStatus::Other => "Changed",
    };
    format!(
        " {}  {}    +{} −{}\n",
        status,
        file.display_path(),
        file.count_lines(LineKind::Added),
        file.count_lines(LineKind::Removed)
    )
}

struct Fold {
    head: gtk::TextTag,
    body: gtk::TextTag,
    head_tags: Vec<String>,
}

/// Called with the index of a fold and whether it is now open.
type ToggledCallback = Box<dyn Fn(usize, bool)>;

/// Sections of a text view that open and close when their head line is clicked.
pub struct Folds {
    view: gtk::TextView,
    folds: RefCell<Vec<Fold>>,
    on_toggled: RefCell<Option<ToggledCallback>>,
}

impl Folds {
    pub fn new(view: &gtk::TextView) -> Rc<Folds> {
        let folds = Rc::new(Folds {
            view: view.clone(),
            folds: RefCell::new(Vec::new()),
            on_toggled: RefCell::new(None),
        });

        let click = gtk::GestureClick::new();
        let w = Rc::downgrade(&folds);
        click.connect_released(move |_, _, x, y| {
            if let Some(folds) = w.upgrade() {
                folds.clicked(x, y);
            }
        });
        view.add_controller(click);

        folds
    }

    /// Calls `callback` with the index of a fold and whether it is now open,
    /// whenever a click opens or closes it.
    pub fn connect_toggled(&self, callback: impl Fn(usize, bool) + 'static) {
        self.on_toggled.replace(Some(Box::new(callback)));
    }

    /// Forgets the folds of the previous content. Call it before clearing the buffer.
    pub fn clear(&self) {
        let table = self.view.buffer().tag_table();
        for fold in self.folds.borrow_mut().drain(..) {
            table.remove(&fold.head);
            table.remove(&fold.body);
        }
    }

    /// Inserts a head line and returns the name of the tag that the lines of the
    /// section must carry.
    pub fn insert_head(
        &self,
        iter: &mut gtk::TextIter,
        label: &str,
        tags: &[&str],
        open: bool,
    ) -> String {
        let buffer = self.view.buffer();
        let table = buffer.tag_table();
        let mut folds = self.folds.borrow_mut();
        let index = folds.len();

        let head = gtk::TextTag::new(Some(&format!("fold-head-{}", index)));
        let body = gtk::TextTag::builder()
            .name(format!("fold-body-{}", index))
            .invisible(!open)
            .build();
        table.add(&head);
        table.add(&body);

        let mut head_tags: Vec<String> = tags.iter().map(|t| t.to_string()).collect();
        head_tags.push(head.name().unwrap().to_string());
        let head_tag_refs: Vec<&str> = head_tags.iter().map(|t| t.as_str()).collect();

        let marker = if open { OPEN_MARKER } else { CLOSED_MARKER };
        insert(&buffer, iter, marker, &head_tag_refs);
        insert(&buffer, iter, &format!(" {}\n", label), &head_tag_refs);

        let body_name = body.name().unwrap().to_string();
        folds.push(Fold {
            head,
            body,
            head_tags,
        });
        body_name
    }

    fn clicked(&self, x: f64, y: f64) {
        let (bx, by) =
            self.view
                .window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
        let Some(iter) = self.view.iter_at_location(bx, by) else {
            return;
        };

        let found = self
            .folds
            .borrow()
            .iter()
            .enumerate()
            .find(|(_, f)| iter.has_tag(&f.head))
            .map(|(index, f)| (index, f.body.is_invisible()));
        let Some((index, open)) = found else {
            return;
        };

        self.set_open(index, open);
        if let Some(callback) = self.on_toggled.borrow().as_ref() {
            callback(index, open);
        }
    }

    /// Opens or closes a fold, without notifying `connect_toggled`.
    pub fn set_open(&self, index: usize, open: bool) {
        let folds = self.folds.borrow();
        let Some(fold) = folds.get(index) else {
            return;
        };
        if fold.body.is_invisible() != open {
            return;
        }
        fold.body.set_invisible(!open);

        let buffer = self.view.buffer();
        let mut start = buffer.start_iter();
        if !start.starts_tag(Some(&fold.head)) {
            start.forward_to_tag_toggle(Some(&fold.head));
        }
        let mut end = start;
        end.forward_char();
        buffer.delete(&mut start, &mut end);
        let marker = if open { OPEN_MARKER } else { CLOSED_MARKER };
        let tag_refs: Vec<&str> = fold.head_tags.iter().map(|t| t.as_str()).collect();
        insert(&buffer, &mut start, marker, &tag_refs);
    }
}
