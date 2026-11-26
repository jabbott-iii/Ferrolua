//! The lexer: [`lex`] splits a source into [`Token`]s, keeps the [`Trivia`] around each one, and
//! reports each [`LexError`] with the message PUC-Rio Lua 5.5.1 gives.
//!
//! It reads Lua 5.5's tokens, apart from `..` and `~=`, which it reads only so that the parser
//! can name their replacements, and Lunaferra's own: `**`, `!=`, the compound assignments, `?`,
//! `->`, and f-strings. An f-string comes out as a run of tokens: its start, stretches of text,
//! and for each field a start, the field's expression as ordinary tokens, an optional conversion
//! and format spec, and an end. The lexer tracks which f-strings, fields, and specs it is
//! inside with a stack, not with recursion, so deep nesting can't overflow the call stack.

mod error;
mod escape;
mod numeral;

pub use error::{LexError, LexErrorKind, Near};
pub use numeral::{Number, numeral_value};

use crate::{Error, LineMap, SourceId, Span, Token, TokenKind, Trivia, TriviaKind};
use escape::{
    Bracket, closes, find_close, line_break_end, long_body_start, open_bracket, read_escape,
};
use numeral::numeral_end;

/// Where a source comes from, which decides whether the lexer skips its first bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Origin {
    /// A file, as `lua`, `dofile`, `loadfile`, and `require` read one. As PUC-Rio's
    /// `luaL_loadfilex` does, the lexer skips a UTF-8 byte order mark at the start, then a first
    /// line that starts with `#`, such as `#!/usr/bin/env lunaferra`, up to its first `\n`. That
    /// line counts as one line, whatever it holds, so a `\r` in it ends no line.
    File,
    /// A string, as `load` reads one: its first byte is code, so `#` there is the length
    /// operator, as in PUC-Rio Lua.
    String,
}

/// The tokens, trivia, and errors of one source, and its line map.
///
/// Every byte of the source belongs to exactly one token or one piece of trivia, so writing out
/// each token's leading trivia, its bytes, and its trailing trivia, in order, gives back the
/// source. The last token is always [`TokenKind::Eof`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lexed {
    tokens: Vec<Token>,
    trivia: Vec<Trivia>,
    errors: Vec<LexError>,
    lines: LineMap,
}

impl Lexed {
    /// Returns the tokens, in the order of the source.
    #[must_use]
    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    /// Returns all the trivia, in the order of the source.
    #[must_use]
    pub fn trivia(&self) -> &[Trivia] {
        &self.trivia
    }

    /// Returns the trivia before `token`, one of this source's tokens: everything between it and
    /// the previous token's trailing trivia.
    #[must_use]
    pub fn leading(&self, token: &Token) -> &[Trivia] {
        self.trivia_in(token.leading)
    }

    /// Returns the trivia after `token`, one of this source's tokens, on the same line: the
    /// spaces and comments up to the next line break or token.
    ///
    /// A token inside an f-string's text, or one that starts a stretch of it, has none, because
    /// the spaces there are text.
    #[must_use]
    pub fn trailing(&self, token: &Token) -> &[Trivia] {
        self.trivia_in(token.trailing)
    }

    /// Returns the errors, in the order of the source. PUC-Rio Lua reports only the first.
    #[must_use]
    pub fn errors(&self) -> &[LexError] {
        &self.errors
    }

    /// Returns the source's line map, which gives each offset the line PUC-Rio Lua counts for
    /// it, including after a skipped first line of a file.
    #[must_use]
    pub fn lines(&self) -> &LineMap {
        &self.lines
    }

    fn trivia_in(&self, (start, end): (u32, u32)) -> &[Trivia] {
        self.trivia.get(index(start)..index(end)).unwrap_or(&[])
    }
}

