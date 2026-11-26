//! The views of statements, and of the parts that only statements hold: clauses, names,
//! attributes, and function bodies.

use super::{AstNode, Block, Expr, ExprList, after};
use crate::{NodeKind, SyntaxNode, SyntaxToken, TokenKind};

union! {
    /// Any statement: a view of the nodes of every statement kind.
    Stat {
        /// `;`.
        Empty(EmptyStat),
        /// An assignment.
        Assign(AssignStat),
        /// A call as a statement.
        Call(CallStat),
        /// A label.
        Label(LabelStat),
        /// `break`.
        Break(BreakStat),
        /// `goto`.
        Goto(GotoStat),
        /// `do ... end`.
        Do(DoStat),
        /// `while ... end`.
        While(WhileStat),
        /// `repeat ... until`.
        Repeat(RepeatStat),
        /// `if ... end`.
        If(IfStat),
        /// A numeric `for`.
        NumericFor(NumericForStat),
        /// A generic `for`.
        GenericFor(GenericForStat),
        /// `function`.
        Function(FunctionStat),
        /// `local function`.
        LocalFunction(LocalFunctionStat),
        /// `global function`.
        GlobalFunction(GlobalFunctionStat),
        /// `local`.
        Local(LocalStat),
        /// `global`.
        Global(GlobalStat),
        /// `nonlocal`.
        Nonlocal(NonlocalStat),
        /// `return`.
        Return(ReturnStat),
    }
}

view! {
    /// The empty statement, `;`, a node of kind [`NodeKind::EmptyStat`].
    EmptyStat, EmptyStat
}

view! {
    /// An assignment, such as `a, t.b = 1, 2`, a node of kind [`NodeKind::AssignStat`].
    AssignStat, AssignStat
}

impl<'t> AssignStat<'t> {
    /// Returns the targets, before the `=`.
    #[must_use]
    pub fn targets(&self) -> Option<ExprList<'t>> {
        self.0.child()
    }

    /// Returns the values, after the `=`.
    #[must_use]
    pub fn values(&self) -> Option<ExprList<'t>> {
        after(self.0, TokenKind::Equal)
    }
}

view! {
    /// A call as a statement, such as `print(x)`, a node of kind [`NodeKind::CallStat`].
    CallStat, CallStat
}

impl<'t> CallStat<'t> {
    /// Returns the call: an [`Expr::Call`] or an [`Expr::MethodCall`].
    #[must_use]
    pub fn call(&self) -> Option<Expr<'t>> {
        self.0.child()
    }
}

view! {
    /// A label, such as `::top::`, a node of kind [`NodeKind::LabelStat`].
    LabelStat, LabelStat
}

impl<'t> LabelStat<'t> {
    /// Returns the label's name.
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken<'t>> {
        self.0.child_token(TokenKind::Name)
    }
}

view! {
    /// `break`, a node of kind [`NodeKind::BreakStat`].
    BreakStat, BreakStat
}

view! {
    /// `goto` and a label's name, a node of kind [`NodeKind::GotoStat`].
    GotoStat, GotoStat
}

impl<'t> GotoStat<'t> {
    /// Returns the name of the label it jumps to.
    #[must_use]
    pub fn label(&self) -> Option<SyntaxToken<'t>> {
        self.0.child_token(TokenKind::Name)
    }
}

view! {
    /// `do`, a block, and `end`, a node of kind [`NodeKind::DoStat`].
    DoStat, DoStat
}

impl<'t> DoStat<'t> {
    /// Returns the block.
    #[must_use]
    pub fn block(&self) -> Option<Block<'t>> {
        self.0.child()
    }
}

view! {
    /// A `while` loop, a node of kind [`NodeKind::WhileStat`].
    WhileStat, WhileStat
}

impl<'t> WhileStat<'t> {
    /// Returns the condition, tested before each pass.
    #[must_use]
    pub fn condition(&self) -> Option<Expr<'t>> {
        self.0.child()
    }

    /// Returns the body.
    #[must_use]
    pub fn block(&self) -> Option<Block<'t>> {
        self.0.child()
    }
}

