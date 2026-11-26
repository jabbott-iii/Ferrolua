//! Building a [`CompactTree`] from a source's tokens and the parser's events, which
//! [`CompactTree::build`] does.
//!
//! The builder reads the events in order with a stack of the nodes that are open, so it never
//! recurses. Each node's children become *items*, kept on one list for every node that is open:
//! a token is an item as soon as its event comes, with a name's or a literal's value read, and a
//! node becomes one item when it finishes, made from the items of its children. A node whose
//! items don't make one of its kind's forms fails the build. A module of its own because it holds
//! a rule for every kind of node, apart from the tree's types.

use super::{
    Args, AttName, Attrib, BinaryOp, Branch, Bytes, Call, CompactTree, CompareOp, Comparison,
    Conversion, Expr, ExprId, ExprKind, FStringField, FStringPart, Function, FunctionId, List,
    Name, Stat, StatId, StatKind, TableField, Target, TargetKind, UnaryOp, Vararg,
};
use crate::lexer::{Quoting, fstring_text_value, index, string_value};
use crate::{
    Error, Event, Lexed, NodeKind, Number, SourceId, Span, Token, TokenKind, numeral_value,
};

use Item as I;
use NodeKind as K;
use TokenKind as T;

/// The pattern of an item that is a token of kind `TokenKind::$kind`, such as `t!(End)` for an
/// `end`, which has no value of its own.
macro_rules! t {
    ($kind:ident) => {
        I::Token(T::$kind, _)
    };
}

/// Builds the compact tree of `source`, whose tokens are `lexed`, from `events`, as
/// [`CompactTree::build`] describes.
pub(super) fn build(source: &[u8], lexed: &Lexed, events: &[Event]) -> Result<CompactTree, Error> {
    if !lexed.errors().is_empty() {
        return Err(Error::SourceHasErrors);
    }
    let mut builder = Builder::new(source, lexed);
    for (event, step) in events.iter().enumerate() {
        match *step {
            Event::Start(kind) => builder.start(kind, event)?,
            Event::Token(kind, span) => builder.token(kind, span, event)?,
            Event::Finish => builder.finish(event)?,
            Event::Error(_) => return Err(Error::SourceHasErrors),
        }
    }
    builder.into_tree()
}

/// The builder's state: the tree so far, the nodes that are open, and their items.
struct Builder<'s> {
    /// The source's bytes, which names, strings, and the text of f-strings' fields come from.
    source: &'s [u8],
    /// The source's tokens, which the events must name in order.
    tokens: &'s [Token],
    /// The source that every span names.
    id: SourceId,
    /// The tree so far: everything but its block and its end, which the chunk gives last.
    tree: CompactTree,
    /// The nodes that are open, the innermost last.
    open: Vec<Open>,
    /// The items of every open node, in order: each node's items follow those of the node it is
    /// inside.
    items: Vec<Item>,
    /// How many tokens the events have named.
    named: usize,
}

/// A node that is open.
#[derive(Clone, Copy)]
struct Open {
    /// What the node is.
    kind: NodeKind,
    /// The position of the node's `Start` event, which an error names.
    event: usize,
    /// The position in [`Builder::items`] of the node's first item.
    items: usize,
    /// The position in the tokens of the node's first token.
    first: usize,
}

/// A child of a node that is open: a token, with its value if it has one, or a finished node, as
/// what the tree made of it.
#[derive(Clone, Copy)]
enum Item {
    /// A token that has no value of its own, such as a keyword or a symbol.
    Token(TokenKind, Span),
    /// A [`TokenKind::Name`] token.
    Name(Name),
    /// A [`TokenKind::Integer`] or [`TokenKind::Float`] token's value.
    Number(Number),
    /// A [`TokenKind::String`] token's value, and the token's span.
    String(Bytes, Span),
    /// A [`TokenKind::FStringText`] token's value.
    Text(Bytes),
    /// A chunk: its block, and the span of its end.
    Chunk { block: List<StatId>, eof: Span },
    /// A block.
    Block(List<StatId>),
    /// A statement.
    Stat(StatId),
    /// An empty statement, which the tree leaves out.
    Empty,
    /// An expression.
    Expr(ExprId),
    /// A list of expressions.
    Exprs(List<ExprId>),
    /// A call's arguments.
    Args(Args),
    /// A list of names.
    Names(List<Name>),
    /// The names of a `local` or `global` declaration.
    AttNames(List<AttName>),
    /// A declared name, and its own attribute, if it has one.
    AttName(Name, Option<Attrib>),
    /// An attribute.
    Attrib(Attrib),
    /// The name of a function statement.
    FuncName {
        path: List<Name>,
        method: Option<Name>,
    },
    /// A function's parameters and body.
    Body(FunctionId),
    /// A function's parameters.
    Params(List<Name>, Option<Vararg>),
    /// A named parameter.
    Param(Name),
    /// A vararg parameter.
    Vararg(Vararg),
    /// An `elseif` clause.
    Elseif(Branch),
    /// An `else` clause's block.
    Else(List<StatId>),
    /// A table field.
    Field(TableField),
    /// A field of an f-string.
    FStringField(FStringField),
    /// A format spec's parts.
    Spec(List<FStringPart>),
}

