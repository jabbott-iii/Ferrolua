//! The parser's errors: [`ParseError`], with the message that PUC-Rio Lua 5.5.1 gives for the
//! same mistake. A module of its own because the statement and expression readers both report
//! them, and because each kind needs the name of a token, which this module writes.

use std::fmt;

use crate::{Near, Span, TokenKind};

/// A syntax error: a token the parser doesn't expect where it is, or a statement that breaks a
/// rule PUC-Rio's parser checks, such as two `<close>` variables in one `local` statement.
///
/// The parser reports every syntax error of a source and goes on after each, while PUC-Rio Lua
/// stops at the first. [`source_errors`](crate::source_errors) gives the one PUC-Rio reports, and
/// [`ParseError::diagnostic`] gives this error as a diagnostic for the tools.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// What the mistake is.
    pub kind: ParseErrorKind,
    /// The token where the parser found the error, or, for an attribute's error, which PUC-Rio
    /// writes without `near`, the name at fault: the attribute's name, or the second `<close>`
    /// variable's.
    pub span: Span,
    /// The line that PUC-Rio's message names: the line where the token at which the parser found
    /// the error ends, or, for an attribute's error, where the last token it read ends.
    pub line: u32,
    /// What the message says the error is near, or `None` for an error written without it: an
    /// attribute error and an unexpected zero byte, which PUC-Rio writes so, and Lua's
    /// operators that Lunaferra replaced, whose messages name the operator themselves.
    pub near: Option<Near>,
}

impl ParseError {
    /// Returns the message for this error, as PUC-Rio Lua 5.5.1 writes it after the chunk name
    /// and the line, such as `'end' expected near <eof>`.
    ///
    /// The message is bytes, because the text after `near` is a copy of the source, which need
    /// not be valid UTF-8. As in PUC-Rio's message, that text stops at its first zero byte.
    #[must_use]
    pub fn message(&self) -> Vec<u8> {
        let mut message = self.kind.to_string().into_bytes();
        if let Some(near) = &self.near {
            near.append_to(&mut message);
        }
        message
    }
}

impl fmt::Display for ParseError {
    /// Writes [`ParseError::message`], with each sequence of bytes that isn't valid UTF-8
    /// replaced by U+FFFD, the replacement character.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&String::from_utf8_lossy(&self.message()))
    }
}

impl std::error::Error for ParseError {}