/// Splits `source`, the source numbered `id`, into tokens and trivia, as [`Origin`] says to read
/// it.
///
/// The lexer never panics. When it can't read a token, it records a [`LexError`], makes the
/// bytes a [`TokenKind::Error`] token, and goes on.
///
/// # Errors
///
/// Returns [`Error::SourceTooLong`] if `source` is longer than
/// [`MAX_SOURCE_LEN`](crate::MAX_SOURCE_LEN) bytes.
///
/// # Examples
///
/// ```
/// use lunaferra_syntax::{Origin, SourceId, TokenKind, lex};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let lexed = lex(b"x = 2 ** 10 -- a comment\n", SourceId::new(0), Origin::String)?;
/// let kinds: Vec<TokenKind> = lexed.tokens().iter().map(|token| token.kind).collect();
/// use TokenKind::*;
/// assert_eq!(kinds, [Name, Equal, Integer, StarStar, Integer, Eof]);
///
/// let broken = lex(b"x = 0x", SourceId::new(1), Origin::String)?;
/// assert_eq!(broken.errors()[0].to_string(), "malformed number near '0x'");
/// # Ok(())
/// # }
/// ```
pub fn lex(source: &[u8], id: SourceId, origin: Origin) -> Result<Lexed, Error> {
    let (mark, first_line) = match origin {
        Origin::File => file_prefix(source),
        Origin::String => (0, 0),
    };
    let lines = LineMap::counting_from(source, first_line)?;
    let mut lexer = Lexer {
        source,
        id,
        lines,
        pos: 0,
        tokens: Vec::new(),
        trivia: Vec::new(),
        errors: Vec::new(),
        nest: Vec::new(),
        leading_start: 0,
        stretch_start: 0,
        buffer: Vec::new(),
    };
    lexer.skip_prefix(mark, first_line);
    lexer.run();
    Ok(Lexed {
        tokens: lexer.tokens,
        trivia: lexer.trivia,
        errors: lexer.errors,
        lines: lexer.lines,
    })
}

/// Returns where a file's byte order mark ends, and where its first line ends if that line
/// starts with `#`: at its first `\n`, or the end of the file. Without a `#` line, both are the
/// same offset.
fn file_prefix(source: &[u8]) -> (usize, usize) {
    let mark = if source.starts_with(&[0xEF, 0xBB, 0xBF]) {
        3
    } else {
        0
    };
    let rest = source.get(mark..).unwrap_or(&[]);
    if rest.first() != Some(&b'#') {
        return (mark, mark);
    }
    let line = rest
        .iter()
        .position(|&byte| byte == b'\n')
        .unwrap_or(rest.len());
    (mark, mark.saturating_add(line))
}

/// Converts an offset or a count to a `u32`. Every offset and count fits, because [`lex`]
/// refuses a source longer than [`MAX_SOURCE_LEN`](crate::MAX_SOURCE_LEN) bytes.
fn offset(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// Converts a `u32` offset or count back to a `usize`, for the lexer and the trees alike.
pub(crate) fn index(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

/// The f-string that a stretch of text, a field, or a spec belongs to.
#[derive(Clone, Copy, Debug)]
struct FString {
    /// The offset of its `f`.
    start: usize,
    /// What closes it.
    close: Close,
    /// The line of its `f`, which an unfinished long f-string's message names.
    start_line: u32,
}

/// What closes an f-string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Close {
    /// The quote that opened it, `"` or `'`.
    Quote(u8),
    /// A closing long bracket with this many `=` signs.
    Long(usize),
}

/// One level of the stack of f-strings, fields, and specs the lexer is inside.
#[derive(Clone, Copy, Debug)]
enum Frame {
    /// The text of an f-string.
    Text(FString),
    /// A field's expression, with the number of brackets open in it.
    Field { depth: u32, fstring: FString },
    /// A field's format spec.
    Spec(FString),
}

/// Which part of an f-string a stretch of text is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    /// The text between fields, where `{{` and `}}` stand for braces.
    Text,
    /// A format spec, where `{` starts a field and `}` ends the spec's field.
    Spec,
}

