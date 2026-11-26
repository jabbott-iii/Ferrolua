//! The views of expressions, and of the parts that only expressions hold: expression lists,
//! arguments, table fields, and the fields and format specs of f-strings.

use super::{AstNode, FuncBody, after};
use crate::{NodeKind, SyntaxElement, SyntaxNode, SyntaxToken, TokenKind};

union! {
    /// Any expression: a view of the nodes of every expression kind.
    Expr {
        /// A name.
        Name(NameExpr),
        /// A parenthesized expression.
        Paren(ParenExpr),
        /// A field, such as `t.x`.
        Field(FieldExpr),
        /// An index, such as `t[k]`.
        Index(IndexExpr),
        /// A call, such as `f(x)`.
        Call(CallExpr),
        /// A method call, such as `s:upper()`.
        MethodCall(MethodCallExpr),
        /// `nil`, `true`, `false`, a numeral, or a string.
        Literal(LiteralExpr),
        /// `...`.
        Vararg(VarargExpr),
        /// A binary operator and its operands, such as `a + b`.
        Binary(BinaryExpr),
        /// A unary operator and its operand, such as `-x`.
        Unary(UnaryExpr),
        /// A comparison or a chain of them, such as `a < b < c`.
        Compare(CompareExpr),
        /// A conditional expression, such as `x if c else y`.
        Conditional(ConditionalExpr),
        /// A function expression, such as `function(x) return x end`.
        Function(FunctionExpr),
        /// A table constructor, such as `{1, x = 2}`.
        Table(TableExpr),
        /// A list literal, such as `[1, 2]`.
        List(ListExpr),
        /// A tuple literal, such as `(1, 2)`.
        Tuple(TupleExpr),
        /// An f-string, such as `f"x is {x}"`.
        FString(FStringExpr),
    }
}

view! {
    /// Expressions with a `,` between each two, a node of kind [`NodeKind::ExprList`].
    ExprList, ExprList
}

impl<'t> ExprList<'t> {
    /// Returns the expressions, in order.
    pub fn exprs(&self) -> impl Iterator<Item = Expr<'t>> + use<'t> {
        self.0.children_of()
    }
}

view! {
    /// A name as an expression, a node of kind [`NodeKind::NameExpr`].
    NameExpr, NameExpr
}

impl<'t> NameExpr<'t> {
    /// Returns the name.
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken<'t>> {
        self.0.child_token(TokenKind::Name)
    }
}

view! {
    /// An expression in parentheses, a node of kind [`NodeKind::ParenExpr`].
    ParenExpr, ParenExpr
}

impl<'t> ParenExpr<'t> {
    /// Returns the expression inside the parentheses.
    #[must_use]
    pub fn expr(&self) -> Option<Expr<'t>> {
        self.0.child()
    }
}

view! {
    /// A field, such as `t.x`, a node of kind [`NodeKind::FieldExpr`].
    FieldExpr, FieldExpr
}

impl<'t> FieldExpr<'t> {
    /// Returns the expression before the `.`.
    #[must_use]
    pub fn object(&self) -> Option<Expr<'t>> {
        self.0.child()
    }

    /// Returns the field's name, after the `.`.
    #[must_use]
    pub fn field(&self) -> Option<SyntaxToken<'t>> {
        self.0.child_token(TokenKind::Name)
    }
}

view! {
    /// An index, such as `t[k]`, a node of kind [`NodeKind::IndexExpr`].
    IndexExpr, IndexExpr
}

impl<'t> IndexExpr<'t> {
    /// Returns the expression before the `[`.
    #[must_use]
    pub fn object(&self) -> Option<Expr<'t>> {
        self.0.children_of().next()
    }

    /// Returns the index, between the brackets.
    #[must_use]
    pub fn index(&self) -> Option<Expr<'t>> {
        self.0.children_of().nth(1)
    }
}

view! {
    /// A call, such as `f(x)` or `f "x"`, a node of kind [`NodeKind::CallExpr`].
    CallExpr, CallExpr
}

impl<'t> CallExpr<'t> {
    /// Returns the expression that gives the function.
    #[must_use]
    pub fn callee(&self) -> Option<Expr<'t>> {
        self.0.child()
    }

    /// Returns the arguments.
    #[must_use]
    pub fn args(&self) -> Option<Args<'t>> {
        self.0.child()
    }
}