/// The kinds of [`ParseError`], each with the words of its message.
///
/// Every kind but three comes from PUC-Rio Lua 5.5.1's parser, `lparser.c`, with the same words,
/// and [`NestingTooDeep`](ParseErrorKind::NestingTooDeep) from its `lstate.c`. The three that
/// Lunaferra adds report Lua's operators that it replaced with Python's, after an expression, and
/// name the replacement.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ParseErrorKind {
    /// `X expected`, such as `'then' expected` or `<name> expected`: a token of this kind must
    /// come next.
    ///
    /// A reserved word or a symbol is written in quotes, and the other kinds as PUC-Rio writes
    /// them, such as `<name>` and `<eof>`. The kinds that Lunaferra adds, which PUC-Rio has no
    /// name for, are written `<f-string>`, `<f-string text>`, `<conversion>`, `<end of f-string>`,
    /// `<symbol>` for [`TokenKind::Unknown`], and `<error>` for [`TokenKind::Error`].
    Expected(TokenKind),
    /// `X expected (to close Y at line N)`, such as `'end' expected (to close 'if' at line 1)`: the
    /// token that closes a construct must come next, and the construct opened on an earlier line.
    /// When it opened on the same line, the error is [`ParseErrorKind::Expected`] instead.
    Unclosed {
        /// The closing token that must come next.
        expected: TokenKind,
        /// The token that opened the construct.
        opener: TokenKind,
        /// The line where the opening token ends.
        line: u32,
    },
    /// `unexpected symbol`: a token that can't start an expression where one must come.
    UnexpectedSymbol,
    /// `syntax error`: an expression that starts a statement but is neither a call nor the
    /// target of an assignment, or an assignment target that isn't a name, a field, or an index.
    SyntaxError,
    /// `'=' or 'in' expected`: a `for` loop's first name followed by neither.
    EqualOrInExpected,
    /// `<name> or '...' expected`: a parameter list holding something else.
    NameOrVarargExpected,
    /// `function arguments expected`: a method name after `:` without arguments.
    FunctionArgumentsExpected,
    /// `break outside loop`: a `break` that no loop of its function holds.
    BreakOutsideLoop,
    /// `cannot use '...' outside a vararg function`: `...` in a function without `...` among its
    /// parameters.
    VarargOutsideVarargFunction,
    /// `unknown attribute 'x'`: an attribute other than `const` and `close`.
    UnknownAttribute {
        /// The attribute's name.
        name: Vec<u8>,
    },
    /// `multiple to-be-closed variables in local list`: two `<close>` variables in one `local`
    /// statement.
    MultipleToBeClosed,
    /// `global variables cannot be to-be-closed`: a `<close>` attribute in a `global`
    /// declaration.
    GlobalToBeClosed,
    /// `'..' is not an operator: use '+' to join strings`: Lua's operator that joins two
    /// strings, after an expression.
    LuaConcatenation,
    /// `'~=' is not an operator: use '!=' to test inequality`: Lua's not-equal operator, after
    /// an expression.
    LuaNotEqual,
    /// `'~' is not a binary operator: use '^' for bitwise XOR`: Lua's XOR operator, after an
    /// expression. Before an operand, `~` is still bitwise NOT.
    LuaBitwiseXor,
    /// `C stack overflow`: one level of nesting past [`MAX_DEPTH`](crate::MAX_DEPTH), at the
    /// token the parser would have read next.
    ///
    /// PUC-Rio raises it as a runtime error of `load`, not as a syntax error, so its message has
    /// no `near`, and `load` writes it with no chunk name or line before it. The error's
    /// [`line`](ParseError::line) is still where the token ends, for the tools.
    NestingTooDeep,
}

impl fmt::Display for ParseErrorKind {
    /// Writes the words of the message, before `near`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Expected(kind) => write!(f, "{} expected", token_name(*kind)),
            Self::Unclosed {
                expected,
                opener,
                line,
            } => write!(
                f,
                "{} expected (to close {} at line {line})",
                token_name(*expected),
                token_name(*opener),
            ),
            Self::UnexpectedSymbol => f.write_str("unexpected symbol"),
            Self::SyntaxError => f.write_str("syntax error"),
            Self::EqualOrInExpected => f.write_str("'=' or 'in' expected"),
            Self::NameOrVarargExpected => f.write_str("<name> or '...' expected"),
            Self::FunctionArgumentsExpected => f.write_str("function arguments expected"),
            Self::BreakOutsideLoop => f.write_str("break outside loop"),
            Self::VarargOutsideVarargFunction => {
                f.write_str("cannot use '...' outside a vararg function")
            }
            Self::UnknownAttribute { name } => {
                write!(f, "unknown attribute '{}'", String::from_utf8_lossy(name))
            }
            Self::MultipleToBeClosed => {
                f.write_str("multiple to-be-closed variables in local list")
            }
            Self::GlobalToBeClosed => f.write_str("global variables cannot be to-be-closed"),
            Self::LuaConcatenation => {
                f.write_str("'..' is not an operator: use '+' to join strings")
            }
            Self::LuaNotEqual => {
                f.write_str("'~=' is not an operator: use '!=' to test inequality")
            }
            Self::LuaBitwiseXor => {
                f.write_str("'~' is not a binary operator: use '^' for bitwise XOR")
            }
            Self::NestingTooDeep => f.write_str("C stack overflow"),
        }
    }
}

