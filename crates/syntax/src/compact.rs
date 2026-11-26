//! The compact tree, which the compiler reads: [`CompactTree`], which [`CompactTree::build`]
//! builds from a source's tokens and the parser's [`Event`]s, and the statements, expressions,
//! and parts it holds.
//!
//! The compact tree holds only what the compiler needs. It keeps no keyword, punctuation, space,
//! or comment, and no empty statement; it holds names and literals as values, with each string's
//! escapes read and each numeral's value worked out, and it keeps the span of each statement,
//! expression, function body, call's arguments, and operator, so the compiler can take whichever
//! line PUC-Rio Lua 5.5.1 gives each instruction. It is built only for a source with no lexical or
//! syntax error. The lossless tree, [`SyntaxTree`](crate::SyntaxTree), is built from the same
//! events, so both trees see the same program (ADR-0009: one parser for the compiler and the
//! tools).
//!
//! The tree is stored as flat lists, one for each kind of thing it holds. A node refers to another
//! by an id, such as an [`ExprId`], and to a list of them by a [`List`], and indexing the tree
//! with one, as in `tree[id]`, gives what it refers to. So building, cloning, and dropping a tree
//! never recurse: a chain of 100,000 additions, each the left operand of the next, can't overflow
//! the call stack. The ids and lists of one tree mean nothing to another.
//!
//! The enums here, such as [`StatKind`] and [`ExprKind`], are exhaustive, unlike
//! [`NodeKind`](crate::NodeKind) and [`TokenKind`](crate::TokenKind), so the compiler can match
//! every kind with no `_` arm, and a kind added later fails the compiler's build until it handles
//! it. The structs are `#[non_exhaustive]`, so fields can join
//! them.
//!
//! # Examples
//!
//! ```
//! use lunaferra_syntax::compact::{BinaryOp, ExprKind, StatKind, TargetKind};
//! use lunaferra_syntax::{Limits, CompactTree, Number, Origin, SourceId, lex, parse};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let source = b"x = 1 + 2\n";
//! let lexed = lex(source, SourceId::new(0), Origin::String, &Limits::default())?;
//! let tree = CompactTree::build(source, &lexed, &parse(source, &lexed))?;
//!
//! let [statement] = tree[tree.block()] else {
//!     return Err("the chunk has one statement".into());
//! };
//! let StatKind::Assign { targets, values } = tree[statement].kind else {
//!     return Err("the statement is an assignment".into());
//! };
//! let [target] = tree[targets] else {
//!     return Err("it assigns one variable".into());
//! };
//! let TargetKind::Name(name) = target.kind else {
//!     return Err("the variable is a name".into());
//! };
//! assert_eq!(&tree[name], b"x");
//!
//! let [value] = tree[values] else {
//!     return Err("it assigns one value".into());
//! };
//! let ExprKind::Binary { left, operator, operator_span, right } = tree[value].kind else {
//!     return Err("the value is a binary expression".into());
//! };
//! assert_eq!(operator, BinaryOp::Add);
//! assert_eq!(tree[left].kind, ExprKind::Number(Number::Integer(1)));
//! assert_eq!(tree[right].kind, ExprKind::Number(Number::Integer(2)));
//! // The `+` is byte 6 of the source, on line 1.
//! assert_eq!((operator_span.start, operator_span.end), (6, 7));
//! assert_eq!(tree.lines().line_col(operator_span.start), (1, 7));
//! # Ok(())
//! # }
//! ```

mod build;

pub use crate::ast::CompareOp;

use std::marker::PhantomData;
use std::ops::{Index, Range};

use crate::lexer::index;
use crate::{Error, Event, Lexed, LineMap, Number, Span};

