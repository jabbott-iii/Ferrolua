//! Expressions, read by precedence climbing with the precedence table of the language spec, which
//! is Python's: conditional expressions, `or`, `and`, `not`, chained comparisons, the bitwise
//! operators, `+` and `-`, `*`, `/`, `//`, and `%`, the unary operators, and `**`. Their operands
//! are literals, `...`, f-strings, function expressions, table constructors, lists, tuples, and
//! the names and parenthesized expressions that fields, indexes, calls, and method calls follow,
//! which PUC-Rio Lua 5.5.1's `suffixedexp` and `primaryexp` read.
//!
//! After an operand, Lua's `..`, `~=`, and binary `~` get a message that names the operator to
//! write instead.

use super::{Done, Marker, ParseErrorKind, Parser, PendingIf, Skip};
use crate::{NodeKind, TokenKind};

use NodeKind as N;
use TokenKind as T;

/// The levels of the precedence table, from the loosest binding to the tightest, as the language
/// spec numbers them. A binary operator's right operand holds only operators that bind more
/// tightly than its own level, which makes each binary operator but `**` group from left to
/// right.
mod level {
    /// No operator yet: the whole of an `orexp`.
    pub(super) const NONE: u8 = 0;
    /// `or`.
    pub(super) const OR: u8 = 2;
    /// `and`.
    pub(super) const AND: u8 = 3;
    /// Unary `not`, whose operand may hold comparisons.
    pub(super) const NOT: u8 = 4;
    /// The comparisons, which chain.
    pub(super) const COMPARE: u8 = 5;
    /// `|`.
    pub(super) const BIT_OR: u8 = 6;
    /// `^`.
    pub(super) const BIT_XOR: u8 = 7;
    /// `&`.
    pub(super) const BIT_AND: u8 = 8;
    /// `<<` and `>>`.
    pub(super) const SHIFT: u8 = 9;
    /// `+` and `-`.
    pub(super) const SUM: u8 = 10;
    /// `*`, `/`, `//`, and `%`.
    pub(super) const TERM: u8 = 11;
    /// Unary `-`, `+`, `~`, and `#`, and the right operand of `**`.
    pub(super) const UNARY: u8 = 12;
    /// `**`.
    pub(super) const POWER: u8 = 13;
}

/// What an `if` right after an expression may start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum IfAfter {
    /// Only a conditional expression, because the statement can't end after the expression.
    Conditional,
    /// A conditional expression, or an `if` statement after the statement that the expression
    /// ends, as in `x = f if c then g() end`. The token after the condition decides: `else`
    /// makes a conditional expression.
    ConditionalOrStatement,
}

