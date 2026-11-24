//! Syntax highlighting of the lines of a diff: which kind of token each part
//! of a line is, such as a keyword or a string, found from the path of its
//! file. The views choose the colors of the kinds.

use std::ops::Range;
use std::path::Path;
use std::str::FromStr;
use std::sync::OnceLock;

use syntect::highlighting::{
    Color, HighlightState, Highlighter, RangedHighlightIterator, ScopeSelectors, StyleModifier,
    Theme, ThemeItem,
};
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};

use super::model::{DiffHunk, DiffLine, FileDiff, LineKind};

/// Lines longer than this are left plain, along with the rest of their file,
/// as they are mostly generated, such as minified code, and slow to parse.
pub const MAX_HIGHLIGHT_LINE_BYTES: usize = 2_000;

/// The kinds of tokens that are drawn in colors of their own. Other text is
/// drawn in the color of the view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Token {
    Comment,
    String,
    Keyword,
    /// Numbers and constants of the language, such as `true` and `null`
    Constant,
    Type,
    Function,
}

impl Token {
    pub const ALL: [Token; 6] = [
        Token::Comment,
        Token::String,
        Token::Keyword,
        Token::Constant,
        Token::Type,
        Token::Function,
    ];

    fn index(self) -> u8 {
        Token::ALL.iter().position(|t| *t == self).unwrap() as u8
    }
}

/// The scopes of TextMate grammars that each kind of token covers. A more
/// specific selector wins, so that operators, which are keywords to most
/// grammars, stay plain.
const SELECTORS: &[(&str, Option<Token>)] = &[
    ("comment", Some(Token::Comment)),
    ("string, constant.character", Some(Token::String)),
    (
        "keyword, storage, entity.name.tag, keyword.control",
        Some(Token::Keyword),
    ),
    ("keyword.operator, punctuation", None),
    // Strings and comments keep their color up to their quotes and markers
    ("string punctuation", Some(Token::String)),
    ("comment punctuation", Some(Token::Comment)),
    (
        "constant.numeric, constant.language, constant.other, support.constant, \
         variable.language",
        Some(Token::Constant),
    ),
    (
        "entity.name.type, entity.name.class, entity.name.struct, entity.name.enum, \
         entity.name.trait, entity.name.interface, entity.other.inherited-class, \
         support.type, support.class, storage.type.primitive, storage.type.numeric",
        Some(Token::Type),
    ),
    (
        "entity.name.function, support.function, variable.function, \
         meta.function-call entity.name, entity.other.attribute-name",
        Some(Token::Function),
    ),
];

/// A theme whose foreground colors are not colors but the kinds of tokens:
/// the red of a color is the index of its kind plus one, and zero is plain.
/// It lets the selectors of syntect decide the kind of each scope.
fn token_theme() -> Theme {
    let scopes = SELECTORS
        .iter()
        .map(|(selector, token)| ThemeItem {
            scope: ScopeSelectors::from_str(selector).unwrap(),
            style: StyleModifier {
                foreground: Some(token_color(*token)),
                background: None,
                font_style: None,
            },
        })
        .collect();
    let mut theme = Theme {
        scopes,
        ..Theme::default()
    };
    theme.settings.foreground = Some(token_color(None));
    theme
}

fn token_color(token: Option<Token>) -> Color {
    Color {
        r: token.map_or(0, |t| t.index() + 1),
        g: 0,
        b: 0,
        a: 0xff,
    }
}

fn color_token(color: Color) -> Option<Token> {
    Token::ALL.get((color.r as usize).checked_sub(1)?).copied()
}

struct Grammars {
    syntaxes: SyntaxSet,
    theme: Theme,
}

/// The grammars that come with syntect, loaded the first time they are used.
fn grammars() -> &'static Grammars {
    static GRAMMARS: OnceLock<Grammars> = OnceLock::new();
    GRAMMARS.get_or_init(|| Grammars {
        syntaxes: SyntaxSet::load_defaults_newlines(),
        theme: token_theme(),
    })
}

/// The grammar of a file, found from the extension of its path or its name,
/// such as `Makefile`, or else from its first line, such as `#!/bin/sh`.
fn syntax_of<'a>(syntaxes: &'a SyntaxSet, file: &FileDiff) -> Option<&'a SyntaxReference> {
    let path = file.new_path.as_deref().or(file.old_path.as_deref())?;
    let path = Path::new(path);
    let by_name = |name: Option<&std::ffi::OsStr>| {
        name.and_then(|n| n.to_str())
            .and_then(|n| syntaxes.find_syntax_by_extension(n))
    };
    let found = by_name(path.extension()).or_else(|| by_name(path.file_name()));
    if found.is_some() {
        return found;
    }
    let first = file.hunks.first()?;
    let line = first.lines.first()?;
    let is_first_line = match line.kind {
        LineKind::Removed => line.old_lineno == Some(1),
        _ => line.new_lineno == Some(1),
    };
    let found = is_first_line
        .then(|| syntaxes.find_syntax_by_first_line(&line.content))
        .flatten()?;
    // Plain text has no tokens to color
    (found.name != "Plain Text").then_some(found)
}