/// Whether a list may end with a separator after its last element, as a table's fields may.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Trailing {
    /// The list may end with a separator.
    Allowed,
    /// A separator comes only between two elements.
    Refused,
}

impl<'s> Builder<'s> {
    /// Returns a builder for `source`, whose tokens are `lexed`, with nothing read.
    fn new(source: &'s [u8], lexed: &'s Lexed) -> Self {
        let tokens = lexed.tokens();
        // The last token is always `Eof`, and every token names the same source.
        let id = tokens
            .last()
            .map_or(SourceId::new(0), |eof| eof.span.source);
        let tree = CompactTree {
            block: List::EMPTY,
            eof: Span {
                source: id,
                start: 0,
                end: 0,
            },
            lines: lexed.lines().clone(),
            stats: Vec::new(),
            exprs: Vec::new(),
            functions: Vec::new(),
            bytes: Vec::new(),
            stat_lists: Vec::new(),
            expr_lists: Vec::new(),
            names: Vec::new(),
            targets: Vec::new(),
            att_names: Vec::new(),
            branches: Vec::new(),
            fields: Vec::new(),
            comparisons: Vec::new(),
            parts: Vec::new(),
        };
        Self {
            source,
            tokens,
            id,
            tree,
            open: Vec::new(),
            items: Vec::new(),
            named: 0,
        }
    }

    /// Opens a node of `kind`, whose `Start` is at position `event`.
    fn start(&mut self, kind: NodeKind, event: usize) -> Result<(), Error> {
        if self.open.is_empty() {
            // Only the finished root leaves an item when no node is open.
            if !self.items.is_empty() {
                return Err(Error::EventOutsideRoot { event });
            }
            if kind != K::Chunk {
                return Err(Error::MalformedNode { event });
            }
        }
        self.open.push(Open {
            kind,
            event,
            items: self.items.len(),
            first: self.named,
        });
        Ok(())
    }

    /// Adds the token of `kind` and `span`, whose event is at position `event`, to the node that
    /// is open.
    fn token(&mut self, kind: TokenKind, span: Span, event: usize) -> Result<(), Error> {
        if self.open.is_empty() {
            return Err(Error::EventOutsideRoot { event });
        }
        let token = *self
            .tokens
            .get(self.named)
            .filter(|token| token.kind == kind && token.span == span)
            .ok_or(Error::TokenOutOfOrder { event })?;
        self.named += 1;
        let text = bytes_at(self.source, token.span);
        let item = match token.kind {
            T::Name => I::Name(Name {
                text: self.tree.push_bytes(text),
                span: token.span,
            }),
            // The lexer gives a numeral one of these kinds only when it has a value.
            T::Integer | T::Float => numeral_value(text).map_or(I::Token(kind, span), I::Number),
            T::String => {
                let start = self.tree.bytes.len();
                string_value(text, &mut self.tree.bytes);
                I::String(self.tree.bytes_from(start), span)
            }
            T::FStringText => {
                let quoting = self.quoting();
                let start = self.tree.bytes.len();
                fstring_text_value(text, quoting, &mut self.tree.bytes);
                I::Text(self.tree.bytes_from(start))
            }
            // Every other kind of token is a keyword or a symbol, whose kind is all it says.
            _ => I::Token(kind, span),
        };
        self.items.push(item);
        Ok(())
    }

    /// Returns how the innermost f-string that is open is written, which decides how its text
    /// reads: as a long string's when its start, such as `f[[`, opens a long bracket.
    fn quoting(&self) -> Quoting {
        let start = self
            .open
            .iter()
            .rev()
            .find(|open| open.kind == K::FStringExpr)
            .and_then(|open| self.tokens.get(open.first));
        match start.map(|token| bytes_at(self.source, token.span)) {
            Some([_, b'[', ..]) => Quoting::Long,
            _ => Quoting::Short,
        }
    }

