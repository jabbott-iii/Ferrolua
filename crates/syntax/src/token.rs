//! The lexer's output, one piece at a time: [`Token`] and its [`TokenKind`], and [`Trivia`], the
//! spaces, line breaks, and comments between tokens.
//!
//! These are a module of their own because the parser and the lossless tree read them as well as
//! the lexer, which builds them.

use crate::Span;

/// What a token is: a reserved word, a name, a literal, a part of an f-string, an operator or
/// other symbol, or the end of the source.
///
/// The 24 reserved words are Lua 5.5's 23 and `is`. Every other word, including the contextual
/// keywords `nonlocal`, `class`, `extends`, `type`, and `super`, is a [`TokenKind::Name`], which
/// the parser reads as a keyword where the language spec says.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TokenKind {
    /// The reserved word `and`.
    And,
    /// The reserved word `break`.
    Break,
    /// The reserved word `do`.
    Do,
    /// The reserved word `else`.
    Else,
    /// The reserved word `elseif`.
    Elseif,
    /// The reserved word `end`.
    End,
    /// The reserved word `false`.
    False,
    /// The reserved word `for`.
    For,
    /// The reserved word `function`.
    Function,
    /// The reserved word `global`.
    Global,
    /// The reserved word `goto`.
    Goto,
    /// The reserved word `if`.
    If,
    /// The reserved word `in`.
    In,
    /// The reserved word `is`, which Lunaferra adds to Lua's.
    Is,
    /// The reserved word `local`.
    Local,
    /// The reserved word `nil`.
    Nil,
    /// The reserved word `not`.
    Not,
    /// The reserved word `or`.
    Or,
    /// The reserved word `repeat`.
    Repeat,
    /// The reserved word `return`.
    Return,
    /// The reserved word `then`.
    Then,
    /// The reserved word `true`.
    True,
    /// The reserved word `until`.
    Until,
    /// The reserved word `while`.
    While,

    /// A name: a letter or `_`, then letters, digits, and `_`, in ASCII, that isn't a reserved
    /// word.
    Name,
    /// A numeral whose value is an integer, such as `42` or `0xff`.
    Integer,
    /// A numeral whose value is a float, such as `1.5`, `1e3`, or `0x1p4`, or a decimal integer
    /// too large for an integer.
    Float,
    /// A short string, such as `"a\n"` or `'b'`, or a long string, such as `[==[c]==]`, with its
    /// quotes or brackets.
    String,

    /// The start of an f-string: `f` and its opening quote or long bracket, such as `f"` or
    /// `f[[`, with the line break that follows a long bracket, which isn't part of the text.
    FStringStart,
    /// A stretch of an f-string's text, or of a field's format spec, between the tokens around
    /// it. `{{` and `}}` in the text stand for a brace each, and a short f-string's text keeps
    /// the escapes of a short string.
    FStringText,
    /// The `{` that starts a field of an f-string. The field's expression follows as ordinary
    /// tokens.
    FStringFieldStart,
    /// A conversion in an f-string's field, `!s` or `!r`, at the top level of the field and
    /// just before its `:` or `}`.
    FStringConversion,
    /// The `:` that starts a field's format spec, at the top level of the field and not followed
    /// by a name and `(`, which would make it a method call.
    FStringSpec,
    /// The `}` that ends a field of an f-string.
    FStringFieldEnd,
    /// The end of an f-string: its closing quote or long bracket.
    FStringEnd,

    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `//`
    SlashSlash,
    /// `%`
    Percent,
    /// `**`, power.
    StarStar,
    /// `^`, bitwise XOR.
    Caret,
    /// `#`
    Hash,
    /// `&`
    Ampersand,
    /// `~`
    Tilde,
    /// `|`
    Pipe,
    /// `<<`
    LessLess,
    /// `>>`
    GreaterGreater,
    /// `==`
    EqualEqual,
    /// `!=`
    BangEqual,
    /// `<=`
    LessEqual,
    /// `>=`
    GreaterEqual,
    /// `<`
    Less,
    /// `>`
    Greater,
    /// `=`
    Equal,
    /// `+=`
    PlusEqual,
    /// `-=`
    MinusEqual,
    /// `*=`
    StarEqual,
    /// `/=`
    SlashEqual,
    /// `//=`
    SlashSlashEqual,
    /// `%=`
    PercentEqual,
    /// `**=`
    StarStarEqual,
    /// `&=`
    AmpersandEqual,
    /// `|=`
    PipeEqual,
    /// `^=`
    CaretEqual,
    /// `<<=`
    LessLessEqual,
    /// `>>=`
    GreaterGreaterEqual,
    /// `(`
    LeftParen,
    /// `)`
    RightParen,
    /// `{`
    LeftBrace,
    /// `}`
    RightBrace,
    /// `[`
    LeftBracket,
    /// `]`
    RightBracket,
    /// `::`
    ColonColon,
    /// `;`
    Semicolon,
    /// `:`
    Colon,
    /// `,`
    Comma,
    /// `.`
    Dot,
    /// `...`
    DotDotDot,
    /// `?`
    Question,
    /// `->`
    Arrow,

    /// `..`, Lua's operator that joins two strings, which Lunaferra writes as `+`. The lexer
    /// reads it so that the parser can name the replacement.
    DotDot,
    /// `~=`, Lua's not-equal operator, which Lunaferra writes as `!=`. The lexer reads it so that
    /// the parser can name the replacement.
    TildeEqual,

    /// A byte that starts no token, such as `@`, `$`, a lone `!`, a control character, or a
    /// byte outside ASCII. The parser reports it, as PUC-Rio Lua reports such a byte with
    /// `unexpected symbol`.
    Unknown,
    /// Bytes the lexer couldn't read as a token, such as a malformed number or an unfinished
    /// string. Each error token comes with a [`LexError`](crate::LexError).
    Error,
    /// The end of the source: an empty token after the last byte, whose leading trivia holds
    /// what follows the last token.
    Eof,
}

/// One token: its kind, its bytes, and the trivia before and after it.
///
/// The leading and trailing trivia are in the [`Lexed`](crate::Lexed) that holds the token, which
/// [`Lexed::leading`](crate::Lexed::leading) and [`Lexed::trailing`](crate::Lexed::trailing) read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Token {
    /// What the token is.
    pub kind: TokenKind,
    /// The token's own bytes, without its trivia.
    pub span: Span,
    /// The range of the token's leading trivia in its `Lexed`.
    pub(crate) leading: (u32, u32),
    /// The range of the token's trailing trivia in its `Lexed`.
    pub(crate) trailing: (u32, u32),
}

/// One piece of trivia: bytes between tokens that the parser skips but the lossless tree keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Trivia {
    /// What the trivia is.
    pub kind: TriviaKind,
    /// The trivia's bytes.
    pub span: Span,
}

/// What a piece of [`Trivia`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TriviaKind {
    /// A run of spaces, tabs, vertical tabs, and form feeds.
    Whitespace,
    /// One line break: `\n`, `\r`, `\r\n`, or `\n\r`, as PUC-Rio Lua counts them.
    LineBreak,
    /// A short comment: `--` and the rest of its line, without the line break.
    Comment,
    /// A long comment, such as `--[[ ... ]]` or `--[==[ ... ]==]`.
    LongComment,
    /// The first line of a file when it starts with `#`, such as `#!/usr/bin/env lunaferra`, up
    /// to its first `\n`.
    Shebang,
    /// The UTF-8 byte order mark, the bytes `EF BB BF`, at the start of a file.
    ByteOrderMark,
}