/// The compact tree of a source with no lexical or syntax error: its statements and expressions,
/// with names and literals as values and the span of each part, and nothing else.
///
/// [`CompactTree::build`] builds one, and indexing it with an id or a [`List`] that one of its
/// nodes holds gives what that refers to. [`CompactTree::block`] gives the statements of the
/// chunk, where reading starts.
///
/// Indexing a tree with an id or a list from another tree gives a part of this tree, or panics if
/// this tree has no part there.
#[derive(Clone, Debug, PartialEq)]
pub struct CompactTree {
    /// The chunk's statements.
    block: List<StatId>,
    /// The span of the [`TokenKind::Eof`](crate::TokenKind::Eof) token, at the end of the source.
    eof: Span,
    /// The source's line map.
    lines: LineMap,
    /// Every statement, each after the statements and expressions it holds.
    stats: Vec<Stat>,
    /// Every expression, each after the expressions it holds.
    exprs: Vec<Expr>,
    /// Every function body.
    functions: Vec<Function>,
    /// The bytes of every name and string value, and of the text of f-strings.
    bytes: Vec<u8>,
    /// The elements of every list of statements: each block.
    stat_lists: Vec<StatId>,
    /// The elements of every list of expressions.
    expr_lists: Vec<ExprId>,
    /// The elements of every list of names.
    names: Vec<Name>,
    /// The elements of every list of assignment targets.
    targets: Vec<Target>,
    /// The elements of every list of declared names.
    att_names: Vec<AttName>,
    /// The elements of every list of `if` branches.
    branches: Vec<Branch>,
    /// The elements of every list of table fields.
    fields: Vec<TableField>,
    /// The elements of every list of comparisons after a chain's first operand.
    comparisons: Vec<Comparison>,
    /// The elements of every list of f-string parts.
    parts: Vec<FStringPart>,
}

impl CompactTree {
    /// Builds the compact tree of `source`, whose tokens are `lexed`, from `events`, the
    /// parser's events for it.
    ///
    /// `lexed` must be what [`lex`](crate::lex) gave for `source`. With any other source, the
    /// names and strings in the tree mean nothing, though nothing panics.
    ///
    /// The events must make a tree, by the rule of
    /// [`SyntaxTree::build`](crate::SyntaxTree::build), and each node must hold the parts of its
    /// kind, in order, with every keyword and symbol, as the
    /// [syntax spec](https://github.com/jabbott-iii/Lunaferra/blob/main/docs/specs/syntax.md#lossless-tree)
    /// lists them: the events of a source with no syntax error, which [`parse`](crate::parse)
    /// gives. Building checks the events in order, and returns the first error it finds. It
    /// takes time in proportion to the number of events and the source's length, and it never
    /// recurses.
    ///
    /// # Errors
    ///
    /// - [`Error::SourceHasErrors`] if `lexed` holds a lexical error, or for the first
    ///   [`Error`](Event::Error) event.
    /// - [`Error::EventOutsideRoot`], [`Error::TokenOutOfOrder`], and [`Error::EventsEndEarly`] for
    ///   events that make no tree, as [`SyntaxTree::build`](crate::SyntaxTree::build) gives them.
    /// - [`Error::MalformedNode`] for a node that doesn't hold the parts of its kind, which
    ///   includes every [`NodeKind::Error`](crate::NodeKind::Error) node, and for a root that
    ///   isn't a [`NodeKind::Chunk`](crate::NodeKind::Chunk).
    pub fn build(source: &[u8], lexed: &Lexed, events: &[Event]) -> Result<Self, Error> {
        build::build(source, lexed, events)
    }

    /// Returns the statements of the chunk, the source as a whole.
    #[must_use]
    pub fn block(&self) -> List<StatId> {
        self.block
    }

    /// Returns the span of the end of the source, the
    /// [`TokenKind::Eof`](crate::TokenKind::Eof) token, which is empty.
    #[must_use]
    pub fn eof(&self) -> Span {
        self.eof
    }

    /// Returns the source's line map, which gives the line and column of each offset in a span.
    #[must_use]
    pub fn lines(&self) -> &LineMap {
        &self.lines
    }
}

impl Index<StatId> for CompactTree {
    type Output = Stat;

    fn index(&self, id: StatId) -> &Stat {
        &self.stats[index(id.0)]
    }
}

impl Index<ExprId> for CompactTree {
    type Output = Expr;

    fn index(&self, id: ExprId) -> &Expr {
        &self.exprs[index(id.0)]
    }
}

impl Index<FunctionId> for CompactTree {
    type Output = Function;

    fn index(&self, id: FunctionId) -> &Function {
        &self.functions[index(id.0)]
    }
}

impl Index<Bytes> for CompactTree {
    type Output = [u8];

    fn index(&self, bytes: Bytes) -> &[u8] {
        &self.bytes[index(bytes.start)..index(bytes.end)]
    }
}

/// Defines `tree[list]` for a [`List`] of each element type, which gives the elements as a
/// slice of the tree's list of that type.
macro_rules! lists {
    ($($element:ty => $field:ident),+ $(,)?) => {$(
        impl Index<List<$element>> for CompactTree {
            type Output = [$element];

            fn index(&self, list: List<$element>) -> &[$element] {
                &self.$field[list.range()]
            }
        }
    )+};
}

lists! {
    StatId => stat_lists,
    ExprId => expr_lists,
    Name => names,
    Target => targets,
    AttName => att_names,
    Branch => branches,
    TableField => fields,
    Comparison => comparisons,
    FStringPart => parts,
}