    /// Finishes the node that is open, whose `Finish` is at position `event`, and makes it one
    /// item of the node around it.
    fn finish(&mut self, event: usize) -> Result<(), Error> {
        let open = self.open.pop().ok_or(Error::EventOutsideRoot { event })?;
        let tokens = self.tokens.get(open.first..self.named).unwrap_or(&[]);
        let span = match (tokens.first(), tokens.last()) {
            (Some(first), Some(last)) => Span {
                source: first.span.source,
                start: first.span.start,
                end: last.span.end,
            },
            // Only a block can be empty, and a block keeps no span.
            _ => Span {
                source: self.id,
                start: 0,
                end: 0,
            },
        };
        let items = self.items.get(open.items..).unwrap_or(&[]);
        let item = self
            .tree
            .node(self.source, open.kind, items, span)
            .ok_or(Error::MalformedNode { event: open.event })?;
        self.items.truncate(open.items);
        self.items.push(item);
        Ok(())
    }

    /// Returns the tree, once the events have closed the root. The root is a chunk, whose last
    /// token is `Eof`, the source's last, so every token is named once it is closed.
    fn into_tree(mut self) -> Result<CompactTree, Error> {
        if !self.open.is_empty() {
            return Err(Error::EventsEndEarly);
        }
        // With no node open, the items are the root's, or none if no event came.
        let &[I::Chunk { block, eof }] = self.items.as_slice() else {
            return Err(Error::EventsEndEarly);
        };
        self.tree.block = block;
        self.tree.eof = eof;
        Ok(self.tree)
    }
}