view! {
    /// A method call, such as `s:upper()`, a node of kind [`NodeKind::MethodCallExpr`].
    MethodCallExpr, MethodCallExpr
}

impl<'t> MethodCallExpr<'t> {
    /// Returns the expression before the `:`, which the method gets as its first argument.
    #[must_use]
    pub fn object(&self) -> Option<Expr<'t>> {
        self.0.child()
    }

    /// Returns the method's name, after the `:`.
    #[must_use]
    pub fn method(&self) -> Option<SyntaxToken<'t>> {
        self.0.child_token(TokenKind::Name)
    }

    /// Returns the arguments.
    #[must_use]
    pub fn args(&self) -> Option<Args<'t>> {
        self.0.child()
    }
}

view! {
    /// A call's arguments, `(...)` or a string, a node of kind [`NodeKind::Args`].
    Args, Args
}

impl<'t> Args<'t> {
    /// Returns the expressions in the parentheses, if there are any.
    #[must_use]
    pub fn exprs(&self) -> Option<ExprList<'t>> {
        self.0.child()
    }

    /// Returns the string, when the argument is a string without parentheses, as in `f "x"`.
    #[must_use]
    pub fn string(&self) -> Option<SyntaxToken<'t>> {
        self.0.child_token(TokenKind::String)
    }

    /// Returns the table constructor, when the argument is one without parentheses, as in
    /// `f{1, 2}`.
    #[must_use]
    pub fn table(&self) -> Option<TableExpr<'t>> {
        self.0.child()
    }
}

view! {
    /// `nil`, `true`, `false`, a numeral, or a string, a node of kind [`NodeKind::LiteralExpr`].
    LiteralExpr, LiteralExpr
}

impl<'t> LiteralExpr<'t> {
    /// Returns the literal's token.
    #[must_use]
    pub fn token(&self) -> Option<SyntaxToken<'t>> {
        self.0.first_token()
    }
}

view! {
    /// `...` as an expression, a node of kind [`NodeKind::VarargExpr`].
    VarargExpr, VarargExpr
}

view! {
    /// A binary operator and its two operands, such as `a + b` or `a and b`, a node of kind
    /// [`NodeKind::BinaryExpr`].
    BinaryExpr, BinaryExpr
}

impl<'t> BinaryExpr<'t> {
    /// Returns the left operand.
    #[must_use]
    pub fn left(&self) -> Option<Expr<'t>> {
        self.0.children_of().next()
    }

    /// Returns the operator's token, such as `+` or `and`.
    #[must_use]
    pub fn operator(&self) -> Option<SyntaxToken<'t>> {
        self.0.first_token()
    }

    /// Returns the right operand.
    #[must_use]
    pub fn right(&self) -> Option<Expr<'t>> {
        self.0.children_of().nth(1)
    }
}

view! {
    /// A unary operator and its operand, such as `-x`, `#t`, or `not x`, a node of kind
    /// [`NodeKind::UnaryExpr`].
    UnaryExpr, UnaryExpr
}

impl<'t> UnaryExpr<'t> {
    /// Returns the operator's token, such as `-` or `not`.
    #[must_use]
    pub fn operator(&self) -> Option<SyntaxToken<'t>> {
        self.0.first_token()
    }

    /// Returns the operand.
    #[must_use]
    pub fn operand(&self) -> Option<Expr<'t>> {
        self.0.child()
    }
}

view! {
    /// One comparison or a chain of them, such as `a < b` or `a < b <= c`, a node of kind
    /// [`NodeKind::CompareExpr`]. A chain means each comparison in turn, joined by `and`, with
    /// each operand evaluated once.
    CompareExpr, CompareExpr
}

impl<'t> CompareExpr<'t> {
    /// Returns the operands, in order: one more than there are operators.
    pub fn operands(&self) -> impl Iterator<Item = Expr<'t>> + use<'t> {
        self.0.children_of()
    }