impl Parser<'_> {
    /// Reads an expression, and returns its node, or `None` after an error that leaves none.
    pub(super) fn expr(&mut self) -> Option<Done> {
        self.expr_before(IfAfter::Conditional)
    }

    /// Reads an expression, after which an `if` may start what `if_after` says.
    pub(super) fn expr_before(&mut self, if_after: IfAfter) -> Option<Done> {
        let value = self.sub_expr(level::NONE)?;
        Some(self.conditional(value, if_after))
    }

    /// Reads expressions with a `,` between each two, in an expression list.
    pub(super) fn expr_list(&mut self) {
        self.expr_list_before(IfAfter::Conditional);
    }

    /// Reads expressions with a `,` between each two, in an expression list, after each of
    /// which an `if` may start what `if_after` says. It ends at an `if` statement.
    pub(super) fn expr_list_before(&mut self, if_after: IfAfter) {
        let list = self.start();
        self.expr_before(if_after);
        while self.pending_if.is_none() && self.eat(T::Comma) {
            self.expr_before(if_after);
        }
        self.complete(list, N::ExprList);
    }

    /// Reads a conditional expression whose first value, `value`, the parser has read, if an
    /// `if` follows it, and returns its node, or `value` itself.
    ///
    /// When the token after the condition isn't `else` and `if_after` allows it, the `if` and
    /// the condition start an `if` statement instead, so their events move to
    /// [`Parser::pending_if`] and `value` ends the expression.
    pub(super) fn conditional(&mut self, value: Done, if_after: IfAfter) -> Done {
        if !self.at(T::If) {
            return value;
        }
        let mark = self.events.len();
        let line = self.line();
        self.bump();
        let condition = self.sub_expr(level::NONE);
        // The real token decides, so that after an error in the condition, an `else` at the stop
        // still makes the expression conditional, and its `expect` ends the unwinding.
        if self.token().kind != T::Else && if_after == IfAfter::ConditionalOrStatement {
            let events = self.events.split_off(mark);
            self.pending_if = Some(PendingIf {
                events,
                line,
                condition,
            });
            return value;
        }
        let node = self.precede(value);
        self.expect(T::Else);
        self.expr_before(if_after);
        self.complete(node, N::ConditionalExpr)
    }

    /// Reads an operand and the binary operators after it that bind more tightly than `limit`,
    /// as PUC-Rio's `subexpr` does: a unary operator and its operand, or a simple expression,
    /// then each binary operator above `limit` and its right operand.
    fn sub_expr(&mut self, limit: u8) -> Option<Done> {
        let operand_limit = match self.current() {
            // `not` binds more loosely than a comparison, so it may start only an operand of
            // `and`, `or`, or `not`, not one of a comparison or of a tighter operator.
            T::Not if limit <= level::NOT => level::NOT,
            T::Minus | T::Plus | T::Tilde | T::Hash => level::UNARY,
            _ => {
                let operand = self.simple_expr()?;
                return Some(self.binary_rest(operand, limit));
            }
        };
        let unary = self.start();
        self.bump();
        self.sub_expr(operand_limit);
        let operand = self.complete(unary, N::UnaryExpr);
        Some(self.binary_rest(operand, limit))
    }

    /// Reads each binary operator that binds more tightly than `limit` after `left`, an operand
    /// already read, with its right operand, and returns the node of the whole. A comparison
    /// takes every comparison after it into one chain. Then, after the last operand, it reports
    /// Lua's `..`, `~=`, and binary `~`.
    fn binary_rest(&mut self, mut left: Done, limit: u8) -> Done {
        while let Some(level) = self.binary_level().filter(|&level| level > limit) {
            if level == level::COMPARE {
                left = self.comparison(left);
                continue;
            }
            let binary = self.precede(left);
            self.bump();
            // `**` groups from right to left, and its right operand may start with a unary
            // operator, so `2 ** -1` is `2 ** (-1)`.
            let right_limit = if level == level::POWER {
                level::UNARY
            } else {
                level
            };
            self.sub_expr(right_limit);
            left = self.complete(binary, N::BinaryExpr);
        }
        let lua_operator = match self.current() {
            T::DotDot => Some(ParseErrorKind::LuaConcatenation),
            T::TildeEqual => Some(ParseErrorKind::LuaNotEqual),
            T::Tilde => Some(ParseErrorKind::LuaBitwiseXor),
            _ => None,
        };
        if let Some(kind) = lua_operator {
            self.error_without_near(kind);
        }
        left
    }

    /// Returns the level of the binary operator at the current token, if one starts there.
    fn binary_level(&self) -> Option<u8> {
        let level = match self.current() {
            T::Or => level::OR,
            T::And => level::AND,
            T::Less
            | T::LessEqual
            | T::Greater
            | T::GreaterEqual
            | T::EqualEqual
            | T::BangEqual
            | T::In
            | T::Is => level::COMPARE,
            T::Not if self.next_kind() == T::In => level::COMPARE,
            T::Pipe => level::BIT_OR,
            T::Caret => level::BIT_XOR,
            T::Ampersand => level::BIT_AND,
            T::LessLess | T::GreaterGreater => level::SHIFT,
            T::Plus | T::Minus => level::SUM,
            T::Star | T::Slash | T::SlashSlash | T::Percent => level::TERM,
            T::StarStar => level::POWER,
            _ => return None,
        };
        Some(level)
    }

    /// Reads a chain of comparisons after `first`, its first operand: each comparison operator,
    /// `not in` and `is not` included, with its right operand.
    fn comparison(&mut self, first: Done) -> Done {
        let compare = self.precede(first);
        while self.binary_level() == Some(level::COMPARE) {
            let first_token = self.current();
            self.bump();
            match first_token {
                // `binary_level` takes `not` only before `in`.
                T::Not => self.bump(),
                T::Is => {
                    self.eat(T::Not);
                }
                _ => {}
            }
            self.sub_expr(level::COMPARE);
        }
        self.complete(compare, N::CompareExpr)
    }

    /// Reads an operand that no unary operator starts: a literal, `...`, an f-string, a function
    /// expression, a table constructor, a list, an expression that starts with a name or `(`, or
    /// a token the lexer couldn't read.
    fn simple_expr(&mut self) -> Option<Done> {
        match self.current() {
            T::Nil | T::True | T::False | T::Integer | T::Float | T::String => {
                let literal = self.start();
                self.bump();
                Some(self.complete(literal, N::LiteralExpr))
            }
            T::DotDotDot => {
                if !self.function().vararg {
                    self.report(ParseErrorKind::VarargOutsideVarargFunction);
                }
                let vararg = self.start();
                self.bump();
                Some(self.complete(vararg, N::VarargExpr))
            }
            // A token the lexer couldn't read, such as a malformed number or a string with a bad
            // escape, stands where it is as an operand, in an error node. The lexer has reported
            // it, so the parser reports nothing and goes on.
            T::Error => {
                let error = self.start();
                self.bump();
                Some(self.complete(error, N::Error))
            }
            T::FStringStart => Some(self.fstring()),
            T::Function => {
                let line = self.line();
                let function = self.start();
                self.bump();
                self.func_body(line);
                Some(self.complete(function, N::FunctionExpr))
            }
            T::LeftBrace => Some(self.table()),
            T::LeftBracket => Some(self.list()),
            _ => self.suffixed_expr(),
        }
    }

    /// Reads a name or a parenthesized expression, then each field, index, call, and method call
    /// after it. Each of those holds the expression before it, so its node starts before that
    /// expression's. A tuple takes none of them, because it isn't a prefix expression.
    pub(super) fn suffixed_expr(&mut self) -> Option<Done> {
        self.suffixed_expr_or_skip(Skip::ToStop)
    }

    /// Reads what [`Parser::suffixed_expr`] reads, after which recovery from a token that starts
    /// no such expression skips as `skip` says.
    pub(super) fn suffixed_expr_or_skip(&mut self, skip: Skip) -> Option<Done> {
        let mut done = self.primary_expr(skip)?;
        if done.kind == N::TupleExpr {
            return Some(done);
        }
        loop {
            let (marker, kind) = match self.current() {
                T::Dot => {
                    let field = self.precede(done);
                    self.bump();
                    self.expect(T::Name);
                    (field, N::FieldExpr)
                }
                T::LeftBracket => {
                    let index = self.precede(done);
                    self.bump();
                    self.bracketed(Self::expr);
                    self.expect(T::RightBracket);
                    (index, N::IndexExpr)
                }
                T::Colon => {
                    let call = self.precede(done);
                    self.bump();
                    self.expect(T::Name);
                    self.args();
                    (call, N::MethodCallExpr)
                }
                T::LeftParen | T::String | T::LeftBrace => {
                    let call = self.precede(done);
                    self.args();
                    (call, N::CallExpr)
                }
                _ => return Some(done),
            };
            done = self.complete(marker, kind);
        }
    }

    /// Reads a name, or what starts with `(`: a parenthesized expression, `(`, an expression,
    /// and `)`, or a tuple, which a `,` after the first expression or a `)` right after the `(`
    /// makes. After any other token, which is an `unexpected symbol`, recovery skips as `skip`
    /// says.
    fn primary_expr(&mut self, skip: Skip) -> Option<Done> {
        match self.current() {
            T::Name => {
                let name = self.start();
                self.bump();
                Some(self.complete(name, N::NameExpr))
            }
            T::LeftParen => {
                let line = self.line();
                let paren = self.start();
                self.bump();
                let kind = if self.at(T::RightParen) {
                    N::TupleExpr
                } else {
                    self.bracketed(|parser| {
                        parser.expr();
                        if parser.at(T::Comma) {
                            parser.more_elements(T::RightParen);
                            N::TupleExpr
                        } else {
                            N::ParenExpr
                        }
                    })
                };
                self.expect_closing(T::RightParen, T::LeftParen, line);
                Some(self.complete(paren, kind))
            }
            _ => {
                let near = self.near(self.token());
                self.error_with(ParseErrorKind::UnexpectedSymbol, near, skip);
                None
            }
        }
    }

    /// Reads the elements of a list or a tuple after its first: each `,` and the element after
    /// it, up to a `,` before `close`, which ends the elements.
    fn more_elements(&mut self, close: TokenKind) {
        while self.eat(T::Comma) {
            if self.at(close) {
                return;
            }
            self.expr();
        }
    }

    /// Reads a list literal: `[`, its elements, and `]`.
    fn list(&mut self) -> Done {
        let line = self.line();
        let list = self.start();
        self.bump();
        if !self.at(T::RightBracket) {
            self.bracketed(|parser| {
                parser.expr();
                parser.more_elements(T::RightBracket);
            });
        }
        self.expect_closing(T::RightBracket, T::LeftBracket, line);
        self.complete(list, N::ListExpr)
    }

    /// Reads a table constructor: `{`, its fields with a `,` or `;` after each, and `}`, as
    /// PUC-Rio's `constructor` does.
    fn table(&mut self) -> Done {
        let line = self.line();
        let table = self.start();
        self.bump();
        self.bracketed(|parser| {
            while !parser.at(T::RightBrace) {
                parser.field();
                if !parser.eat(T::Comma) && !parser.eat(T::Semicolon) {
                    break;
                }
            }
        });
        self.expect_closing(T::RightBrace, T::LeftBrace, line);
        self.complete(table, N::TableExpr)
    }

    /// Reads a field of a table constructor: a name, `=`, and a value; a key in brackets, `=`,
    /// and a value; or a value alone, as PUC-Rio's `field` tells them apart.
    fn field(&mut self) {
        let field = self.start();
        match self.current() {
            T::Name if self.next_kind() == T::Equal => {
                self.bump();
                self.bump();
                self.expr();
                self.complete(field, N::NamedField);
            }
            T::LeftBracket => self.bracket_field(field),
            _ => {
                self.expr();
                self.complete(field, N::PositionalField);
            }
        }
    }

    /// Reads a field that starts with `[`: a key in brackets when one expression, `]`, and `=`
    /// follow, and otherwise a value that starts with a list, such as `[1, 2]` or `[1] + xs`.
    fn bracket_field(&mut self, field: Marker) {
        let line = self.line();
        let list = self.start();
        self.bump();
        if !self.at(T::RightBracket) {
            self.expr();
            if self.at(T::Comma) {
                self.more_elements(T::RightBracket);
                self.expect_closing(T::RightBracket, T::LeftBracket, line);
                let list = self.complete(list, N::ListExpr);
                self.positional_from(field, list);
                return;
            }
            // A key's `]` is PUC-Rio's, which never names the `[` it closes.
            if !self.eat(T::RightBracket) {
                self.error(ParseErrorKind::Expected(T::RightBracket));
                self.abandon(list);
                self.complete(field, N::IndexField);
                return;
            }
            if self.at(T::Equal) {
                self.abandon(list);
                self.bump();
                self.expr();
                self.complete(field, N::IndexField);
                return;
            }
        } else {
            self.bump();
        }
        let list = self.complete(list, N::ListExpr);
        self.positional_from(field, list);
    }

    /// Reads the rest of a positional field whose value starts with `first`, a list already
    /// read: the operators after it, and a conditional expression.
    fn positional_from(&mut self, field: Marker, first: Done) {
        let value = self.binary_rest(first, level::NONE);
        self.conditional(value, IfAfter::Conditional);
        self.complete(field, N::PositionalField);
    }

    /// Reads an f-string: its start, its text and fields, and its end.
    fn fstring(&mut self) -> Done {
        let fstring = self.start();
        self.bump();
        self.fstring_parts();
        self.expect(T::FStringEnd);
        self.complete(fstring, N::FStringExpr)
    }

    /// Reads the text and the fields of an f-string or of a format spec, up to what ends them. A
    /// stretch of text with a lexical error, which the lexer made an error token, is text too.
    fn fstring_parts(&mut self) {
        loop {
            match self.current() {
                T::FStringText | T::Error => self.bump(),
                T::FStringFieldStart => self.fstring_field(),
                _ => return,
            }
        }
    }

    /// Reads a field of an f-string: `{`, an expression, `=` if wanted, a conversion if there is
    /// one, a format spec if there is one, and `}`.
    fn fstring_field(&mut self) {
        let field = self.start();
        self.bump();
        self.bracketed(Self::expr);
        self.eat(T::Equal);
        self.eat(T::FStringConversion);
        if self.at(T::FStringSpec) {
            let spec = self.start();
            self.bump();
            self.fstring_parts();
            self.complete(spec, N::FormatSpec);
        }
        self.expect(T::FStringFieldEnd);
        self.complete(field, N::FStringField);
    }

    /// Reads a call's arguments: `(`, the expressions, and `)`; one string; or one table
    /// constructor, as PUC-Rio's `funcargs` does.
    fn args(&mut self) {
        match self.current() {
            T::LeftParen => {
                let line = self.line();
                let args = self.start();
                self.bump();
                if !self.at(T::RightParen) {
                    self.bracketed(Self::expr_list);
                }
                self.expect_closing(T::RightParen, T::LeftParen, line);
                self.complete(args, N::Args);
            }
            T::String => {
                let args = self.start();
                self.bump();
                self.complete(args, N::Args);
            }
            T::LeftBrace => {
                let args = self.start();
                self.table();
                self.complete(args, N::Args);
            }
            _ => self.error(ParseErrorKind::FunctionArgumentsExpected),
        }
    }
}
