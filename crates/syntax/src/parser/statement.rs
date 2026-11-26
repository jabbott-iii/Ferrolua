//! The statements of Lua 5.5 (§3.3 and §9 of the manual) and Lunaferra's `nonlocal`, read as
//! PUC-Rio Lua 5.5.1's `statement` reads them, with the parts they share: blocks, attributes, and
//! function bodies.

use super::expression::IfAfter;
use super::{Done, Function, Marker, ParseErrorKind, Parser, PendingIf};
use crate::{NodeKind, Span, TokenKind};

use NodeKind as N;
use TokenKind as T;

/// What an attribute asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Attribute {
    /// No attribute, or `<const>`.
    Plain,
    /// `<close>`, a to-be-closed variable.
    Close,
}

/// An attribute the parser has read.
#[derive(Clone, Copy, Debug)]
struct ReadAttrib {
    /// Its node.
    done: Done,
    /// What it asks for.
    attribute: Attribute,
    /// The span of its name.
    name: Span,
}

impl Parser<'_> {
    /// Reads a block: statements up to a token that ends one.
    pub(super) fn block(&mut self) {
        let block = self.start();
        self.statements();
        self.complete(block, N::Block);
    }

    /// Reads statements up to a token that ends a block, as PUC-Rio's `statlist` does: a
    /// `return` is always the last. An `if` statement that a statement's last expression left
    /// pending comes right after that statement.
    pub(super) fn statements(&mut self) {
        loop {
            if let Some(pending) = self.pending_if.take() {
                self.pending_if_stat(pending);
                continue;
            }
            if self.block_ends() {
                return;
            }
            if self.at(T::Return) {
                self.return_stat();
                return;
            }
            self.statement();
        }
    }

    /// Returns whether the current token ends a block: `else`, `elseif`, `end`, `until`, or the
    /// end of the source.
    fn block_ends(&self) -> bool {
        matches!(
            self.current(),
            T::Else | T::Elseif | T::End | T::Until | T::Eof
        )
    }

    /// Reads one statement other than `return`.
    fn statement(&mut self) {
        let line = self.line();
        match self.current() {
            T::Semicolon => {
                let stat = self.start();
                self.bump();
                self.complete(stat, N::EmptyStat);
            }
            T::If => self.if_stat(line),
            T::While => self.while_stat(line),
            T::Do => {
                let stat = self.start();
                self.bump();
                self.block();
                self.expect_closing(T::End, T::Do, line);
                self.complete(stat, N::DoStat);
            }
            T::For => self.for_stat(line),
            T::Repeat => self.repeat_stat(line),
            T::Function => self.function_stat(line),
            T::Local => self.local_stat(),
            T::Global => self.global_stat(),
            T::ColonColon => {
                let stat = self.start();
                self.bump();
                self.expect(T::Name);
                self.expect(T::ColonColon);
                self.complete(stat, N::LabelStat);
            }
            T::Break => {
                if self.function().loops == 0 {
                    self.error(ParseErrorKind::BreakOutsideLoop);
                    return;
                }
                let stat = self.start();
                self.bump();
                self.complete(stat, N::BreakStat);
            }
            T::Goto => {
                let stat = self.start();
                self.bump();
                self.expect(T::Name);
                self.complete(stat, N::GotoStat);
            }
            T::Name if self.nonlocal_starts() => self.nonlocal_stat(),
            _ => self.expr_stat(),
        }
    }

    /// Reads the body of a loop: a block in which `break` may appear.
    fn loop_block(&mut self) {
        self.function_mut().loops = self.function().loops.saturating_add(1);
        self.block();
        self.function_mut().loops = self.function().loops.saturating_sub(1);
    }

    /// Reads `if`, its condition, `then`, and a block, each `elseif` part, the `else` part, and
    /// `end`. The `if` ends on `line`.
    fn if_stat(&mut self, line: u32) {
        let stat = self.start();
        self.bump();
        self.expr();
        self.if_rest(stat, line);
    }

    /// Reads an `if` statement whose `if` and condition the parser read after an expression:
    /// it puts back their events, and goes on from the condition, which may still be the first
    /// part of a conditional expression, as in `if a if b else c then`.
    fn pending_if_stat(&mut self, pending: PendingIf) {
        let stat = self.start();
        self.events.extend(pending.events);
        if let Some(condition) = pending.condition {
            self.conditional(condition, IfAfter::Conditional);
        }
        self.if_rest(stat, pending.line);
    }

    /// Reads the rest of an `if` statement after its condition: `then` and a block, each
    /// `elseif` part, the `else` part, and `end`. The `if` ends on `line`.
    fn if_rest(&mut self, stat: Marker, line: u32) {
        self.expect(T::Then);
        self.block();
        while self.at(T::Elseif) {
            let clause = self.start();
            self.bump();
            self.expr();
            self.expect(T::Then);
            self.block();
            self.complete(clause, N::ElseifClause);
        }
        if self.at(T::Else) {
            let clause = self.start();
            self.bump();
            self.block();
            self.complete(clause, N::ElseClause);
        }
        self.expect_closing(T::End, T::If, line);
        self.complete(stat, N::IfStat);
    }

    /// Reads `while`, its condition, `do`, the body, and `end`.
    fn while_stat(&mut self, line: u32) {
        let stat = self.start();
        self.bump();
        self.expr();
        self.expect(T::Do);
        self.loop_block();
        self.expect_closing(T::End, T::While, line);
        self.complete(stat, N::WhileStat);
    }

    /// Reads `repeat`, the body, `until`, and its condition.
    fn repeat_stat(&mut self, line: u32) {
        let stat = self.start();
        self.bump();
        self.loop_block();
        self.expect_closing(T::Until, T::Repeat, line);
        self.expr_before(IfAfter::ConditionalOrStatement);
        self.complete(stat, N::RepeatStat);
    }

    /// Reads a `for` loop. Its first name, and the `=` or the `,` or `in` after it, decide
    /// whether it is numeric or generic, as in PUC-Rio's `forstat`.
    fn for_stat(&mut self, line: u32) {
        let stat = self.start();
        self.bump();
        let names = self.start();
        self.expect(T::Name);
        let kind = match self.current() {
            T::Equal => {
                self.abandon(names);
                self.bump();
                self.expr();
                self.expect(T::Comma);
                self.expr();
                if self.eat(T::Comma) {
                    self.expr();
                }
                N::NumericForStat
            }
            T::Comma | T::In => {
                while self.eat(T::Comma) {
                    self.expect(T::Name);
                }
                self.complete(names, N::NameList);
                self.expect(T::In);
                self.expr_list();
                N::GenericForStat
            }
            _ => {
                self.abandon(names);
                self.error(ParseErrorKind::EqualOrInExpected);
                self.complete(stat, N::Error);
                return;
            }
        };
        self.expect(T::Do);
        self.loop_block();
        self.expect_closing(T::End, T::For, line);
        self.complete(stat, kind);
    }

    /// Reads `function`, the function's name, and its body.
    fn function_stat(&mut self, line: u32) {
        let stat = self.start();
        self.bump();
        let name = self.start();
        self.expect(T::Name);
        while self.eat(T::Dot) {
            self.expect(T::Name);
        }
        if self.eat(T::Colon) {
            self.expect(T::Name);
        }
        self.complete(name, N::FuncName);
        self.func_body(line);
        self.complete(stat, N::FunctionStat);
    }

    /// Reads a function's parameter list, its block, and `end`, as PUC-Rio's `body` does. The
    /// construct that `end` closes opened on `line`.
    pub(super) fn func_body(&mut self, line: u32) {
        let body = self.start();
        let params = self.start();
        self.expect(T::LeftParen);
        let mut vararg = false;
        if !self.at(T::RightParen) {
            loop {
                match self.current() {
                    T::Name => {
                        let param = self.start();
                        self.bump();
                        self.complete(param, N::Param);
                    }
                    T::DotDotDot => {
                        let param = self.start();
                        self.bump();
                        self.eat(T::Name);
                        self.complete(param, N::VarargParam);
                        vararg = true;
                    }
                    _ => {
                        self.error(ParseErrorKind::NameOrVarargExpected);
                        break;
                    }
                }
                if vararg || !self.eat(T::Comma) {
                    break;
                }
            }
        }
        self.expect(T::RightParen);
        self.complete(params, N::ParamList);
        self.functions.push(Function { vararg, loops: 0 });
        self.block();
        self.functions.pop();
        self.expect_closing(T::End, T::Function, line);
        self.complete(body, N::FuncBody);
    }

    /// Reads `local function` and a function, or `local` and its names and values.
    fn local_stat(&mut self) {
        let stat = self.start();
        self.bump();
        if self.eat(T::Function) {
            self.expect(T::Name);
            self.func_body(self.line());
            self.complete(stat, N::LocalFunctionStat);
            return;
        }
        let list = self.start();
        let first = self.attrib().map(|attrib| attrib.attribute);
        let mut closing = false;
        loop {
            let name = self.start();
            let at = self.token().span;
            self.expect(T::Name);
            let own = self.attrib().map(|attrib| attrib.attribute);
            self.complete(name, N::AttName);
            if own.or(first) == Some(Attribute::Close) {
                if closing {
                    self.error_at(ParseErrorKind::MultipleToBeClosed, at);
                }
                closing = true;
            }
            if !self.eat(T::Comma) {
                break;
            }
        }
        self.complete(list, N::AttNameList);
        if self.eat(T::Equal) {
            self.expr_list_before(IfAfter::ConditionalOrStatement);
        }
        self.complete(stat, N::LocalStat);
    }

    /// Reads `global function` and a function, `global`, an attribute, and `*`, or `global` and
    /// its names and values.
    fn global_stat(&mut self) {
        let stat = self.start();
        self.bump();
        if self.eat(T::Function) {
            self.expect(T::Name);
            self.func_body(self.line());
            self.complete(stat, N::GlobalFunctionStat);
            return;
        }
        let first = self.global_attrib();
        if self.eat(T::Star) {
            self.complete(stat, N::GlobalStat);
            return;
        }
        let list = match first {
            Some(attrib) => self.precede(attrib),
            None => self.start(),
        };
        loop {
            let name = self.start();
            self.expect(T::Name);
            self.global_attrib();
            self.complete(name, N::AttName);
            if !self.eat(T::Comma) {
                break;
            }
        }
        self.complete(list, N::AttNameList);
        if self.eat(T::Equal) {
            self.expr_list_before(IfAfter::ConditionalOrStatement);
        }
        self.complete(stat, N::GlobalStat);
    }

    /// Reads an attribute of a `global` declaration if one starts here, as PUC-Rio's
    /// `getglobalattribute` does, refusing `<close>`.
    fn global_attrib(&mut self) -> Option<Done> {
        let attrib = self.attrib()?;
        if attrib.attribute == Attribute::Close {
            self.error_at(ParseErrorKind::GlobalToBeClosed, attrib.name);
        }
        Some(attrib.done)
    }

    /// Reads an attribute if one starts here: `<`, a name, and `>`. As in PUC-Rio's
    /// `getvarattribute`, a name other than `const` and `close` is an error, found once the `>`
    /// is read.
    fn attrib(&mut self) -> Option<ReadAttrib> {
        if !self.at(T::Less) {
            return None;
        }
        let attrib = self.start();
        self.bump();
        let name = self.token();
        self.expect(T::Name);
        self.expect(T::Greater);
        let done = self.complete(attrib, N::Attrib);
        let attribute = match self.text(name) {
            b"const" => Attribute::Plain,
            b"close" => Attribute::Close,
            other => {
                let error = ParseErrorKind::UnknownAttribute {
                    name: other.to_vec(),
                };
                self.error_at(error, name.span);
                Attribute::Plain
            }
        };
        Some(ReadAttrib {
            done,
            attribute,
            name: name.span,
        })
    }

    /// Returns whether the current token starts a `nonlocal` declaration: it is the name
    /// `nonlocal`, followed by a name (ADR-0026).
    fn nonlocal_starts(&self) -> bool {
        self.text(self.token()) == b"nonlocal" && self.next_kind() == T::Name
    }

    /// Reads `nonlocal` and its names.
    fn nonlocal_stat(&mut self) {
        let stat = self.start();
        self.bump();
        let names = self.start();
        self.expect(T::Name);
        while self.eat(T::Comma) {
            self.expect(T::Name);
        }
        self.complete(names, N::NameList);
        self.complete(stat, N::NonlocalStat);
    }

    /// Reads `return`, its values, and the `;` after them.
    fn return_stat(&mut self) {
        let stat = self.start();
        self.bump();
        if !self.block_ends() && !self.at(T::Semicolon) {
            self.expr_list();
        }
        self.eat(T::Semicolon);
        self.complete(stat, N::ReturnStat);
    }

    /// Reads a statement that starts with an expression: an assignment or a call, as PUC-Rio's
    /// `exprstat` and `restassign` read them. The first expression comes before the parser knows
    /// which, so the statement's node, and the list of targets, start before it.
    fn expr_stat(&mut self) {
        let Some(first) = self.suffixed_expr() else {
            return;
        };
        if !self.at(T::Equal) && !self.at(T::Comma) {
            let stat = self.precede(first);
            let kind = if matches!(first.kind, N::CallExpr | N::MethodCallExpr) {
                N::CallStat
            } else {
                self.error(ParseErrorKind::SyntaxError);
                N::Error
            };
            self.complete(stat, kind);
            return;
        }
        let list = self.precede(first);
        self.check_target(first);
        while self.eat(T::Comma) {
            if let Some(target) = self.suffixed_expr() {
                self.check_target(target);
            }
        }
        let targets = self.complete(list, N::ExprList);
        let stat = self.precede(targets);
        self.expect(T::Equal);
        self.expr_list_before(IfAfter::ConditionalOrStatement);
        self.complete(stat, N::AssignStat);
    }

    /// Reports `syntax error` unless `target` can be assigned to: a name, a field, or an index.
    fn check_target(&mut self, target: Done) {
        if !matches!(target.kind, N::NameExpr | N::FieldExpr | N::IndexExpr) {
            self.error(ParseErrorKind::SyntaxError);
        }
    }
}
