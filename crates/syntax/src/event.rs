//! The parser's output: a list of [`Event`]s, each of which starts a node of a [`NodeKind`], adds
//! a token to the open node, finishes it, or reports a syntax error.
//!
//! These are a module of their own because two builders read the same events: the lossless tree
//! for the tools, which [`SyntaxTree::build`](crate::SyntaxTree::build) builds, and the compact
//! tree for the compiler. Both see the same program, because both read what the one parser saw
//! (ADR-0009: one parser for the compiler and the tools).

use crate::{ParseError, Span, TokenKind};

/// One step of a parse: start a node, add a token to the open node, finish the open node, or
/// report a syntax error.
///
/// A list of events describes a tree in the order a reader meets its parts. A `Start` opens a
/// node inside the node that is open, a `Token` adds the source's next token to the node that is
/// open, and a `Finish` closes it. For `f()` on its own, [`parse`](crate::parse) gives these
/// events, with each token's span written as its byte range:
///
/// ```text
/// Start(Chunk), Start(Block), Start(CallStat), Start(CallExpr), Start(NameExpr),
/// Token(Name, 0..1), Finish, Start(Args), Token(LeftParen, 1..2), Token(RightParen, 2..3),
/// Finish, Finish, Finish, Finish, Token(Eof, 3..3), Finish
/// ```
///
/// An `Error` adds nothing to the tree. [`SyntaxTree::build`](crate::SyntaxTree::build) says
/// which lists make a tree.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Event {
    /// Opens a node of this kind inside the node that is open, or as the root.
    Start(NodeKind),
    /// Adds the source's next token, of this kind and with these bytes, to the node that is
    /// open.
    Token(TokenKind, Span),
    /// Closes the node that is open.
    Finish,
    /// Reports a syntax error found at this point. The parser reports only the source's first
    /// error, and puts the tokens from where it stopped to the end of the source in an
    /// [`NodeKind::Error`] node, which follows this event.
    Error(ParseError),
}