/// The id of a [`Stat`] in a [`CompactTree`], which indexing the tree with gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StatId(u32);

/// The id of an [`Expr`] in a [`CompactTree`], which indexing the tree with gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExprId(u32);

/// The id of a [`Function`] in a [`CompactTree`], which indexing the tree with gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FunctionId(u32);

/// A list of `T`s in a [`CompactTree`], such as the statements of a block. Indexing the tree with
/// it gives the elements as a slice, in source order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct List<T> {
    /// The position of the first element in the tree's list of `T`s.
    start: u32,
    /// The number of elements.
    len: u32,
    /// The type of the elements, which picks the tree's list that holds them.
    element: PhantomData<fn() -> T>,
}

impl<T> List<T> {
    /// The list with no elements.
    const EMPTY: Self = Self::new(0, 0);

    /// Returns the list of the `len` elements from `start` in the tree's list of `T`s.
    const fn new(start: u32, len: u32) -> Self {
        Self {
            start,
            len,
            element: PhantomData,
        }
    }

    /// Returns the positions of the list's elements in the tree's list of `T`s.
    fn range(self) -> Range<usize> {
        let start = index(self.start);
        start..start.saturating_add(index(self.len))
    }
}

/// The bytes of a name or of a string's value in a [`CompactTree`], which indexing the tree with
/// gives. They need not be valid UTF-8, as Lua strings need not be.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Bytes {
    /// The offset of the first byte in the tree's bytes.
    start: u32,
    /// The offset just past the last byte in the tree's bytes.
    end: u32,
}

/// A name, such as a variable's or a field's, with its span.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Name {
    /// The name's bytes.
    pub text: Bytes,
    /// The span of the name's token.
    pub span: Span,
}

/// A statement: what it is, and its span, from the start of its first token to the end of its
/// last.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Stat {
    /// What the statement is, with its parts.
    pub kind: StatKind,
    /// The statement's span.
    pub span: Span,
}

/// What a [`Stat`] is, with its parts. The syntax spec's statement nodes list the source form of
/// each; an empty statement, `;`, does nothing, so the tree leaves it out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StatKind {
    /// An assignment, such as `a, t.b = 1, 2`.
    Assign {
        /// The variables assigned to, at least one.
        targets: List<Target>,
        /// The values, at least one.
        values: List<ExprId>,
    },
    /// A call as a statement, such as `print(x)`, whose results are thrown away.
    Call(Call),
    /// A label, such as `::top::`.
    Label(Name),
    /// `break`.
    Break,
    /// `goto` and the label's name.
    Goto(Name),
    /// `do`, a block, and `end`.
    Do(List<StatId>),
    /// A `while` loop.
    While {
        /// The condition, tested before each pass.
        condition: ExprId,
        /// The loop's body.
        body: List<StatId>,
    },
    /// A `repeat` loop.
    Repeat {
        /// The loop's body, whose locals the condition sees.
        body: List<StatId>,
        /// The condition, tested after each pass.
        condition: ExprId,
    },
    /// An `if` statement.
    If {
        /// The `if` branch, then each `elseif` branch, in order.
        branches: List<Branch>,
        /// The block of the `else` clause, if there is one.
        else_body: Option<List<StatId>>,
    },
    /// A numeric `for`, such as `for i = 0, 10, 2 do ... end`.
    NumericFor {
        /// The control variable.
        name: Name,
        /// The first value.
        start: ExprId,
        /// The value the loop stops before.
        stop: ExprId,
        /// The step, if there is one.
        step: Option<ExprId>,
        /// The loop's body.
        body: List<StatId>,
    },
    /// A generic `for`, such as `for k, v in pairs(t) do ... end`.
    GenericFor {
        /// The variables, at least one.
        names: List<Name>,
        /// The values after `in`, at least one.
        values: List<ExprId>,
        /// The loop's body.
        body: List<StatId>,
    },
    /// A function statement, such as `function a.b:c() ... end`.
    Function {
        /// The function's name and the fields before it, such as `a` and `b` in `a.b:c`, at
        /// least one.
        path: List<Name>,
        /// The method's name after `:`, if there is one, such as `c` in `a.b:c`.
        method: Option<Name>,
        /// The function.
        function: FunctionId,
    },
    /// `local function`, a name, and the function.
    LocalFunction {
        /// The local's name.
        name: Name,
        /// The function.
        function: FunctionId,
    },
    /// `global function`, a name, and the function.
    GlobalFunction {
        /// The global's name.
        name: Name,
        /// The function.
        function: FunctionId,
    },
    /// A `local` declaration, such as `local x <const>, y = 1`.
    Local {
        /// The names, at least one, each with its attribute.
        names: List<AttName>,
        /// The values after `=`, none if there is no `=`.
        values: List<ExprId>,
    },
    /// A `global` declaration of names, such as `global x, y = 1`.
    Global {
        /// The names, at least one, each with its attribute.
        names: List<AttName>,
        /// The values after `=`, none if there is no `=`.
        values: List<ExprId>,
    },
    /// `global *`, or `global <const> *`, which declares every name a global.
    GlobalAll {
        /// The attribute before `*`, if there is one.
        attrib: Option<Attrib>,
    },
    /// `nonlocal` and the names, at least one.
    Nonlocal(List<Name>),
    /// `return` and the values, none for `return` alone.
    Return(List<ExprId>),
}