/// Parses one version of the lines of a hunk, line by line, and tells the
/// kinds of tokens of each line.
struct Side<'a> {
    grammars: &'a Grammars,
    highlighter: &'a Highlighter<'a>,
    parse: ParseState,
    highlight: HighlightState,
}

impl<'a> Side<'a> {
    fn new(
        grammars: &'a Grammars,
        highlighter: &'a Highlighter<'a>,
        syntax: &SyntaxReference,
    ) -> Side<'a> {
        Side {
            grammars,
            highlighter,
            parse: ParseState::new(syntax),
            highlight: HighlightState::new(highlighter, ScopeStack::new()),
        }
    }

    /// The tokens of the next line of this version, as byte ranges of its
    /// content, joined where neighbors are of one kind. A line that fails to
    /// parse is left plain.
    fn tokens(&mut self, content: &str) -> Vec<(Range<usize>, Token)> {
        // The grammars are those for lines with their line breaks
        let line = format!("{}\n", content);
        let Ok(ops) = self.parse.parse_line(&line, &self.grammars.syntaxes) else {
            return Vec::new();
        };
        let mut tokens: Vec<(Range<usize>, Token)> = Vec::new();
        let iter = RangedHighlightIterator::new(&mut self.highlight, &ops, &line, self.highlighter);
        for (style, _, range) in iter {
            let range = range.start.min(content.len())..range.end.min(content.len());
            let Some(token) = color_token(style.foreground) else {
                continue;
            };
            if range.is_empty() {
                continue;
            }
            match tokens.last_mut() {
                Some((last, last_token)) if last.end == range.start && *last_token == token => {
                    last.end = range.end;
                }
                _ => tokens.push((range, token)),
            }
        }
        tokens
    }
}

/// Finds the tokens of the lines of a hunk. The old version, the context and
/// removed lines, and the new one, the context and added lines, are parsed
/// apart, so that a string or comment over several lines carries on in each.
fn highlight_hunk(
    grammars: &Grammars,
    highlighter: &Highlighter,
    syntax: &SyntaxReference,
    hunk: &mut DiffHunk,
) {
    let mut old = Side::new(grammars, highlighter, syntax);
    let mut new = Side::new(grammars, highlighter, syntax);
    for line in &mut hunk.lines {
        line.syntax = match line.kind {
            LineKind::Removed => old.tokens(&line.content),
            LineKind::Added => new.tokens(&line.content),
            LineKind::Context => {
                old.tokens(&line.content);
                new.tokens(&line.content)
            }
        };
    }
}

impl FileDiff {
    /// Finds the tokens of every line, unless they are found already. Files of
    /// no known language and those with very long lines are left plain.
    pub fn highlight(&mut self) {
        if self.highlighted {
            return;
        }
        self.highlighted = true;
        if self.binary {
            return;
        }
        let too_long = self
            .hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .any(|line| line.content.len() > MAX_HIGHLIGHT_LINE_BYTES);
        if too_long {
            return;
        }
        let grammars = grammars();
        let Some(syntax) = syntax_of(&grammars.syntaxes, self) else {
            return;
        };
        let highlighter = Highlighter::new(&grammars.theme);
        for hunk in &mut self.hunks {
            highlight_hunk(grammars, &highlighter, syntax, hunk);
        }
    }
}

/// A part of the content of a line with one look: whether it changed within
/// its line pair, and its kind of token if it has one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub range: Range<usize>,
    pub emphasized: bool,
    pub token: Option<Token>,
}

impl DiffLine {
    /// The content of the line cut where its emphasis or its tokens start or
    /// end, so that each part can be drawn with its own tags.
    pub fn segments(&self) -> Vec<Segment> {
        let len = self.content.len();
        let mut cuts: Vec<usize> = vec![0, len];
        for range in self
            .emphasis
            .iter()
            .chain(self.syntax.iter().map(|(r, _)| r))
        {
            cuts.push(range.start.min(len));
            cuts.push(range.end.min(len));
        }
        cuts.sort_unstable();
        cuts.dedup();

        cuts.windows(2)
            .map(|w| w[0]..w[1])
            .filter(|range| !range.is_empty())
            .map(|range| Segment {
                emphasized: self
                    .emphasis
                    .iter()
                    .any(|e| e.start <= range.start && range.end <= e.end),
                token: self
                    .syntax
                    .iter()
                    .find(|(t, _)| t.start <= range.start && range.end <= t.end)
                    .map(|(_, token)| *token),
                range,
            })
            .collect()
    }
}