    /// Returns the operators, in order, the one between each two operands.
    ///
    /// `not in` and `is not` are two tokens each in the tree, and one operator here. The tokens
    /// are among the node's [`children`](SyntaxNode::children).
    pub fn operators(&self) -> impl Iterator<Item = CompareOp> + use<'t> {
        let mut tokens = self
            .0
            .children()
            .filter_map(SyntaxElement::into_token)
            .map(|token| token.kind())
            .peekable();
        std::iter::from_fn(move || {
            let operator = match tokens.next()? {
                TokenKind::Less => CompareOp::Less,
                TokenKind::LessEqual => CompareOp::LessEqual,
                TokenKind::Greater => CompareOp::Greater,
                TokenKind::GreaterEqual => CompareOp::GreaterEqual,
                TokenKind::EqualEqual => CompareOp::Equal,
                TokenKind::BangEqual => CompareOp::NotEqual,
                TokenKind::In => CompareOp::In,
                // The parser takes `not` here only before `in`.
                TokenKind::Not => {
                    tokens.next_if_eq(&TokenKind::In);
                    CompareOp::NotIn
                }
                TokenKind::Is if tokens.next_if_eq(&TokenKind::Not).is_some() => CompareOp::IsNot,
                // Every token among the children is an operator's, so this one is `is`.
                _ => CompareOp::Is,
            };
            Some(operator)
        })
    }
}

/// A comparison operator of a [`CompareExpr`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CompareOp {
    /// `<`
    Less,
    /// `<=`
    LessEqual,
    /// `>`
    Greater,
    /// `>=`
    GreaterEqual,
    /// `==`
    Equal,
    /// `!=`
    NotEqual,
    /// `in`, which tests membership.
    In,
    /// `not in`
    NotIn,
    /// `is`, which tests identity, as `rawequal` does.
    Is,
    /// `is not`
    IsNot,
}

view! {
    /// A conditional expression, such as `x if c else y`, a node of kind
    /// [`NodeKind::ConditionalExpr`].
    ConditionalExpr, ConditionalExpr
}

impl<'t> ConditionalExpr<'t> {
    /// Returns the value when the condition is true, before `if`.
    #[must_use]
    pub fn then_value(&self) -> Option<Expr<'t>> {
        self.0.child()
    }

    /// Returns the condition, between `if` and `else`.
    #[must_use]
    pub fn condition(&self) -> Option<Expr<'t>> {
        after(self.0, TokenKind::If)
    }

    /// Returns the value when the condition is false, after `else`.
    #[must_use]
    pub fn else_value(&self) -> Option<Expr<'t>> {
        after(self.0, TokenKind::Else)
    }
}

view! {
    /// A function expression, such as `function(x) return x end`, a node of kind
    /// [`NodeKind::FunctionExpr`].
    FunctionExpr, FunctionExpr
}

impl<'t> FunctionExpr<'t> {
    /// Returns the function's parameters and body.
    #[must_use]
    pub fn body(&self) -> Option<FuncBody<'t>> {
        self.0.child()
    }
}

view! {
    /// A table constructor, such as `{1, x = 2, [k] = 3}`, a node of kind
    /// [`NodeKind::TableExpr`].
    TableExpr, TableExpr
}

impl<'t> TableExpr<'t> {
    /// Returns the fields, in order.
    pub fn fields(&self) -> impl Iterator<Item = TableField<'t>> + use<'t> {
        self.0.children_of()
    }
}

union! {
    /// Any field of a table constructor: a view of the nodes of every field kind.
    TableField {
        /// A field with a key in brackets, such as `[k] = v`.
        Index(IndexField),
        /// A field with a name for its key, such as `x = 1`.
        Named(NamedField),
        /// A field with no key, such as the `1` of `{1}`.
        Positional(PositionalField),
    }
}

view! {
    /// A table field with a key in brackets, such as `[k] = v`, a node of kind
    /// [`NodeKind::IndexField`].
    IndexField, IndexField
}

impl<'t> IndexField<'t> {
    /// Returns the key, between the brackets.
    #[must_use]
    pub fn key(&self) -> Option<Expr<'t>> {
        self.0.child()
    }

    /// Returns the value, after the `=`.
    #[must_use]
    pub fn value(&self) -> Option<Expr<'t>> {
        after(self.0, TokenKind::Equal)
    }
}

view! {
    /// A table field with a name for its key, such as `x = 1`, a node of kind
    /// [`NodeKind::NamedField`].
    NamedField, NamedField
}