/// How far a run of trivia may reach.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reach {
    /// Every piece of trivia: the leading trivia of a token.
    Lines,
    /// Up to the next line break: the trailing trivia of a token.
    SameLine,
}

/// The state of a lexer over one source.
struct Lexer<'s> {
    source: &'s [u8],
    id: SourceId,
    lines: LineMap,
    /// The offset of the next byte to read.
    pos: usize,
    tokens: Vec<Token>,
    trivia: Vec<Trivia>,
    errors: Vec<LexError>,
    /// The f-strings, fields, and specs the lexer is inside, the innermost last.
    nest: Vec<Frame>,
    /// Where the next token's leading trivia starts in `trivia`.
    leading_start: usize,
    /// The offset of the token that began the current stretch of f-string text: the f-string's
    /// start, the `}` of the field before it, or the `:` of its spec.
    stretch_start: usize,
    /// The bytes of the short string being read, as PUC-Rio's lexer keeps them for its
    /// messages: the opening quote, then what each byte and escape so far stands for.
    buffer: Vec<u8>,
}

impl Lexer<'_> {
    /// Records the byte order mark that ends at `mark` and the `#` line that ends at
    /// `first_line` as the first token's leading trivia.
    fn skip_prefix(&mut self, mark: usize, first_line: usize) {
        if mark > 0 {
            self.push_trivia(TriviaKind::ByteOrderMark, 0, mark);
        }
        if first_line > mark {
            self.push_trivia(TriviaKind::Shebang, mark, first_line);
        }
        self.pos = first_line;
    }

    /// Reads the whole source.
    fn run(&mut self) {
        loop {
            match self.nest.last().copied() {
                Some(Frame::Text(fstring)) => self.stretch(Part::Text, fstring),
                Some(Frame::Spec(fstring)) => self.stretch(Part::Spec, fstring),
                Some(Frame::Field { .. }) | None => {
                    if !self.code_token() {
                        return;
                    }
                }
            }
        }
    }

    /// Reads the trivia and the token at `pos` in code, outside f-string text. Returns `false`
    /// after the end of the source.
    fn code_token(&mut self) -> bool {
        self.scan_trivia(Reach::Lines);
        let start = self.pos;
        let Some(&byte) = self.source.get(start) else {
            if let Some(Frame::Field { fstring, .. }) = self.nest.last().copied() {
                self.unfinished_at_end(fstring);
            }
            self.nest.clear();
            self.push_token(TokenKind::Eof, start);
            return false;
        };
        let kind = match self.nest.last().copied() {
            Some(Frame::Field { depth: 0, fstring }) => self
                .field_top_level(byte, fstring)
                .unwrap_or_else(|| self.ordinary(byte)),
            _ => self.ordinary(byte),
        };
        self.push_token(kind, start);
        true
    }

    /// Reads the tokens that mean something of their own at the top level of an f-string's
    /// field: the `}` that ends it, the `:` that starts its spec, and a conversion. Returns
    /// `None` for any other token.
    fn field_top_level(&mut self, byte: u8, fstring: FString) -> Option<TokenKind> {
        let start = self.pos;
        match byte {
            b'}' => {
                self.pos = start.saturating_add(1);
                self.nest.pop();
                self.stretch_start = start;
                Some(TokenKind::FStringFieldEnd)
            }
            // A `:` that starts a spec never joins the next `:` into `::`, so `{x::>8}` has the
            // spec `:>8`, as in Python.
            b':' if !self.method_call_follows() => {
                self.pos = start.saturating_add(1);
                self.nest.push(Frame::Spec(fstring));
                self.stretch_start = start;
                Some(TokenKind::FStringSpec)
            }
            b'!' if self.conversion_follows() => {
                self.pos = start.saturating_add(2);
                Some(TokenKind::FStringConversion)
            }
            _ => None,
        }
    }

    /// Returns whether a name that isn't a reserved word, then `(`, touch the `:` at `pos`, as
    /// in the method call `{p:length()}`.
    fn method_call_follows(&self) -> bool {
        let rest = self.source.get(self.pos.saturating_add(1)..).unwrap_or(&[]);
        let (word, after) = rest.split_at(name_len(rest));
        !word.is_empty() && keyword(word).is_none() && after.first() == Some(&b'(')
    }

    /// Returns whether `s` or `r`, then `:` or `}`, follow the `!` at `pos`.
    fn conversion_follows(&self) -> bool {
        let rest = self.source.get(self.pos.saturating_add(1)..).unwrap_or(&[]);
        matches!(rest, [b's' | b'r', b':' | b'}', ..])
    }

    /// Reads a token that means the same in code and in an f-string's field.
    fn ordinary(&mut self, byte: u8) -> TokenKind {
        use TokenKind as T;
        let start = self.pos;
        match byte {
            b'+' => self.longest(&[(b"+=", T::PlusEqual)], T::Plus),
            // `--` never gets here: it starts a comment, which is trivia.
            b'-' => self.longest(&[(b"-=", T::MinusEqual), (b"->", T::Arrow)], T::Minus),
            b'*' => self.longest(
                &[
                    (b"**=", T::StarStarEqual),
                    (b"**", T::StarStar),
                    (b"*=", T::StarEqual),
                ],
                T::Star,
            ),
            b'/' => self.longest(
                &[
                    (b"//=", T::SlashSlashEqual),
                    (b"//", T::SlashSlash),
                    (b"/=", T::SlashEqual),
                ],
                T::Slash,
            ),
            b'%' => self.longest(&[(b"%=", T::PercentEqual)], T::Percent),
            b'^' => self.longest(&[(b"^=", T::CaretEqual)], T::Caret),
            b'&' => self.longest(&[(b"&=", T::AmpersandEqual)], T::Ampersand),
            b'|' => self.longest(&[(b"|=", T::PipeEqual)], T::Pipe),
            b'~' => self.longest(&[(b"~=", T::TildeEqual)], T::Tilde),
            b'<' => self.longest(
                &[
                    (b"<<=", T::LessLessEqual),
                    (b"<<", T::LessLess),
                    (b"<=", T::LessEqual),
                ],
                T::Less,
            ),
            b'>' => self.longest(
                &[
                    (b">>=", T::GreaterGreaterEqual),
                    (b">>", T::GreaterGreater),
                    (b">=", T::GreaterEqual),
                ],
                T::Greater,
            ),
            b'=' => self.longest(&[(b"==", T::EqualEqual)], T::Equal),
            b'!' => self.longest(&[(b"!=", T::BangEqual)], T::Unknown),
            b':' => self.longest(&[(b"::", T::ColonColon)], T::Colon),
            b'#' => self.longest(&[], T::Hash),
            b';' => self.longest(&[], T::Semicolon),
            b',' => self.longest(&[], T::Comma),
            b'?' => self.longest(&[], T::Question),
            b'(' => self.bracket(1, T::LeftParen),
            b'{' => self.bracket(1, T::LeftBrace),
            b')' => self.bracket(-1, T::RightParen),
            b']' => self.bracket(-1, T::RightBracket),
            b'}' => self.bracket(-1, T::RightBrace),
            b'[' => self.left_bracket(start),
            b'.' => self.dot(start),
            b'0'..=b'9' => self.numeral(start, start),
            b'"' | b'\'' => self.short_string(byte),
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.name(start),
            _ => self.longest(&[], T::Unknown),
        }
    }

    /// Reads the first of `longer`, each a spelling and its kind, that the source has at
    /// `pos`, or else one byte as `single`.
    fn longest(&mut self, longer: &[(&[u8], TokenKind)], single: TokenKind) -> TokenKind {
        let rest = self.source.get(self.pos..).unwrap_or(&[]);
        for &(spelling, kind) in longer {
            if rest.starts_with(spelling) {
                self.pos = self.pos.saturating_add(spelling.len());
                return kind;
            }
        }
        self.pos = self.pos.saturating_add(1);
        single
    }

    /// Reads a one-byte bracket as `kind`, and opens (`change` 1) or closes (`change` -1) a
    /// bracket in the field the lexer is in, if any.
    fn bracket(&mut self, change: i8, kind: TokenKind) -> TokenKind {
        self.pos = self.pos.saturating_add(1);
        if let Some(Frame::Field { depth, .. }) = self.nest.last_mut() {
            *depth = if change > 0 {
                depth.saturating_add(1)
            } else {
                depth.saturating_sub(1)
            };
        }
        kind
    }

    /// Reads a `[`: a long string, or a bracket alone.
    fn left_bracket(&mut self, start: usize) -> TokenKind {
        match open_bracket(self.source, start) {
            Bracket::Long(level) => self.long_string(start, level),
            Bracket::Single => self.bracket(1, TokenKind::LeftBracket),
            Bracket::Unfinished(end) => {
                self.pos = end;
                let near = self.source.get(start..end).unwrap_or(&[]).to_vec();
                self.report(
                    LexErrorKind::InvalidLongStringDelimiter,
                    start,
                    end,
                    end,
                    Near::Text(near),
                );
                TokenKind::Error
            }
        }
    }

    /// Reads the long string that opens at `start` with `level` `=` signs.
    fn long_string(&mut self, start: usize, level: usize) -> TokenKind {
        let body = long_body_start(self.source, start, level);
        if let Some(end) = find_close(self.source, body, level) {
            self.pos = end;
            return TokenKind::String;
        }
        let end = self.source.len();
        self.pos = end;
        let start_line = self.line(start);
        self.report(
            LexErrorKind::UnfinishedLongString { start_line },
            start,
            end,
            end,
            Near::Eof,
        );
        TokenKind::Error
    }

    /// Reads a token that starts with `.`: `...`, `..`, a numeral such as `.5`, or `.`.
    fn dot(&mut self, start: usize) -> TokenKind {
        let rest = self.source.get(start..).unwrap_or(&[]);
        let (kind, len) = match rest {
            [b'.', b'.', b'.', ..] => (TokenKind::DotDotDot, 3),
            [b'.', b'.', ..] => (TokenKind::DotDot, 2),
            [b'.', b'0'..=b'9', ..] => return self.numeral(start, start.saturating_add(1)),
            _ => (TokenKind::Dot, 1),
        };
        self.pos = start.saturating_add(len);
        kind
    }

    /// Reads the numeral that starts at `start`, whose first digit is at `digit`.
    fn numeral(&mut self, start: usize, digit: usize) -> TokenKind {
        let end = numeral_end(self.source, digit);
        self.pos = end;
        let text = self.source.get(start..end).unwrap_or(&[]);
        match numeral_value(text) {
            Some(Number::Integer(_)) => TokenKind::Integer,
            Some(Number::Float(_)) => TokenKind::Float,
            None => {
                let near = Near::Text(text.to_vec());
                self.report(LexErrorKind::MalformedNumber, start, end, end, near);
                TokenKind::Error
            }
        }
    }

    /// Reads the short string that opens at `pos` with `quote`, as PUC-Rio's `read_string`
    /// does. After an error, it reads on from the byte that failed to the string's end, so the
    /// next token starts where it would have, and reports nothing more in the string.
    fn short_string(&mut self, quote: u8) -> TokenKind {
        let start = self.pos;
        self.buffer.clear();
        self.buffer.push(quote);
        self.pos = start.saturating_add(1);
        let mut failed = false;
        loop {
            let at = self.pos;
            match self.source.get(at) {
                None | Some(b'\n' | b'\r') => {
                    if !failed {
                        let near = if at < self.source.len() {
                            Near::Text(self.buffer.clone())
                        } else {
                            Near::Eof
                        };
                        self.report(LexErrorKind::UnfinishedString, start, at, at, near);
                    }
                    return TokenKind::Error;
                }
                Some(&byte) if byte == quote => {
                    self.pos = at.saturating_add(1);
                    return if failed {
                        TokenKind::Error
                    } else {
                        TokenKind::String
                    };
                }
                Some(b'\\') => match read_escape(self.source, at, &mut self.buffer) {
                    Ok(end) => self.pos = end,
                    Err(escape) => {
                        if !failed {
                            let mut near = self.buffer.clone();
                            near.extend_from_slice(self.source.get(at..escape.end).unwrap_or(&[]));
                            let near = Near::Text(near);
                            self.report(escape.kind, start, escape.end, escape.at, near);
                        }
                        failed = true;
                        // Go on from the byte that failed, so the quote in `"\x"` still ends
                        // the string.
                        self.pos = escape.at;
                    }
                },
                Some(&byte) => {
                    self.buffer.push(byte);
                    self.pos = at.saturating_add(1);
                }
            }
        }
    }

    /// Reads the name or reserved word that starts at `start`, or the start of an f-string when
    /// the name is `f` and a quote or a long bracket touches it.
    fn name(&mut self, start: usize) -> TokenKind {
        let rest = self.source.get(start..).unwrap_or(&[]);
        let (word, _) = rest.split_at(name_len(rest));
        self.pos = start.saturating_add(word.len());
        if word == b"f"
            && let Some(kind) = self.fstring_start(start)
        {
            return kind;
        }
        keyword(word).unwrap_or(TokenKind::Name)
    }

    /// Reads the opening quote or long bracket after the `f` at `start`, if one touches it, and
    /// enters the f-string's text.
    fn fstring_start(&mut self, start: usize) -> Option<TokenKind> {
        let open = self.pos;
        let close = match self.source.get(open) {
            Some(&quote @ (b'"' | b'\'')) => {
                self.pos = open.saturating_add(1);
                Close::Quote(quote)
            }
            Some(b'[') => match open_bracket(self.source, open) {
                Bracket::Long(level) => {
                    self.pos = long_body_start(self.source, open, level);
                    Close::Long(level)
                }
                Bracket::Single | Bracket::Unfinished(_) => return None,
            },
            _ => return None,
        };
        let start_line = self.line(start);
        self.nest.push(Frame::Text(FString {
            start,
            close,
            start_line,
        }));
        self.stretch_start = start;
        Some(TokenKind::FStringStart)
    }

    /// Reads one stretch of an f-string's text or spec, and the token that ends it: a field's
    /// start or end, or the f-string's end.
    fn stretch(&mut self, part: Part, fstring: FString) {
        let start = self.pos;
        let short = matches!(fstring.close, Close::Quote(_));
        let mut failed = false;
        loop {
            let at = self.pos;
            let rest = self.source.get(at..).unwrap_or(&[]);
            if let Some(close_len) = closing_len(rest, fstring.close) {
                self.end_stretch(start, at, failed);
                self.leave_fstring();
                self.pos = at.saturating_add(close_len);
                self.push_token(TokenKind::FStringEnd, at);
                return;
            }
            match rest {
                [] => {
                    self.end_stretch(start, at, true);
                    if !failed {
                        self.unfinished_at_end(fstring);
                    }
                    self.nest.clear();
                    return;
                }
                [b'{', b'{', ..] | [b'}', b'}', ..] if part == Part::Text => {
                    self.pos = at.saturating_add(2);
                }
                [b'{', ..] => {
                    self.end_stretch(start, at, failed);
                    self.nest.push(Frame::Field { depth: 0, fstring });
                    self.pos = at.saturating_add(1);
                    self.push_token(TokenKind::FStringFieldStart, at);
                    return;
                }
                [b'}', ..] if part == Part::Spec => {
                    self.end_stretch(start, at, failed);
                    // The spec, then the field it belongs to.
                    self.nest.pop();
                    self.nest.pop();
                    self.pos = at.saturating_add(1);
                    self.stretch_start = at;
                    self.push_token(TokenKind::FStringFieldEnd, at);
                    return;
                }
                [b'}', ..] => {
                    self.pos = at.saturating_add(1);
                    if !failed {
                        self.stretch_error(LexErrorKind::SingleClosingBrace, self.pos, at);
                    }
                    failed = true;
                }
                [b'\\', ..] if short => {
                    self.buffer.clear();
                    match read_escape(self.source, at, &mut self.buffer) {
                        Ok(end) => self.pos = end,
                        Err(escape) => {
                            if !failed {
                                self.stretch_error(escape.kind, escape.end, escape.at);
                            }
                            failed = true;
                            self.pos = escape.at;
                        }
                    }
                }
                [b'\n' | b'\r', ..] if short => {
                    self.end_stretch(start, at, true);
                    if !failed {
                        self.stretch_error(LexErrorKind::UnfinishedString, at, at);
                    }
                    self.leave_fstring();
                    return;
                }
                [_, ..] => self.pos = at.saturating_add(1),
            }
        }
    }

    /// Records the text from `start` to `end`, if any, as one token: an error token if the
    /// stretch had an error, or else f-string text.
    fn end_stretch(&mut self, start: usize, end: usize, failed: bool) {
        if end > start {
            let kind = if failed {
                TokenKind::Error
            } else {
                TokenKind::FStringText
            };
            self.push_token(kind, start);
        }
    }

    /// Leaves the innermost f-string, and every field and spec inside it.
    fn leave_fstring(&mut self) {
        while let Some(frame) = self.nest.pop() {
            if matches!(frame, Frame::Text(_)) {
                break;
            }
        }
    }

    /// Records an error in a stretch of f-string text, which the lexer found at `at` and read
    /// up to `end`. The message is near the source from the stretch's start up to `end`.
    fn stretch_error(&mut self, kind: LexErrorKind, end: usize, at: usize) {
        let start = self.stretch_start;
        let near = self.source.get(start..end).unwrap_or(&[]).to_vec();
        self.report(kind, start, end, at, Near::Text(near));
    }

    /// Records the error for an f-string that the source ends inside.
    fn unfinished_at_end(&mut self, fstring: FString) {
        let kind = match fstring.close {
            Close::Quote(_) => LexErrorKind::UnfinishedString,
            Close::Long(_) => LexErrorKind::UnfinishedLongString {
                start_line: fstring.start_line,
            },
        };
        let end = self.source.len();
        self.report(kind, fstring.start, end, end, Near::Eof);
    }

    /// Reads trivia from `pos`, as far as `reach` allows.
    fn scan_trivia(&mut self, reach: Reach) {
        loop {
            let start = self.pos;
            let rest = self.source.get(start..).unwrap_or(&[]);
            let kind = match rest {
                [b' ' | b'\t' | 0x0B | 0x0C, ..] => {
                    let spaces = rest
                        .iter()
                        .take_while(|&&byte| matches!(byte, b' ' | b'\t' | 0x0B | 0x0C))
                        .count();
                    self.pos = start.saturating_add(spaces);
                    TriviaKind::Whitespace
                }
                [b'\n' | b'\r', ..] if reach == Reach::Lines => {
                    self.pos = line_break_end(self.source, start);
                    TriviaKind::LineBreak
                }
                [b'-', b'-', ..] => self.comment(start),
                _ => return,
            };
            self.push_trivia(kind, start, self.pos);
        }
    }

    /// Reads the comment whose `--` is at `start`: a long comment if a long bracket follows,
    /// and otherwise a short one, to the end of the line.
    fn comment(&mut self, start: usize) -> TriviaKind {
        let bracket = start.saturating_add(2);
        if self.source.get(bracket) == Some(&b'[')
            && let Bracket::Long(level) = open_bracket(self.source, bracket)
        {
            let body = long_body_start(self.source, bracket, level);
            if let Some(end) = find_close(self.source, body, level) {
                self.pos = end;
            } else {
                let end = self.source.len();
                self.pos = end;
                let start_line = self.line(start);
                let kind = LexErrorKind::UnfinishedLongComment { start_line };
                self.report(kind, start, end, end, Near::Eof);
            }
            return TriviaKind::LongComment;
        }
        let rest = self.source.get(bracket..).unwrap_or(&[]);
        let text = rest
            .iter()
            .take_while(|&&byte| byte != b'\n' && byte != b'\r')
            .count();
        self.pos = bracket.saturating_add(text);
        TriviaKind::Comment
    }

    /// Records a piece of trivia from `start` to `end`.
    fn push_trivia(&mut self, kind: TriviaKind, start: usize, end: usize) {
        let span = self.span(start, end);
        self.trivia.push(Trivia { kind, span });
    }

    /// Records the token from `start` to `pos`, with the trivia read since the last token as
    /// its leading trivia. In code, it then reads the token's trailing trivia.
    fn push_token(&mut self, kind: TokenKind, start: usize) {
        let span = self.span(start, self.pos);
        let leading = (offset(self.leading_start), offset(self.trivia.len()));
        if matches!(self.nest.last(), None | Some(Frame::Field { .. })) {
            self.scan_trivia(Reach::SameLine);
        }
        let trailing = (leading.1, offset(self.trivia.len()));
        self.tokens.push(Token {
            kind,
            span,
            leading,
            trailing,
        });
        self.leading_start = self.trivia.len();
    }

    /// Records an error of `kind` over the bytes from `start` to `end`, on the line of `at`.
    fn report(&mut self, kind: LexErrorKind, start: usize, end: usize, at: usize, near: Near) {
        let span = self.span(start, end);
        let line = self.line(at);
        self.errors.push(LexError {
            kind,
            span,
            line,
            near,
        });
    }

    fn span(&self, start: usize, end: usize) -> Span {
        Span {
            source: self.id,
            start: offset(start),
            end: offset(end),
        }
    }

    fn line(&self, at: usize) -> u32 {
        self.lines.line_col(offset(at)).0
    }
}

