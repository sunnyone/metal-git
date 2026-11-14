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
/// The line counts on the head line of a file, which matter less than its path
const NOTE_COUNTS: &str = "note-counts";
/// Carried by the lines of a file below its head line, which it indents
pub const FILE_BODY: &str = "file-body";

// Translucent so that they read well on light and dark themes
pub const FILE_HEADER_BACKGROUND: &str = "rgba(127,127,127,0.22)";
pub const HUNK_HEADER_BACKGROUND: &str = "rgba(90,140,255,0.12)";
pub const ADDED_BACKGROUND: &str = "rgba(46,160,67,0.16)";
pub const REMOVED_BACKGROUND: &str = "rgba(248,81,73,0.16)";
pub const FILLER_BACKGROUND: &str = "rgba(127,127,127,0.10)";
pub const FILE_SEPARATOR_COLOR: &str = "rgba(127,127,127,0.65)";
/// How far the lines of a file are indented from its head line
const FILE_BODY_INDENT: i32 = 20;
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
            .name(FILE_BODY)
            .left_margin(FILE_BODY_INDENT)
            .build(),
        gtk::TextTag::builder()
            .name(NOTE)
            .foreground("rgba(127,127,127,1)")
            .style(gtk::pango::Style::Italic)
            .build(),
        gtk::TextTag::builder()
            .name(NOTE_COUNTS)
            .foreground("rgba(127,127,127,1)")
            .weight(400)
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
    // Badges of the status of a file, drawn over the line backgrounds
    for status in FileStatus::ALL {
        table.add(&status_tag(status));
    }
    table
}

fn status_tag_name(status: FileStatus) -> String {
    format!("status-{}", status.letter())
}

