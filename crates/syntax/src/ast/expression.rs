//! The views of expressions, and of the lists and arguments that hold them.
//!
//! Only the expressions that statements are built from have views so far; the others gain theirs
//! with their grammar, in M1.5.

use super::AstNode;
use crate::{NodeKind, SyntaxNode, SyntaxToken, TokenKind};

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