/// Returns the length of the closing quote or long bracket of an f-string closed by `close`, if
/// `rest` starts with it.
fn closing_len(rest: &[u8], close: Close) -> Option<usize> {
    match close {
        Close::Quote(quote) => (rest.first() == Some(&quote)).then_some(1),
        Close::Long(level) => closes(rest, 0, level).then_some(level.saturating_add(2)),
    }
}

/// Returns the length of the name at the start of `rest`: a letter or `_`, then letters,
/// digits, and `_`, all in ASCII. Returns 0 if `rest` doesn't start with a name.
fn name_len(rest: &[u8]) -> usize {
    match rest.first() {
        Some(byte) if byte.is_ascii_alphabetic() || *byte == b'_' => rest
            .iter()
            .take_while(|byte| byte.is_ascii_alphanumeric() || **byte == b'_')
            .count(),
        _ => 0,
    }
}

/// Returns the reserved word that `word` spells, if any.
fn keyword(word: &[u8]) -> Option<TokenKind> {
    use TokenKind as T;
    let kind = match word {
        b"and" => T::And,
        b"break" => T::Break,
        b"do" => T::Do,
        b"else" => T::Else,
        b"elseif" => T::Elseif,
        b"end" => T::End,
        b"false" => T::False,
        b"for" => T::For,
        b"function" => T::Function,
        b"global" => T::Global,
        b"goto" => T::Goto,
        b"if" => T::If,
        b"in" => T::In,
        b"is" => T::Is,
        b"local" => T::Local,
        b"nil" => T::Nil,
        b"not" => T::Not,
        b"or" => T::Or,
        b"repeat" => T::Repeat,
        b"return" => T::Return,
        b"then" => T::Then,
        b"true" => T::True,
        b"until" => T::Until,
        b"while" => T::While,
        _ => return None,
    };
    Some(kind)
}