view! {
    /// A `repeat` loop, a node of kind [`NodeKind::RepeatStat`].
    RepeatStat, RepeatStat
}

impl<'t> RepeatStat<'t> {
    /// Returns the body.
    #[must_use]
    pub fn block(&self) -> Option<Block<'t>> {
        self.0.child()
    }

    /// Returns the condition after `until`, tested after each pass.
    #[must_use]
    pub fn condition(&self) -> Option<Expr<'t>> {
        self.0.child()
    }
}

view! {
    /// An `if` statement, a node of kind [`NodeKind::IfStat`].
    IfStat, IfStat
}

impl<'t> IfStat<'t> {
    /// Returns the condition after `if`.
    #[must_use]
    pub fn condition(&self) -> Option<Expr<'t>> {
        self.0.child()
    }

    /// Returns the block after `then`, which runs when the condition is true.
    #[must_use]
    pub fn block(&self) -> Option<Block<'t>> {
        self.0.child()
    }

    /// Returns the `elseif` parts, in order.
    pub fn elseif_clauses(&self) -> impl Iterator<Item = ElseifClause<'t>> + use<'t> {
        self.0.children_of()
    }

    /// Returns the `else` part.
    #[must_use]
    pub fn else_clause(&self) -> Option<ElseClause<'t>> {
        self.0.child()
    }
}

view! {
    /// An `elseif` part of an `if` statement, a node of kind [`NodeKind::ElseifClause`].
    ElseifClause, ElseifClause
}

impl<'t> ElseifClause<'t> {
    /// Returns the condition after `elseif`.
    #[must_use]
    pub fn condition(&self) -> Option<Expr<'t>> {
        self.0.child()
    }

    /// Returns the block after `then`.
    #[must_use]
    pub fn block(&self) -> Option<Block<'t>> {
        self.0.child()
    }
}

view! {
    /// The `else` part of an `if` statement, a node of kind [`NodeKind::ElseClause`].
    ElseClause, ElseClause
}

impl<'t> ElseClause<'t> {
    /// Returns the block after `else`.
    #[must_use]
    pub fn block(&self) -> Option<Block<'t>> {
        self.0.child()
    }
}

view! {
    /// A numeric `for` loop, such as `for i = 0, 10, 2 do ... end`, a node of kind
    /// [`NodeKind::NumericForStat`].
    NumericForStat, NumericForStat
}

impl<'t> NumericForStat<'t> {
    /// Returns the name of the control variable.
    #[must_use]
    pub fn variable(&self) -> Option<SyntaxToken<'t>> {
        self.0.child_token(TokenKind::Name)
    }

    /// Returns the start, the first expression.
    #[must_use]
    pub fn start(&self) -> Option<Expr<'t>> {
        self.0.children_of().next()
    }

    /// Returns the stop, the second expression, which the loop stops before.
    #[must_use]
    pub fn stop(&self) -> Option<Expr<'t>> {
        self.0.children_of().nth(1)
    }

    /// Returns the step, the third expression, if there is one.
    #[must_use]
    pub fn step(&self) -> Option<Expr<'t>> {
        self.0.children_of().nth(2)
    }

    /// Returns the body.
    #[must_use]
    pub fn block(&self) -> Option<Block<'t>> {
        self.0.child()
    }
}

view! {
    /// A generic `for` loop, such as `for k, v in pairs(t) do ... end`, a node of kind
    /// [`NodeKind::GenericForStat`].
    GenericForStat, GenericForStat
}

impl<'t> GenericForStat<'t> {
    /// Returns the names of the loop's variables.
    #[must_use]
    pub fn names(&self) -> Option<NameList<'t>> {
        self.0.child()
    }

    /// Returns the expressions after `in`.
    #[must_use]
    pub fn values(&self) -> Option<ExprList<'t>> {
        self.0.child()
    }

    /// Returns the body.
    #[must_use]
    pub fn block(&self) -> Option<Block<'t>> {
        self.0.child()
    }
}