/// What a node of a syntax tree is.
///
/// The kinds follow the grammar of Lua 5.5 (§9 of the manual) and Lunaferra's additions to it.
/// Each statement and each part of one has a kind, and so do the expressions that statements are
/// built from: names, parenthesized expressions, fields, indexes, calls, method calls, literals,
/// and `...`. The other expressions gain theirs with their grammar, in task M1.5 of the roadmap.
/// The [syntax spec](https://github.com/jabbott-iii/Lunaferra/blob/main/docs/specs/syntax.md#lossless-tree)
/// gives the children of each kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum NodeKind {
    /// A whole source, the root of its tree: its block, then the [`TokenKind::Eof`] token, whose
    /// leading trivia holds the spaces and comments after the last statement.
    Chunk,
    /// A block: a sequence of statements, as the body of a chunk, a function, or a loop is.
    Block,
    /// A part of the source that the parser couldn't read: the tokens it skipped after a syntax
    /// error, which mark the place of the error in the tree, or a statement that the error left
    /// without a kind, such as an expression that is neither a call nor assigned to.
    Error,

    /// `;`, the empty statement.
    EmptyStat,
    /// An assignment, such as `a, t.b = 1, 2`: an [`ExprList`](NodeKind::ExprList) of targets, `=`,
    /// and an `ExprList` of values.
    AssignStat,
    /// A call as a statement, such as `print(x)`: a [`CallExpr`](NodeKind::CallExpr) or a
    /// [`MethodCallExpr`](NodeKind::MethodCallExpr).
    CallStat,
    /// A label, such as `::top::`.
    LabelStat,
    /// `break`.
    BreakStat,
    /// `goto` and a label's name.
    GotoStat,
    /// `do`, a block, and `end`.
    DoStat,
    /// `while`, a condition, `do`, a block, and `end`.
    WhileStat,
    /// `repeat`, a block, `until`, and a condition.
    RepeatStat,
    /// `if`, a condition, `then`, and a block, then each
    /// [`ElseifClause`](NodeKind::ElseifClause), the [`ElseClause`](NodeKind::ElseClause) if
    /// there is one, and `end`.
    IfStat,
    /// `elseif`, a condition, `then`, and a block, in an [`IfStat`](NodeKind::IfStat).
    ElseifClause,
    /// `else` and a block, in an [`IfStat`](NodeKind::IfStat).
    ElseClause,
    /// A numeric `for`, such as `for i = 0, 10, 2 do ... end`: the control variable's name, then
    /// two or three expressions, the start, the stop, and the step, then the body.
    NumericForStat,
    /// A generic `for`, such as `for k, v in pairs(t) do ... end`: a
    /// [`NameList`](NodeKind::NameList), `in`, an [`ExprList`](NodeKind::ExprList), and the
    /// body.
    GenericForStat,
    /// `function`, a [`FuncName`](NodeKind::FuncName), and a [`FuncBody`](NodeKind::FuncBody).
    FunctionStat,
    /// A function's name in a [`FunctionStat`](NodeKind::FunctionStat), such as `a.b:c`: a name,
    /// each `.` and name after it, and `:` and a method's name if there is one.
    FuncName,
    /// `local function`, a name, and a [`FuncBody`](NodeKind::FuncBody).
    LocalFunctionStat,
    /// `global function`, a name, and a [`FuncBody`](NodeKind::FuncBody).
    GlobalFunctionStat,
    /// `local` and an [`AttNameList`](NodeKind::AttNameList), then `=` and an
    /// [`ExprList`](NodeKind::ExprList) if there are values.
    LocalStat,
    /// A `global` declaration: `global` and an [`AttNameList`](NodeKind::AttNameList), then `=`
    /// and an [`ExprList`](NodeKind::ExprList) if there are values; or `global`, an
    /// [`Attrib`](NodeKind::Attrib) if there is one, and `*`.
    GlobalStat,
    /// `nonlocal`, which is a [`TokenKind::Name`], and a [`NameList`](NodeKind::NameList).
    NonlocalStat,
    /// `return`, an [`ExprList`](NodeKind::ExprList) if there are values, and `;` if one follows.
    ReturnStat,

    /// The names of a `local` or `global` declaration: an [`Attrib`](NodeKind::Attrib) if one
    /// comes first, which applies to every name without its own, then each
    /// [`AttName`](NodeKind::AttName), with a `,` between each two.
    AttNameList,
    /// A name in an [`AttNameList`](NodeKind::AttNameList), and its
    /// [`Attrib`](NodeKind::Attrib) if it has one.
    AttName,
    /// An attribute, such as `<const>`: `<`, a name, and `>`.
    Attrib,
    /// Names with a `,` between each two, as a generic `for` and `nonlocal` list them.
    NameList,
    /// A function's parameters and body: a [`ParamList`](NodeKind::ParamList), a block, and
    /// `end`.
    FuncBody,
    /// `(`, each [`Param`](NodeKind::Param), the [`VarargParam`](NodeKind::VarargParam) if there
    /// is one, with a `,` between each two, and `)`.
    ParamList,
    /// A named parameter.
    Param,
    /// `...`, and the name of the vararg table if it has one, as in `function f(...args)`.
    VarargParam,
    /// Expressions with a `,` between each two.
    ExprList,

    /// A name, as an expression.
    NameExpr,
    /// `(`, an expression, and `)`.
    ParenExpr,
    /// An expression, `.`, and a field's name, such as `t.x`.
    FieldExpr,
    /// An expression, `[`, the index, and `]`, such as `t[k]`.
    IndexExpr,
    /// The function, an expression, and its [`Args`](NodeKind::Args), such as `f(x)`.
    CallExpr,
    /// An expression, `:`, the method's name, and its [`Args`](NodeKind::Args), such as
    /// `s:upper()`.
    MethodCallExpr,
    /// A call's arguments: `(`, an [`ExprList`](NodeKind::ExprList) if there are any, and `)`,
    /// or one string.
    Args,
    /// `nil`, `true`, `false`, a numeral, or a string.
    LiteralExpr,
    /// `...`, the vararg expression.
    VarargExpr,
}