impl CompactTree {
    /// Makes the node of `kind`, whose span is `span`, from `items`, the items of its children,
    /// and returns it as an item, or `None` if the items don't make one of its kind's forms.
    fn node(&mut self, source: &[u8], kind: NodeKind, items: &[Item], span: Span) -> Option<Item> {
        Some(match (kind, items) {
            (K::Chunk, &[I::Block(block), I::Token(T::Eof, eof)]) => I::Chunk { block, eof },
            (K::Block, _) => I::Block(self.statements(items)?),
            (K::EmptyStat, [t!(Semicolon)]) => I::Empty,
            (K::AssignStat, &[I::Exprs(targets), t!(Equal), I::Exprs(values)]) => {
                let targets = self.targets(targets)?;
                self.stat(StatKind::Assign { targets, values }, span)
            }
            (K::CallStat, &[I::Expr(call)]) => {
                let ExprKind::Call(call) = self[call].kind else {
                    return None;
                };
                self.stat(StatKind::Call(call), span)
            }
            (K::LabelStat, &[t!(ColonColon), I::Name(name), t!(ColonColon)]) => {
                self.stat(StatKind::Label(name), span)
            }
            (K::BreakStat, [t!(Break)]) => self.stat(StatKind::Break, span),
            (K::GotoStat, &[t!(Goto), I::Name(name)]) => self.stat(StatKind::Goto(name), span),
            (K::DoStat, &[t!(Do), I::Block(body), t!(End)]) => self.stat(StatKind::Do(body), span),
            (
                K::WhileStat,
                &[
                    t!(While),
                    I::Expr(condition),
                    t!(Do),
                    I::Block(body),
                    t!(End),
                ],
            ) => self.stat(StatKind::While { condition, body }, span),
            (K::RepeatStat, &[t!(Repeat), I::Block(body), t!(Until), I::Expr(condition)]) => {
                self.stat(StatKind::Repeat { body, condition }, span)
            }
            (
                K::IfStat,
                &[
                    t!(If),
                    I::Expr(condition),
                    t!(Then),
                    I::Block(body),
                    ref clauses @ ..,
                    t!(End),
                ],
            ) => {
                let kind = self.if_stat(Branch { condition, body }, clauses)?;
                self.stat(kind, span)
            }
            (K::ElseifClause, &[t!(Elseif), I::Expr(condition), t!(Then), I::Block(body)]) => {
                I::Elseif(Branch { condition, body })
            }
            (K::ElseClause, &[t!(Else), I::Block(body)]) => I::Else(body),
            (
                K::NumericForStat,
                &[
                    t!(For),
                    I::Name(name),
                    t!(Equal),
                    I::Expr(start),
                    t!(Comma),
                    I::Expr(stop),
                    ref step @ ..,
                    t!(Do),
                    I::Block(body),
                    t!(End),
                ],
            ) => {
                let step = match *step {
                    [] => None,
                    [t!(Comma), I::Expr(step)] => Some(step),
                    _ => return None,
                };
                let kind = StatKind::NumericFor {
                    name,
                    start,
                    stop,
                    step,
                    body,
                };
                self.stat(kind, span)
            }
            (
                K::GenericForStat,
                &[
                    t!(For),
                    I::Names(names),
                    t!(In),
                    I::Exprs(values),
                    t!(Do),
                    I::Block(body),
                    t!(End),
                ],
            ) => self.stat(
                StatKind::GenericFor {
                    names,
                    values,
                    body,
                },
                span,
            ),
            (
                K::FunctionStat,
                &[
                    t!(Function),
                    I::FuncName { path, method },
                    I::Body(function),
                ],
            ) => self.stat(
                StatKind::Function {
                    path,
                    method,
                    function,
                },
                span,
            ),
            (K::FuncName, _) => {
                let (path, method) = match *items {
                    [ref path @ .., t!(Colon), I::Name(method)] => (path, Some(method)),
                    _ => (items, None),
                };
                let path = self.name_list(path, T::Dot, I::name)?;
                I::FuncName { path, method }
            }
            (
                K::LocalFunctionStat,
                &[t!(Local), t!(Function), I::Name(name), I::Body(function)],
            ) => self.stat(StatKind::LocalFunction { name, function }, span),
            (
                K::GlobalFunctionStat,
                &[t!(Global), t!(Function), I::Name(name), I::Body(function)],
            ) => self.stat(StatKind::GlobalFunction { name, function }, span),
            (K::LocalStat, &[t!(Local), I::AttNames(names), ref values @ ..]) => {
                let values = declared_values(values)?;
                self.stat(StatKind::Local { names, values }, span)
            }
            (K::GlobalStat, &[t!(Global), I::AttNames(names), ref values @ ..]) => {
                let values = declared_values(values)?;
                self.stat(StatKind::Global { names, values }, span)
            }
            (K::GlobalStat, [t!(Global), t!(Star)]) => {
                self.stat(StatKind::GlobalAll { attrib: None }, span)
            }
            (K::GlobalStat, &[t!(Global), I::Attrib(attrib), t!(Star)]) => self.stat(
                StatKind::GlobalAll {
                    attrib: Some(attrib),
                },
                span,
            ),
            (K::NonlocalStat, &[I::Name(keyword), I::Names(names)])
                if &self[keyword.text] == b"nonlocal" =>
            {
                self.stat(StatKind::Nonlocal(names), span)
            }
            (K::ReturnStat, [t!(Return)] | [t!(Return), t!(Semicolon)]) => {
                self.stat(StatKind::Return(List::EMPTY), span)
            }
            (
                K::ReturnStat,
                &([t!(Return), I::Exprs(values)] | [t!(Return), I::Exprs(values), t!(Semicolon)]),
            ) => self.stat(StatKind::Return(values), span),
            (K::AttNameList, _) => I::AttNames(self.att_names(items)?),
            (K::AttName, &[I::Name(name)]) => I::AttName(name, None),
            (K::AttName, &[I::Name(name), I::Attrib(attrib)]) => I::AttName(name, Some(attrib)),
            (K::Attrib, &[t!(Less), I::Name(name), t!(Greater)]) => {
                I::Attrib(match &self[name.text] {
                    b"const" => Attrib::Const,
                    b"close" => Attrib::Close,
                    _ => return None,
                })
            }
            (K::NameList, _) => I::Names(self.name_list(items, T::Comma, I::name)?),
            (K::FuncBody, &[I::Params(params, vararg), I::Block(body), t!(End)]) => {
                let function = Function {
                    params,
                    vararg,
                    body,
                    span,
                };
                let id = FunctionId(count(self.functions.len()));
                self.functions.push(function);
                I::Body(id)
            }
            (K::ParamList, &[t!(LeftParen), ref inside @ .., t!(RightParen)]) => {
                let (params, vararg) = match *inside {
                    [I::Vararg(vararg)] => (&[][..], Some(vararg)),
                    [ref params @ .., t!(Comma), I::Vararg(vararg)] if !params.is_empty() => {
                        (params, Some(vararg))
                    }
                    _ => (inside, None),
                };
                let params = self.names_or_none(params, T::Comma, I::param)?;
                I::Params(params, vararg)
            }
            (K::Param, &[I::Name(name)]) => I::Param(name),
            (K::VarargParam, [t!(DotDotDot)]) => I::Vararg(Vararg::Unnamed),
            (K::VarargParam, &[t!(DotDotDot), I::Name(name)]) => I::Vararg(Vararg::Named(name)),
            (K::ExprList, _) => I::Exprs(self.exprs(items, Trailing::Refused).filter(non_empty)?),
            (K::NameExpr, &[I::Name(name)]) => self.expr(ExprKind::Name(name.text), span),
            (K::ParenExpr, &[t!(LeftParen), I::Expr(inner), t!(RightParen)]) => {
                self.expr(ExprKind::Paren(inner), span)
            }
            (K::FieldExpr, &[I::Expr(object), t!(Dot), I::Name(field)]) => {
                self.expr(ExprKind::Field { object, field }, span)
            }
            (
                K::IndexExpr,
                &[
                    I::Expr(object),
                    t!(LeftBracket),
                    I::Expr(index),
                    t!(RightBracket),
                ],
            ) => self.expr(ExprKind::Index { object, index }, span),
            (K::CallExpr, &[I::Expr(callee), I::Args(args)]) => {
                self.expr(ExprKind::Call(Call::Function { callee, args }), span)
            }
            (K::MethodCallExpr, &[I::Expr(object), t!(Colon), I::Name(method), I::Args(args)]) => {
                let call = Call::Method {
                    object,
                    method,
                    args,
                };
                self.expr(ExprKind::Call(call), span)
            }
            (K::Args, _) => I::Args(Args {
                values: self.args(items)?,
                span,
            }),
            (K::LiteralExpr, &[item]) => {
                let kind = match item {
                    t!(Nil) => ExprKind::Nil,
                    t!(True) => ExprKind::True,
                    t!(False) => ExprKind::False,
                    I::Number(number) => ExprKind::Number(number),
                    I::String(value, _) => ExprKind::String(value),
                    _ => return None,
                };
                self.expr(kind, span)
            }
            (K::VarargExpr, [t!(DotDotDot)]) => self.expr(ExprKind::Vararg, span),
            (
                K::BinaryExpr,
                &[
                    I::Expr(left),
                    I::Token(operator, operator_span),
                    I::Expr(right),
                ],
            ) => {
                let operator = binary_op(operator)?;
                let kind = ExprKind::Binary {
                    left,
                    operator,
                    operator_span,
                    right,
                };
                self.expr(kind, span)
            }
            (K::UnaryExpr, &[I::Token(operator, _), I::Expr(operand)]) => {
                let operator = unary_op(operator)?;
                self.expr(ExprKind::Unary { operator, operand }, span)
            }
            (K::CompareExpr, &[I::Expr(first), ref rest @ ..]) => {
                let rest = self.comparisons(rest)?;
                self.expr(ExprKind::Compare { first, rest }, span)
            }
            (
                K::ConditionalExpr,
                &[
                    I::Expr(then_value),
                    t!(If),
                    I::Expr(condition),
                    t!(Else),
                    I::Expr(else_value),
                ],
            ) => {
                let kind = ExprKind::Conditional {
                    then_value,
                    condition,
                    else_value,
                };
                self.expr(kind, span)
            }
            (K::FunctionExpr, &[t!(Function), I::Body(function)]) => {
                self.expr(ExprKind::Function(function), span)
            }
            (K::TableExpr, &[t!(LeftBrace), ref inside @ .., t!(RightBrace)]) => {
                let fields = self.table_fields(inside)?;
                self.expr(ExprKind::Table(fields), span)
            }
            (
                K::IndexField,
                &[
                    t!(LeftBracket),
                    I::Expr(key),
                    t!(RightBracket),
                    t!(Equal),
                    I::Expr(value),
                ],
            ) => I::Field(TableField::Index { key, value }),
            (K::NamedField, &[I::Name(name), t!(Equal), I::Expr(value)]) => {
                I::Field(TableField::Named { name, value })
            }
            (K::PositionalField, &[I::Expr(value)]) => I::Field(TableField::Positional(value)),
            (K::ListExpr, &[t!(LeftBracket), ref inside @ .., t!(RightBracket)]) => {
                let elements = self.exprs(inside, Trailing::Allowed)?;
                self.expr(ExprKind::List(elements), span)
            }
            // Only a comma makes a tuple, so `(x)` isn't one.
            (K::TupleExpr, &[t!(LeftParen), ref inside @ .., t!(RightParen)])
                if inside.len() != 1 =>
            {
                let elements = self.exprs(inside, Trailing::Allowed)?;
                self.expr(ExprKind::Tuple(elements), span)
            }
            (K::FStringExpr, &[t!(FStringStart), ref inside @ .., t!(FStringEnd)]) => {
                let parts = self.fstring_parts(inside)?;
                self.expr(ExprKind::FString(parts), span)
            }
            (
                K::FStringField,
                &[
                    t!(FStringFieldStart),
                    I::Expr(expr),
                    ref rest @ ..,
                    t!(FStringFieldEnd),
                ],
            ) => I::FStringField(self.fstring_field(source, expr, rest)?),
            (K::FormatSpec, &[t!(FStringSpec), ref inside @ ..]) => {
                I::Spec(self.fstring_parts(inside)?)
            }
            _ => return None,
        })
    }