view! {
    /// `function`, a name, and a body, a node of kind [`NodeKind::FunctionStat`].
    FunctionStat, FunctionStat
}

impl<'t> FunctionStat<'t> {
    /// Returns the function's name.
    #[must_use]
    pub fn name(&self) -> Option<FuncName<'t>> {
        self.0.child()
    }

    /// Returns the function's parameters and body.
    #[must_use]
    pub fn body(&self) -> Option<FuncBody<'t>> {
        self.0.child()
    }
}

view! {
    /// A function's name in a `function` statement, such as `a.b:c`, a node of kind
    /// [`NodeKind::FuncName`].
    FuncName, FuncName
}

impl<'t> FuncName<'t> {
    /// Returns the names before the `:`, if there is one: the variable, then each field.
    pub fn path(&self) -> impl Iterator<Item = SyntaxToken<'t>> + use<'t> {
        self.0
            .children()
            .filter_map(|child| child.into_token())
            .take_while(|token| token.kind() != TokenKind::Colon)
            .filter(|token| token.kind() == TokenKind::Name)
    }

    /// Returns the method's name, after the `:`, if there is one.
    #[must_use]
    pub fn method(&self) -> Option<SyntaxToken<'t>> {
        self.0
            .children()
            .filter_map(|child| child.into_token())
            .skip_while(|token| token.kind() != TokenKind::Colon)
            .find(|token| token.kind() == TokenKind::Name)
    }
}

view! {
    /// `local function`, a name, and a body, a node of kind [`NodeKind::LocalFunctionStat`].
    LocalFunctionStat, LocalFunctionStat
}

impl<'t> LocalFunctionStat<'t> {
    /// Returns the function's name.
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken<'t>> {
        self.0.child_token(TokenKind::Name)
    }

    /// Returns the function's parameters and body.
    #[must_use]
    pub fn body(&self) -> Option<FuncBody<'t>> {
        self.0.child()
    }
}

view! {
    /// `global function`, a name, and a body, a node of kind [`NodeKind::GlobalFunctionStat`].
    GlobalFunctionStat, GlobalFunctionStat
}

impl<'t> GlobalFunctionStat<'t> {
    /// Returns the function's name.
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken<'t>> {
        self.0.child_token(TokenKind::Name)
    }

    /// Returns the function's parameters and body.
    #[must_use]
    pub fn body(&self) -> Option<FuncBody<'t>> {
        self.0.child()
    }
}

view! {
    /// A `local` declaration, such as `local x <const>, y = 1, 2`, a node of kind
    /// [`NodeKind::LocalStat`].
    LocalStat, LocalStat
}

impl<'t> LocalStat<'t> {
    /// Returns the names, with their attributes.
    #[must_use]
    pub fn names(&self) -> Option<AttNameList<'t>> {
        self.0.child()
    }

    /// Returns the values, after the `=`, if there are any.
    #[must_use]
    pub fn values(&self) -> Option<ExprList<'t>> {
        self.0.child()
    }
}

view! {
    /// A `global` declaration, such as `global x <const> = 1` or `global *`, a node of kind
    /// [`NodeKind::GlobalStat`].
    GlobalStat, GlobalStat
}

impl<'t> GlobalStat<'t> {
    /// Returns the names, with their attributes, or `None` for `global *`.
    #[must_use]
    pub fn names(&self) -> Option<AttNameList<'t>> {
        self.0.child()
    }

    /// Returns the values, after the `=`, if there are any.
    #[must_use]
    pub fn values(&self) -> Option<ExprList<'t>> {
        self.0.child()
    }

    /// Returns the attribute of `global <const> *`. In a declaration with names, the attributes
    /// are in [`GlobalStat::names`].
    #[must_use]
    pub fn attrib(&self) -> Option<Attrib<'t>> {
        self.0.child()
    }

    /// Returns the `*` of `global *`.
    #[must_use]
    pub fn star(&self) -> Option<SyntaxToken<'t>> {
        self.0.child_token(TokenKind::Star)
    }
}