/// Returns the name of `kind` in a message, as PUC-Rio's `luaX_token2str` writes it: a reserved
/// word or a symbol in quotes, such as `'end'`, and the kinds whose text varies in angle brackets,
/// such as `<name>`.
const fn token_name(kind: TokenKind) -> &'static str {
    use TokenKind as T;
    match kind {
        T::And => "'and'",
        T::Break => "'break'",
        T::Do => "'do'",
        T::Else => "'else'",
        T::Elseif => "'elseif'",
        T::End => "'end'",
        T::False => "'false'",
        T::For => "'for'",
        T::Function => "'function'",
        T::Global => "'global'",
        T::Goto => "'goto'",
        T::If => "'if'",
        T::In => "'in'",
        T::Is => "'is'",
        T::Local => "'local'",
        T::Nil => "'nil'",
        T::Not => "'not'",
        T::Or => "'or'",
        T::Repeat => "'repeat'",
        T::Return => "'return'",
        T::Then => "'then'",
        T::True => "'true'",
        T::Until => "'until'",
        T::While => "'while'",
        T::Name => "<name>",
        T::Integer => "<integer>",
        T::Float => "<number>",
        T::String => "<string>",
        T::FStringStart => "<f-string>",
        T::FStringText => "<f-string text>",
        T::FStringFieldStart | T::LeftBrace => "'{'",
        T::FStringConversion => "<conversion>",
        T::FStringSpec | T::Colon => "':'",
        T::FStringFieldEnd | T::RightBrace => "'}'",
        T::FStringEnd => "<end of f-string>",
        T::Plus => "'+'",
        T::Minus => "'-'",
        T::Star => "'*'",
        T::Slash => "'/'",
        T::SlashSlash => "'//'",
        T::Percent => "'%'",
        T::StarStar => "'**'",
        T::Caret => "'^'",
        T::Hash => "'#'",
        T::Ampersand => "'&'",
        T::Tilde => "'~'",
        T::Pipe => "'|'",
        T::LessLess => "'<<'",
        T::GreaterGreater => "'>>'",
        T::EqualEqual => "'=='",
        T::BangEqual => "'!='",
        T::LessEqual => "'<='",
        T::GreaterEqual => "'>='",
        T::Less => "'<'",
        T::Greater => "'>'",
        T::Equal => "'='",
        T::PlusEqual => "'+='",
        T::MinusEqual => "'-='",
        T::StarEqual => "'*='",
        T::SlashEqual => "'/='",
        T::SlashSlashEqual => "'//='",
        T::PercentEqual => "'%='",
        T::StarStarEqual => "'**='",
        T::AmpersandEqual => "'&='",
        T::PipeEqual => "'|='",
        T::CaretEqual => "'^='",
        T::LessLessEqual => "'<<='",
        T::GreaterGreaterEqual => "'>>='",
        T::LeftParen => "'('",
        T::RightParen => "')'",
        T::LeftBracket => "'['",
        T::RightBracket => "']'",
        T::ColonColon => "'::'",
        T::Semicolon => "';'",
        T::Comma => "','",
        T::Dot => "'.'",
        T::DotDotDot => "'...'",
        T::Question => "'?'",
        T::Arrow => "'->'",
        T::DotDot => "'..'",
        T::TildeEqual => "'~='",
        T::Unknown => "<symbol>",
        T::Error => "<error>",
        T::Eof => "<eof>",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Limits, Origin, SourceId, lex};
    use TokenKind as T;

    /// Every token kind, with its name in messages.
    const NAMES: [(TokenKind, &str); 87] = [
        (T::And, "'and'"),
        (T::Break, "'break'"),
        (T::Do, "'do'"),
        (T::Else, "'else'"),
        (T::Elseif, "'elseif'"),
        (T::End, "'end'"),
        (T::False, "'false'"),
        (T::For, "'for'"),
        (T::Function, "'function'"),
        (T::Global, "'global'"),
        (T::Goto, "'goto'"),
        (T::If, "'if'"),
        (T::In, "'in'"),
        (T::Is, "'is'"),
        (T::Local, "'local'"),
        (T::Nil, "'nil'"),
        (T::Not, "'not'"),
        (T::Or, "'or'"),
        (T::Repeat, "'repeat'"),
        (T::Return, "'return'"),
        (T::Then, "'then'"),
        (T::True, "'true'"),
        (T::Until, "'until'"),
        (T::While, "'while'"),
        (T::Name, "<name>"),
        (T::Integer, "<integer>"),
        (T::Float, "<number>"),
        (T::String, "<string>"),
        (T::FStringStart, "<f-string>"),
        (T::FStringText, "<f-string text>"),
        (T::FStringFieldStart, "'{'"),
        (T::FStringConversion, "<conversion>"),
        (T::FStringSpec, "':'"),
        (T::FStringFieldEnd, "'}'"),
        (T::FStringEnd, "<end of f-string>"),
        (T::Plus, "'+'"),
        (T::Minus, "'-'"),
        (T::Star, "'*'"),
        (T::Slash, "'/'"),
        (T::SlashSlash, "'//'"),
        (T::Percent, "'%'"),
        (T::StarStar, "'**'"),
        (T::Caret, "'^'"),
        (T::Hash, "'#'"),
        (T::Ampersand, "'&'"),
        (T::Tilde, "'~'"),
        (T::Pipe, "'|'"),
        (T::LessLess, "'<<'"),
        (T::GreaterGreater, "'>>'"),
        (T::EqualEqual, "'=='"),
        (T::BangEqual, "'!='"),
        (T::LessEqual, "'<='"),
        (T::GreaterEqual, "'>='"),
        (T::Less, "'<'"),
        (T::Greater, "'>'"),
        (T::Equal, "'='"),
        (T::PlusEqual, "'+='"),
        (T::MinusEqual, "'-='"),
        (T::StarEqual, "'*='"),
        (T::SlashEqual, "'/='"),
        (T::SlashSlashEqual, "'//='"),
        (T::PercentEqual, "'%='"),
        (T::StarStarEqual, "'**='"),
        (T::AmpersandEqual, "'&='"),
        (T::PipeEqual, "'|='"),
        (T::CaretEqual, "'^='"),
        (T::LessLessEqual, "'<<='"),
        (T::GreaterGreaterEqual, "'>>='"),
        (T::LeftParen, "'('"),
        (T::RightParen, "')'"),
        (T::LeftBrace, "'{'"),
        (T::RightBrace, "'}'"),
        (T::LeftBracket, "'['"),
        (T::RightBracket, "']'"),
        (T::ColonColon, "'::'"),
        (T::Semicolon, "';'"),
        (T::Colon, "':'"),
        (T::Comma, "','"),
        (T::Dot, "'.'"),
        (T::DotDotDot, "'...'"),
        (T::Question, "'?'"),
        (T::Arrow, "'->'"),
        (T::DotDot, "'..'"),
        (T::TildeEqual, "'~='"),
        (T::Unknown, "<symbol>"),
        (T::Error, "<error>"),
        (T::Eof, "<eof>"),
    ];

    #[test]
    fn names_each_token_kind_as_puc_rio_writes_it() {
        for (kind, name) in NAMES {
            assert_eq!(token_name(kind), name, "{kind:?}");
        }
    }

    #[test]
    fn names_each_symbol_and_reserved_word_by_the_text_that_lexes_as_it() {
        for (kind, name) in NAMES {
            let Some(text) = name
                .strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
            else {
                continue;
            };
            let lexed = lex(
                text.as_bytes(),
                SourceId::new(0),
                Origin::String,
                &Limits::default(),
            )
            .unwrap();
            let first = lexed.tokens()[0];
            let fstring_part = matches!(
                kind,
                T::FStringFieldStart | T::FStringSpec | T::FStringFieldEnd
            );
            if !fstring_part {
                assert_eq!(first.kind, kind, "{text}");
            }
            assert_eq!(lexed.tokens().len(), 2, "{text} is one token");
        }
    }
}