    /// Adds the statement of `kind` and `span`, and returns it as an item.
    fn stat(&mut self, kind: StatKind, span: Span) -> Item {
        let id = StatId(count(self.stats.len()));
        self.stats.push(Stat { kind, span });
        I::Stat(id)
    }

    /// Adds the expression of `kind` and `span`, and returns it as an item.
    fn expr(&mut self, kind: ExprKind, span: Span) -> Item {
        I::Expr(self.add_expr(kind, span))
    }

    /// Adds the expression of `kind` and `span`, and returns its id.
    fn add_expr(&mut self, kind: ExprKind, span: Span) -> ExprId {
        let id = ExprId(count(self.exprs.len()));
        self.exprs.push(Expr { kind, span });
        id
    }

    /// Adds `bytes` to the tree's bytes, and returns where they are.
    fn push_bytes(&mut self, bytes: &[u8]) -> Bytes {
        let start = self.bytes.len();
        self.bytes.extend_from_slice(bytes);
        self.bytes_from(start)
    }

    /// Returns the tree's bytes from `start` to the end.
    fn bytes_from(&self, start: usize) -> Bytes {
        Bytes {
            start: count(start),
            end: count(self.bytes.len()),
        }
    }

    /// Returns the list of a block's statements, `items`, leaving out each empty statement.
    fn statements(&mut self, items: &[Item]) -> Option<List<StatId>> {
        let start = self.stat_lists.len();
        for item in items {
            match *item {
                I::Stat(id) => self.stat_lists.push(id),
                I::Empty => {}
                _ => return None,
            }
        }
        Some(list_from(start, self.stat_lists.len()))
    }