/// A variable that an assignment assigns to: what it is, and its span.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Target {
    /// What the variable is.
    pub kind: TargetKind,
    /// The variable's span.
    pub span: Span,
}

/// What a [`Target`] is: a name, a field, or an index, as Lua's `var` is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TargetKind {
    /// A name, such as `x`.
    Name(Bytes),
    /// A field, such as `t.x`.
    Field {
        /// The expression before the `.`.
        object: ExprId,
        /// The field's name.
        field: Name,
    },
    /// An index, such as `t[k]`.
    Index {
        /// The expression before the `[`.
        object: ExprId,
        /// The index, between the brackets.
        index: ExprId,
    },
}

/// A call, as an expression or as a statement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Call {
    /// A call of a function, such as `f(x)`.
    Function {
        /// The function called.
        callee: ExprId,
        /// The arguments.
        args: Args,
    },
    /// A method call, such as `s:upper()`, which passes the object as the first argument.
    Method {
        /// The object, before the `:`.
        object: ExprId,
        /// The method's name.
        method: Name,
        /// The arguments, after the object.
        args: Args,
    },
}

/// The arguments of a [`Call`], and their span.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Args {
    /// The arguments' values. A call written with a string or a table, as `f "x"` or `f {1}` is,
    /// has that one value.
    pub values: List<ExprId>,
    /// The span of the arguments, from the `(` to the `)`, or of the string or the table. Its
    /// start is where PUC-Rio Lua 5.5.1 takes the call's line from.
    pub span: Span,
}

/// A branch of an `if` statement: the `if` or an `elseif`, with its condition and block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Branch {
    /// The condition.
    pub condition: ExprId,
    /// The block that runs when the condition is true.
    pub body: List<StatId>,
}

/// A name in a `local` or `global` declaration, with its attribute.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct AttName {
    /// The name.
    pub name: Name,
    /// The name's attribute: its own, or else the one before the first name, which applies to
    /// every name without its own. `local <const> x, y <close>` gives `x` the attribute `const`
    /// and `y` the attribute `close`.
    pub attrib: Option<Attrib>,
}

/// An attribute of a declared name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Attrib {
    /// `<const>`: the variable can't be assigned to.
    Const,
    /// `<close>`: the variable's value is closed when it goes out of scope.
    Close,
}

/// A function's parameters and body, and its span.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Function {
    /// The named parameters, in order.
    pub params: List<Name>,
    /// The vararg parameter, `...`, if there is one.
    pub vararg: Option<Vararg>,
    /// The function's body.
    pub body: List<StatId>,
    /// The span from the `(` of the parameters to the `end`, where the function's last line is.
    /// The `function` keyword is where the [`Stat`] of a function statement starts, or the
    /// [`Expr`] of a function expression.
    pub span: Span,
}

/// A function's vararg parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Vararg {
    /// `...` alone.
    Unnamed,
    /// `...` and the name of the vararg table, as in `function f(...args)`.
    Named(Name),
}

/// An expression: what it is, and its span, from the start of its first token to the end of its
/// last.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct Expr {
    /// What the expression is, with its parts.
    pub kind: ExprKind,
    /// The expression's span.
    pub span: Span,
}