/// A badge of white bold text on the color of a status.
fn status_tag(status: FileStatus) -> gtk::TextTag {
    gtk::TextTag::builder()
        .name(status_tag_name(status))
        .foreground("white")
        .background(super::status_color(status))
        .weight(700)
        .build()
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

/// Finishes lines inserted between two file head lines: gives them the base
/// font, and takes off the style of the head lines. Adjacent ranges of a tag
/// join into one, so the lines inserted between two head lines fall inside
/// the range of `FILE_HEADER`.
pub fn finish_file_body(buffer: &gtk::TextBuffer, start: &gtk::TextIter, end: &gtk::TextIter) {
    buffer.remove_tag_by_name(FILE_HEADER, start, end);
    buffer.apply_tag_by_name(BASE, start, end);
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

/// The label of the head line of a file, in segments with the tags of each:
/// a badge of its status, which is one letter wide so that the paths line up,
/// then its path, which matters most, then its line counts.
pub fn file_header_segments(file: &FileDiff) -> Vec<(String, Vec<String>)> {
    vec![
        (
            format!(" {} ", file.status.letter()),
            vec![status_tag_name(file.status)],
        ),
        (format!(" {}", file.display_path()), vec![]),
        (
            format!(
                "    +{} −{}",
                file.count_lines(LineKind::Added),
                file.count_lines(LineKind::Removed)
            ),
            vec![NOTE_COUNTS.to_string()],
        ),
    ]
}

/// Where a file is in a view, and whether its lines are in the buffer yet.
pub struct FileSlot {
    /// The fold that the head line of the file opens and closes
    pub fold: usize,
    /// At the start of the head line. It has right gravity, so that it stays on
    /// its line when the lines of the file before are inserted.
    pub head: gtk::TextMark,
    /// The tag that the lines of the file carry
    pub body: String,
    /// The lines of a closed file are inserted only when it is opened
    pub loaded: bool,
}

/// Inserts the head line of a file, which opens and closes the file when clicked.
pub fn insert_file_head(
    folds: &Folds,
    iter: &mut gtk::TextIter,
    file: &FileDiff,
    open: bool,
) -> FileSlot {
    let buffer = iter.buffer();
    let start = iter.offset();
    let fold = folds.len();
    let segments = file_header_segments(file);
    let tag_refs: Vec<Vec<&str>> = segments
        .iter()
        .map(|(_, tags)| tags.iter().map(|t| t.as_str()).collect())
        .collect();
    let segment_refs: Vec<(&str, &[&str])> = segments
        .iter()
        .zip(&tag_refs)
        .map(|((text, _), tags)| (text.as_str(), tags.as_slice()))
        .collect();
    let body = folds.insert_head_segments(iter, &segment_refs, &[FILE_HEADER], open);
    let head = buffer.create_mark(None, &buffer.iter_at_offset(start), false);
    FileSlot {
        fold,
        head,
        body,
        loaded: false,
    }
}

/// Scrolls the head line of a file to the top. Lines that are not laid out yet
/// have an estimated height, so the view scrolls once more after the layout of
/// the buffer is done, which runs at a higher priority than the default idle.
pub fn scroll_to_file_head(view: &gtk::TextView, slot: &FileSlot) {
    view.scroll_to_mark(&slot.head, 0.0, true, 0.0, 0.0);
    let view = view.clone();
    let head = slot.head.clone();
    gtk::glib::idle_add_local_once(move || {
        if !head.is_deleted() {
            view.scroll_to_mark(&head, 0.0, true, 0.0, 0.0);
        }
    });
}

/// Deletes the marks of the files shown before. Call it before showing others.
pub fn forget_file_slots(buffer: &gtk::TextBuffer, slots: &mut Vec<FileSlot>) {
    for slot in slots.drain(..) {
        buffer.delete_mark(&slot.head);
    }
}

/// Where the lines of a file go: the start of the line after its head line.
pub fn file_body_iter(buffer: &gtk::TextBuffer, slot: &FileSlot) -> gtk::TextIter {
    let mut iter = buffer.iter_at_mark(&slot.head);
    iter.forward_line();
    iter
}

/// Hides the lines of a closed fold. An open one leaves their visibility unset
/// rather than visible, so that a closed fold around it, such as a file around
/// a hunk, still hides them whatever the priorities of the tags.
fn set_body_open(body: &gtk::TextTag, open: bool) {
    body.set_invisible(!open);
    body.set_property("invisible-set", !open);
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

    /// The number of folds, which is the index the next one gets.
    pub fn len(&self) -> usize {
        self.folds.borrow().len()
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
        self.insert_head_segments(iter, &[(label, &[])], tags, open)
    }

    /// Inserts a head line whose label is in segments, each with tags of its
    /// own on top of `tags`, and returns the name of the tag that the lines of
    /// the section must carry.
    pub fn insert_head_segments(
        &self,
        iter: &mut gtk::TextIter,
        label: &[(&str, &[&str])],
        tags: &[&str],
        open: bool,
    ) -> String {
        let buffer = self.view.buffer();
        let table = buffer.tag_table();
        let mut folds = self.folds.borrow_mut();
        let index = folds.len();

        let head = gtk::TextTag::new(Some(&format!("fold-head-{}", index)));
        let body = gtk::TextTag::new(Some(&format!("fold-body-{}", index)));
        set_body_open(&body, open);
        table.add(&head);
        table.add(&body);

        let mut head_tags: Vec<String> = tags.iter().map(|t| t.to_string()).collect();
        head_tags.push(head.name().unwrap().to_string());
        let head_tag_refs: Vec<&str> = head_tags.iter().map(|t| t.as_str()).collect();

        let marker = if open { OPEN_MARKER } else { CLOSED_MARKER };
        insert(&buffer, iter, marker, &head_tag_refs);
        insert(&buffer, iter, " ", &head_tag_refs);
        for (text, extra_tags) in label {
            let mut segment_tags = head_tag_refs.clone();
            segment_tags.extend_from_slice(extra_tags);
            insert(&buffer, iter, text, &segment_tags);
        }
        insert(&buffer, iter, "\n", &head_tag_refs);

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
        set_body_open(&fold.body, open);

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