    /// Returns the targets of an assignment, the expressions of `list`, each of which must be a
    /// name, a field, or an index.
    fn targets(&mut self, list: List<ExprId>) -> Option<List<Target>> {
        let start = self.targets.len();
        for at in list.range() {
            let target = self[self.expr_lists[at]];
            let kind = match target.kind {
                ExprKind::Name(name) => TargetKind::Name(name),
                ExprKind::Field { object, field } => TargetKind::Field { object, field },
                ExprKind::Index { object, index } => TargetKind::Index { object, index },
                _ => return None,
            };
            self.targets.push(Target {
                kind,
                span: target.span,
            });
        }
        Some(list_from(start, self.targets.len()))
    }

    /// Returns the kind of an `if` statement whose first branch is `first`, from `clauses`, the
    /// items of its `elseif` clauses and its `else` clause.
    fn if_stat(&mut self, first: Branch, clauses: &[Item]) -> Option<StatKind> {
        let (elseifs, else_body) = match *clauses {
            [ref elseifs @ .., I::Else(body)] => (elseifs, Some(body)),
            _ => (clauses, None),
        };
        let start = self.branches.len();
        self.branches.push(first);
        for item in elseifs {
            let &I::Elseif(branch) = item else {
                return None;
            };
            self.branches.push(branch);
        }
        let branches = list_from(start, self.branches.len());
        Some(StatKind::If {
            branches,
            else_body,
        })
    }

    /// Returns the names of a `local` or `global` declaration, from `items`: an attribute if one
    /// comes first, which applies to every name without its own, then the names, with a `,`
    /// between each two.
    fn att_names(&mut self, items: &[Item]) -> Option<List<AttName>> {
        let (default, items) = match *items {
            [I::Attrib(default), ref rest @ ..] => (Some(default), rest),
            _ => (None, items),
        };
        let start = self.att_names.len();
        for item in elements(items, &[T::Comma], Trailing::Refused)? {
            let &I::AttName(name, attrib) = item else {
                return None;
            };
            let attrib = attrib.or(default);
            self.att_names.push(AttName { name, attrib });
        }
        Some(list_from(start, self.att_names.len())).filter(non_empty)
    }

    /// Returns the names that `items` hold, one in each item that `name` reads, with `separator`
    /// between each two: at least one.
    fn name_list(
        &mut self,
        items: &[Item],
        separator: TokenKind,
        name: fn(&Item) -> Option<Name>,
    ) -> Option<List<Name>> {
        self.names_or_none(items, separator, name).filter(non_empty)
    }