impl<'t> NamedField<'t> {
    /// Returns the name.
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken<'t>> {
        self.0.child_token(TokenKind::Name)
    }

    /// Returns the value, after the `=`.
    #[must_use]
    pub fn value(&self) -> Option<Expr<'t>> {
        self.0.child()
    }
}

view! {
    /// A table field with no key, which takes the next position, such as the `1` of `{1}`, a
    /// node of kind [`NodeKind::PositionalField`].
    PositionalField, PositionalField
}

impl<'t> PositionalField<'t> {
    /// Returns the value.
    #[must_use]
    pub fn value(&self) -> Option<Expr<'t>> {
        self.0.child()
    }
}

view! {
    /// A list literal, such as `[1, 2]`, a node of kind [`NodeKind::ListExpr`].
    ListExpr, ListExpr
}

impl<'t> ListExpr<'t> {
    /// Returns the elements, in order.
    pub fn elements(&self) -> impl Iterator<Item = Expr<'t>> + use<'t> {
        self.0.children_of()
    }
}

view! {
    /// A tuple literal, such as `(1, 2)`, `(1,)`, or `()`, a node of kind
    /// [`NodeKind::TupleExpr`].
    TupleExpr, TupleExpr
}

impl<'t> TupleExpr<'t> {
    /// Returns the elements, in order.
    pub fn elements(&self) -> impl Iterator<Item = Expr<'t>> + use<'t> {
        self.0.children_of()
    }
}

view! {
    /// An f-string, such as `f"x is {x}"`, a node of kind [`NodeKind::FStringExpr`].
    FStringExpr, FStringExpr
}

impl<'t> FStringExpr<'t> {
    /// Returns the text and the fields, in order.
    pub fn parts(&self) -> impl Iterator<Item = FStringPart<'t>> + use<'t> {
        fstring_parts(self.0)
    }
}

view! {
    /// A field of an f-string, such as `{x!r:>8}`, a node of kind [`NodeKind::FStringField`].
    FStringField, FStringField
}

impl<'t> FStringField<'t> {
    /// Returns the field's expression.
    #[must_use]
    pub fn expr(&self) -> Option<Expr<'t>> {
        self.0.child()
    }

    /// Returns the `=` after the expression, as in `{x=}`, which writes the expression's text
    /// before its value, if the field has one.
    #[must_use]
    pub fn equal(&self) -> Option<SyntaxToken<'t>> {
        self.0.child_token(TokenKind::Equal)
    }

    /// Returns the conversion, `!s` or `!r`, if the field has one.
    #[must_use]
    pub fn conversion(&self) -> Option<SyntaxToken<'t>> {
        self.0.child_token(TokenKind::FStringConversion)
    }

    /// Returns the format spec, after `:`, if the field has one.
    #[must_use]
    pub fn spec(&self) -> Option<FormatSpec<'t>> {
        self.0.child()
    }
}

view! {
    /// The format spec of an f-string's field, such as the `:>8` of `{x:>8}`, a node of kind
    /// [`NodeKind::FormatSpec`].
    FormatSpec, FormatSpec
}

impl<'t> FormatSpec<'t> {
    /// Returns the spec's text and the fields inside it, in order, after its `:`.
    pub fn parts(&self) -> impl Iterator<Item = FStringPart<'t>> + use<'t> {
        fstring_parts(self.0)
    }
}

/// A part of an f-string or of a format spec: a stretch of text, or a field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FStringPart<'t> {
    /// A [`TokenKind::FStringText`] token. In an f-string's text, `{{` and `}}` stand for one
    /// brace each, and a short f-string's text keeps the escapes of a short string.
    Text(SyntaxToken<'t>),
    /// A field.
    Field(FStringField<'t>),
}

/// Returns the text tokens and the fields among the children of `node`, in order.
fn fstring_parts<'t>(node: SyntaxNode<'t>) -> impl Iterator<Item = FStringPart<'t>> + use<'t> {
    node.children().filter_map(|child| match child {
        SyntaxElement::Token(token) => {
            (token.kind() == TokenKind::FStringText).then_some(FStringPart::Text(token))
        }
        SyntaxElement::Node(node) => FStringField::cast(node).map(FStringPart::Field),
    })
}