view! {
    /// A `nonlocal` declaration, such as `nonlocal count`, a node of kind
    /// [`NodeKind::NonlocalStat`].
    NonlocalStat, NonlocalStat
}

impl<'t> NonlocalStat<'t> {
    /// Returns the names it declares.
    #[must_use]
    pub fn names(&self) -> Option<NameList<'t>> {
        self.0.child()
    }
}

view! {
    /// `return` and its values, a node of kind [`NodeKind::ReturnStat`].
    ReturnStat, ReturnStat
}

impl<'t> ReturnStat<'t> {
    /// Returns the values, if there are any.
    #[must_use]
    pub fn values(&self) -> Option<ExprList<'t>> {
        self.0.child()
    }
}

view! {
    /// The names of a `local` or `global` declaration, a node of kind
    /// [`NodeKind::AttNameList`].
    AttNameList, AttNameList
}

impl<'t> AttNameList<'t> {
    /// Returns the attribute before the first name, which applies to each name without one of
    /// its own, as in `local <const> x, y`.
    #[must_use]
    pub fn attrib(&self) -> Option<Attrib<'t>> {
        self.0.child()
    }

    /// Returns the names, in order.
    pub fn names(&self) -> impl Iterator<Item = AttName<'t>> + use<'t> {
        self.0.children_of()
    }
}

view! {
    /// A name in a declaration and its attribute, a node of kind [`NodeKind::AttName`].
    AttName, AttName
}

impl<'t> AttName<'t> {
    /// Returns the name.
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken<'t>> {
        self.0.child_token(TokenKind::Name)
    }

    /// Returns the name's own attribute, if it has one.
    #[must_use]
    pub fn attrib(&self) -> Option<Attrib<'t>> {
        self.0.child()
    }
}

view! {
    /// An attribute, such as `<const>`, a node of kind [`NodeKind::Attrib`].
    Attrib, Attrib
}

impl<'t> Attrib<'t> {
    /// Returns the attribute's name, such as `const` or `close`.
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken<'t>> {
        self.0.child_token(TokenKind::Name)
    }
}

view! {
    /// Names with a `,` between each two, a node of kind [`NodeKind::NameList`].
    NameList, NameList
}

impl<'t> NameList<'t> {
    /// Returns the names, in order.
    pub fn names(&self) -> impl Iterator<Item = SyntaxToken<'t>> + use<'t> {
        self.0.child_tokens(TokenKind::Name)
    }
}

view! {
    /// A function's parameters, block, and `end`, a node of kind [`NodeKind::FuncBody`].
    FuncBody, FuncBody
}

impl<'t> FuncBody<'t> {
    /// Returns the parameter list.
    #[must_use]
    pub fn params(&self) -> Option<ParamList<'t>> {
        self.0.child()
    }

    /// Returns the function's block.
    #[must_use]
    pub fn block(&self) -> Option<Block<'t>> {
        self.0.child()
    }
}

view! {
    /// A function's parameters, in parentheses, a node of kind [`NodeKind::ParamList`].
    ParamList, ParamList
}

impl<'t> ParamList<'t> {
    /// Returns the named parameters, in order.
    pub fn params(&self) -> impl Iterator<Item = Param<'t>> + use<'t> {
        self.0.children_of()
    }

    /// Returns `...`, the last parameter, if there is one.
    #[must_use]
    pub fn vararg(&self) -> Option<VarargParam<'t>> {
        self.0.child()
    }
}

view! {
    /// A named parameter, a node of kind [`NodeKind::Param`].
    Param, Param
}

impl<'t> Param<'t> {
    /// Returns the parameter's name.
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken<'t>> {
        self.0.child_token(TokenKind::Name)
    }
}

view! {
    /// `...` as a parameter, and the vararg table's name if it has one, a node of kind
    /// [`NodeKind::VarargParam`].
    VarargParam, VarargParam
}

impl<'t> VarargParam<'t> {
    /// Returns the name of the vararg table, as in `function f(...args)`, if it has one.
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken<'t>> {
        self.0.child_token(TokenKind::Name)
    }
}