    /// Returns the names that `items` hold, one in each item that `name` reads, with `separator`
    /// between each two: none or more.
    fn names_or_none(
        &mut self,
        items: &[Item],
        separator: TokenKind,
        name: fn(&Item) -> Option<Name>,
    ) -> Option<List<Name>> {
        let start = self.names.len();
        for item in elements(items, &[separator], Trailing::Refused)? {
            self.names.push(name(item)?);
        }
        Some(list_from(start, self.names.len()))
    }

    /// Returns the expressions of `items`, with a `,` between each two, and after the last when
    /// `trailing` allows it: none or more.
    fn exprs(&mut self, items: &[Item], trailing: Trailing) -> Option<List<ExprId>> {
        let start = self.expr_lists.len();
        for item in elements(items, &[T::Comma], trailing)? {
            let &I::Expr(id) = item else {
                return None;
            };
            self.expr_lists.push(id);
        }
        Some(list_from(start, self.expr_lists.len()))
    }

    /// Returns the values of a call's arguments, from `items`: `(`, an expression list if there
    /// are any, and `)`; a string; or a table.
    fn args(&mut self, items: &[Item]) -> Option<List<ExprId>> {
        let one = match *items {
            [t!(LeftParen), t!(RightParen)] => return Some(List::EMPTY),
            [t!(LeftParen), I::Exprs(values), t!(RightParen)] => return Some(values),
            [I::String(value, span)] => self.add_expr(ExprKind::String(value), span),
            [I::Expr(table)] if matches!(self[table].kind, ExprKind::Table(_)) => table,
            _ => return None,
        };
        let start = self.expr_lists.len();
        self.expr_lists.push(one);
        Some(list_from(start, self.expr_lists.len()))
    }

    /// Returns the comparisons of a chain after its first operand, from `items`: each operator,
    /// one token or the two of `not in` and `is not`, and the operand after it, at least one.
    fn comparisons(&mut self, mut items: &[Item]) -> Option<List<Comparison>> {
        let start = self.comparisons.len();
        while !items.is_empty() {
            let (operator, first, last, operand, rest) = match *items {
                [
                    I::Token(T::Not, first),
                    I::Token(T::In, last),
                    I::Expr(operand),
                    ref rest @ ..,
                ] => (CompareOp::NotIn, first, last, operand, rest),
                [
                    I::Token(T::Is, first),
                    I::Token(T::Not, last),
                    I::Expr(operand),
                    ref rest @ ..,
                ] => (CompareOp::IsNot, first, last, operand, rest),
                [I::Token(operator, span), I::Expr(operand), ref rest @ ..] => {
                    (compare_op(operator)?, span, span, operand, rest)
                }
                _ => return None,
            };
            let operator_span = Span {
                source: first.source,
                start: first.start,
                end: last.end,
            };
            self.comparisons.push(Comparison {
                operator,
                operator_span,
                operand,
            });
            items = rest;
        }
        Some(list_from(start, self.comparisons.len())).filter(non_empty)
    }

    /// Returns a table's fields, from `items`, the items between its braces: the fields, with a
    /// `,` or `;` after each but the last, and after the last if wanted.
    fn table_fields(&mut self, items: &[Item]) -> Option<List<TableField>> {
        let start = self.fields.len();
        for item in elements(items, &[T::Comma, T::Semicolon], Trailing::Allowed)? {
            let &I::Field(field) = item else {
                return None;
            };
            self.fields.push(field);
        }
        Some(list_from(start, self.fields.len()))
    }

    /// Returns the parts of an f-string or of a format spec, from `items`: its text and fields,
    /// in order.
    fn fstring_parts(&mut self, items: &[Item]) -> Option<List<FStringPart>> {
        let start = self.parts.len();
        for item in items {
            let part = match *item {
                I::Text(text) => FStringPart::Text(text),
                I::FStringField(field) => FStringPart::Field(field),
                _ => return None,
            };
            self.parts.push(part);
        }
        Some(list_from(start, self.parts.len()))
    }

    /// Returns the field of an f-string that writes `expr`, from `rest`, the items between the
    /// expression and the field's end: `=` if wanted, a conversion if there is one, and a format
    /// spec if there is one. The conversion's text in `source` is `!s` or `!r`.
    fn fstring_field(
        &mut self,
        source: &[u8],
        expr: ExprId,
        rest: &[Item],
    ) -> Option<FStringField> {
        let (expr_text, rest) = match *rest {
            [t!(Equal), ref rest @ ..] => {
                let span = self[expr].span;
                (Some(self.push_bytes(bytes_at(source, span))), rest)
            }
            _ => (None, rest),
        };
        let (conversion, rest) = match *rest {
            [I::Token(T::FStringConversion, span), ref rest @ ..] => {
                let conversion = if bytes_at(source, span) == b"!s" {
                    Conversion::Str
                } else {
                    Conversion::Repr
                };
                (Some(conversion), rest)
            }
            _ => (None, rest),
        };
        let spec = match *rest {
            [] => None,
            [I::Spec(parts)] => Some(parts),
            _ => return None,
        };
        Some(FStringField {
            expr,
            expr_text,
            conversion,
            spec,
        })
    }
}

