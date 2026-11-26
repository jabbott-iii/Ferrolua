//! The expressions that statements are built from, read as PUC-Rio Lua 5.5.1's `simpleexp`,
//! `suffixedexp`, and `primaryexp` read them: literals, `...`, names, parenthesized expressions,
//! fields, indexes, calls, and method calls.
//!
//! The rest of the expressions, with operators and their precedence, table constructors, function
//! expressions, lists, tuples, f-strings, and conditional expressions, join these in M1.5, which
//! replaces [`Parser::expr`] with the full grammar. Until then, a token that only they can start,
//! such as `{` or `-`, is an `unexpected symbol` where an expression must come, and an operator
//! after an expression ends it.

use super::{Done, ParseErrorKind, Parser};
use crate::{NodeKind, TokenKind};

use NodeKind as N;
use TokenKind as T;

impl Parser<'_> {
    /// Reads an expression, and returns its node, or `None` after an error that leaves none.
    pub(super) fn expr(&mut self) -> Option<Done> {
        self.simple_expr()
    }

    /// Reads expressions with a `,` between each two, in an expression list.
    pub(super) fn expr_list(&mut self) {
        let list = self.start();
        self.expr();
        while self.eat(T::Comma) {
            self.expr();
        }
        self.complete(list, N::ExprList);
    }

    /// Reads a literal, `...`, or an expression that starts with a name or `(`.
    fn simple_expr(&mut self) -> Option<Done> {
        match self.current() {
            T::Nil | T::True | T::False | T::Integer | T::Float | T::String => {
                let literal = self.start();
                self.bump();
                Some(self.complete(literal, N::LiteralExpr))
            }
            T::DotDotDot => {
                if !self.function().vararg {
                    self.error(ParseErrorKind::VarargOutsideVarargFunction);
                    return None;
                }
                let vararg = self.start();
                self.bump();
                Some(self.complete(vararg, N::VarargExpr))
            }
            _ => self.suffixed_expr(),
        }
    }

    /// Reads a name or a parenthesized expression, then each field, index, call, and method call
    /// after it. Each of those holds the expression before it, so its node starts before that
    /// expression's.
    pub(super) fn suffixed_expr(&mut self) -> Option<Done> {
        let mut done = self.primary_expr()?;
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
                    self.expr();
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
                T::LeftParen | T::String => {
                    let call = self.precede(done);
                    self.args();
                    (call, N::CallExpr)
                }
                _ => return Some(done),
            };
            done = self.complete(marker, kind);
        }
    }

    /// Reads a name or `(`, an expression, and `)`.
    fn primary_expr(&mut self) -> Option<Done> {
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
                self.expr();
                self.expect_closing(T::RightParen, T::LeftParen, line);
                Some(self.complete(paren, N::ParenExpr))
            }
            _ => {
                self.error(ParseErrorKind::UnexpectedSymbol);
                None
            }
        }
    }

    /// Reads a call's arguments: `(`, the expressions, and `)`, or one string, as PUC-Rio's
    /// `funcargs` does.
    fn args(&mut self) {
        match self.current() {
            T::LeftParen => {
                let line = self.line();
                let args = self.start();
                self.bump();
                if !self.at(T::RightParen) {
                    self.expr_list();
                }
                self.expect_closing(T::RightParen, T::LeftParen, line);
                self.complete(args, N::Args);
            }
            T::String => {
                let args = self.start();
                self.bump();
                self.complete(args, N::Args);
            }
            _ => self.error(ParseErrorKind::FunctionArgumentsExpected),
        }
    }
}