/// What an [`Expr`] is, with its parts. The syntax spec's expression nodes list the source form of
/// each.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ExprKind {
    /// `nil`.
    Nil,
    /// `true`.
    True,
    /// `false`.
    False,
    /// A numeral's value.
    Number(Number),
    /// A string's value: the bytes its characters and escapes stand for.
    String(Bytes),
    /// `...`.
    Vararg,
    /// A name.
    Name(Bytes),
    /// An expression in parentheses, which keeps only its first value, as `(f())` does.
    Paren(ExprId),
    /// A field, such as `t.x`.
    Field {
        /// The expression before the `.`.
        object: ExprId,
        /// The field's name.
        field: Name,
    },
    /// An index, such as `t[k]`.
    Index {
        /// The expression before the `[`.
        object: ExprId,
        /// The index, between the brackets.
        index: ExprId,
    },
    /// A call.
    Call(Call),
    /// A binary operator and its operands, such as `a + b`. A comparison is a
    /// [`Compare`](ExprKind::Compare) instead.
    Binary {
        /// The left operand.
        left: ExprId,
        /// The operator.
        operator: BinaryOp,
        /// The span of the operator's token.
        operator_span: Span,
        /// The right operand.
        right: ExprId,
    },
    /// A unary operator and its operand, such as `-x`. The operator's token starts the
    /// expression.
    Unary {
        /// The operator.
        operator: UnaryOp,
        /// The operand.
        operand: ExprId,
    },
    /// One comparison or a chain of them, such as `a < b <= c`.
    Compare {
        /// The first operand.
        first: ExprId,
        /// Each comparison, with the operand after its operator, at least one.
        rest: List<Comparison>,
    },
    /// A conditional expression, such as `x if c else y`.
    Conditional {
        /// The value when the condition is true, before `if`.
        then_value: ExprId,
        /// The condition.
        condition: ExprId,
        /// The value otherwise, after `else`.
        else_value: ExprId,
    },
    /// A function expression.
    Function(FunctionId),
    /// A table constructor, such as `{1, x = 2}`, with its fields in order.
    Table(List<TableField>),
    /// A list literal, such as `[1, 2]`, with its elements.
    List(List<ExprId>),
    /// A tuple literal, such as `(1, 2)`, with its elements.
    Tuple(List<ExprId>),
    /// An f-string, such as `f"x is {x}"`, with its text and fields in order.
    FString(List<FStringPart>),
}

/// A binary operator, from the loosest binding to the tightest. Comparisons are a
/// [`CompareOp`] each.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    /// `or`
    Or,
    /// `and`
    And,
    /// `|`
    BitOr,
    /// `^`, bitwise XOR.
    BitXor,
    /// `&`
    BitAnd,
    /// `<<`
    ShiftLeft,
    /// `>>`
    ShiftRight,
    /// `+`, which adds numbers and joins strings.
    Add,
    /// `-`
    Subtract,
    /// `*`
    Multiply,
    /// `/`
    Divide,
    /// `//`
    FloorDivide,
    /// `%`
    Modulo,
    /// `**`
    Power,
}

/// A unary operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    /// `not`
    Not,
    /// `-`
    Negate,
    /// `+`
    Plus,
    /// `~`, bitwise NOT.
    BitNot,
    /// `#`
    Length,
}

/// One comparison of a chain, after the chain's first operand: the operator and the operand after
/// it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Comparison {
    /// The operator.
    pub operator: CompareOp,
    /// The span of the operator, both tokens of `not in` and `is not` included.
    pub operator_span: Span,
    /// The operand after the operator.
    pub operand: ExprId,
}

/// A field of a table constructor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TableField {
    /// A field with a key in brackets, such as `[k] = v`.
    Index {
        /// The key.
        key: ExprId,
        /// The value.
        value: ExprId,
    },
    /// A field with a name for its key, such as `x = 1`.
    Named {
        /// The key, as a name.
        name: Name,
        /// The value.
        value: ExprId,
    },
    /// A field with no key, which takes the next position, such as the `1` of `{1}`.
    Positional(ExprId),
}

/// A part of an f-string or of a field's format spec: text, or a field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FStringPart {
    /// Text, with its escapes read and each `{{` and `}}` as one brace.
    Text(Bytes),
    /// A field.
    Field(FStringField),
}

/// A field of an f-string, such as `{x!r:>8}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct FStringField {
    /// The expression whose value the field writes.
    pub expr: ExprId,
    /// The expression's source text, from the start of its first token to the end of its last,
    /// when `=` follows it, as in `{x=}`, which writes it before the value.
    pub expr_text: Option<Bytes>,
    /// The conversion, `!s` or `!r`, if there is one.
    pub conversion: Option<Conversion>,
    /// The format spec's parts, after its `:`, if there is one, such as the text `>8` of
    /// `{x:>8}`.
    pub spec: Option<List<FStringPart>>,
}

/// The conversion of an f-string's field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Conversion {
    /// `!s`, which writes `tostring` of the value.
    Str,
    /// `!r`, which writes a string quoted.
    Repr,
}