impl Item {
    /// Returns the name of a [`TokenKind::Name`] item.
    fn name(&self) -> Option<Name> {
        match *self {
            I::Name(name) => Some(name),
            _ => None,
        }
    }

    /// Returns the name of a parameter's item.
    fn param(&self) -> Option<Name> {
        match *self {
            I::Param(name) => Some(name),
            _ => None,
        }
    }
}

/// Returns the values of a `local` or `global` declaration, from `items`, the items after its
/// names: `=` and an expression list, or nothing.
fn declared_values(items: &[Item]) -> Option<List<ExprId>> {
    match *items {
        [] => Some(List::EMPTY),
        [t!(Equal), I::Exprs(values)] => Some(values),
        _ => None,
    }
}

/// Returns the elements of `items`, which must have one of `separators` between each two, and
/// after the last only when `trailing` allows it, or `None` if they don't.
fn elements<'i>(
    items: &'i [Item],
    separators: &[TokenKind],
    trailing: Trailing,
) -> Option<impl Iterator<Item = &'i Item>> {
    let separated = items
        .iter()
        .skip(1)
        .step_by(2)
        .all(|item| matches!(item, I::Token(kind, _) if separators.contains(kind)));
    let ends_with_separator = !items.is_empty() && items.len().is_multiple_of(2);
    (separated && (trailing == Trailing::Allowed || !ends_with_separator))
        .then(|| items.iter().step_by(2))
}

/// Returns whether `list` has an element.
fn non_empty<T>(list: &List<T>) -> bool {
    list.len > 0
}

/// Returns the list of the elements from `start` to `end` in one of the tree's lists.
fn list_from<T>(start: usize, end: usize) -> List<T> {
    List::new(count(start), count(end.saturating_sub(start)))
}

/// Converts a position or a length in one of the tree's lists to a `u32`. Each fits, because a
/// tree holds fewer parts than its source has bytes, and a source has at most
/// [`MAX_SOURCE_LEN`](crate::MAX_SOURCE_LEN) bytes.
fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// Returns the bytes of `span` in `source`, or none if it isn't inside the source.
fn bytes_at(source: &[u8], span: Span) -> &[u8] {
    source
        .get(index(span.start)..index(span.end))
        .unwrap_or(&[])
}

/// Returns the binary operator of a token of `kind`, or `None` if it isn't one.
fn binary_op(kind: TokenKind) -> Option<BinaryOp> {
    Some(match kind {
        T::Or => BinaryOp::Or,
        T::And => BinaryOp::And,
        T::Pipe => BinaryOp::BitOr,
        T::Caret => BinaryOp::BitXor,
        T::Ampersand => BinaryOp::BitAnd,
        T::LessLess => BinaryOp::ShiftLeft,
        T::GreaterGreater => BinaryOp::ShiftRight,
        T::Plus => BinaryOp::Add,
        T::Minus => BinaryOp::Subtract,
        T::Star => BinaryOp::Multiply,
        T::Slash => BinaryOp::Divide,
        T::SlashSlash => BinaryOp::FloorDivide,
        T::Percent => BinaryOp::Modulo,
        T::StarStar => BinaryOp::Power,
        _ => return None,
    })
}

/// Returns the unary operator of a token of `kind`, or `None` if it isn't one.
fn unary_op(kind: TokenKind) -> Option<UnaryOp> {
    Some(match kind {
        T::Not => UnaryOp::Not,
        T::Minus => UnaryOp::Negate,
        T::Plus => UnaryOp::Plus,
        T::Tilde => UnaryOp::BitNot,
        T::Hash => UnaryOp::Length,
        _ => return None,
    })
}

/// Returns the comparison operator of a single token of `kind`, or `None` if it isn't one.
fn compare_op(kind: TokenKind) -> Option<CompareOp> {
    Some(match kind {
        T::Less => CompareOp::Less,
        T::LessEqual => CompareOp::LessEqual,
        T::Greater => CompareOp::Greater,
        T::GreaterEqual => CompareOp::GreaterEqual,
        T::EqualEqual => CompareOp::Equal,
        T::BangEqual => CompareOp::NotEqual,
        T::In => CompareOp::In,
        T::Is => CompareOp::Is,
        _ => return None,
    })
}
