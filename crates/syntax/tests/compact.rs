//! Tests of the compact tree through the public API: that it holds what the lossless tree built
//! from the same events holds, for each statement and expression, every conformance script, and
//! generated programs; the values of names and literals; the spans it keeps; each list of events
//! it refuses; and trees too deep for a builder that recurses.
//!
//! The shape of a tree is an S-expression that names each statement and expression with its
//! span and its parts, and leaves out keywords and symbols, which the compact tree doesn't keep.
//! One writer reads the compact tree and another reads the lossless tree through its typed views,
//! so two working-outs of each node's parts must agree.

use lunaferra_syntax::ast::{self, AstNode};
use lunaferra_syntax::compact::{
    AttName, Attrib, BinaryOp, Bytes, Call, CompareOp, Conversion, ExprId, ExprKind, FStringPart,
    FunctionId, List, Name, StatId, StatKind, TableField, TargetKind, UnaryOp, Vararg,
};
use lunaferra_syntax::{
    CompactTree, Error, Event, Lexed, NodeKind, Number, Origin, SourceId, Span, SyntaxElement,
    SyntaxNode, SyntaxToken, SyntaxTree, TokenKind, lex, numeral_value, parse,
};
use lunaferra_testkit::Config;

use program::generate_program;

mod program;

use NodeKind as N;

/// Lexes `source` as a string, as `load` reads one.
fn lexed(source: &[u8]) -> Lexed {
    lex(source, SourceId::new(0), Origin::String).expect("each test's source is short")
}

/// Parses `source`, lexed with `origin`, and builds both of its trees from the same events.
fn trees(source: &[u8], origin: Origin) -> (SyntaxTree, Result<CompactTree, Error>) {
    let lexed = lex(source, SourceId::new(0), origin).expect("each test's source is short");
    let events = parse(source, &lexed);
    let lossless =
        SyntaxTree::build(source, &lexed, &events).expect("the parser's events make a tree");
    (lossless, CompactTree::build(source, &lexed, &events))
}

/// Parses `source`, which must be valid, and builds its compact tree.
fn compact(source: &str) -> CompactTree {
    trees(source.as_bytes(), Origin::String)
        .1
        .expect("the source is valid")
}

/// Returns the shapes of both trees of `source`, lexed with `origin`: the compact tree's, or its
/// error, then the lossless tree's.
fn shapes(source: &[u8], origin: Origin) -> (String, String) {
    let (lossless, compact) = trees(source, origin);
    let compact = match compact {
        Ok(tree) => Compact(&tree).chunk(),
        Err(error) => format!("error: {error}"),
    };
    (compact, Lossless(&lossless).chunk())
}

/// Writes a span as `@start..end`.
fn at(span: Span) -> String {
    format!("@{}..{}", span.start, span.end)
}

/// Writes bytes as text, quoted, with each sequence that isn't UTF-8 replaced.
fn quoted(bytes: &[u8]) -> String {
    format!("{:?}", String::from_utf8_lossy(bytes))
}

/// Writes `parts` between brackets, with a space between each two.
fn list(parts: impl IntoIterator<Item = String>) -> String {
    format!("[{}]", parts.into_iter().collect::<Vec<_>>().join(" "))
}

/// Writes `part`, or `-` for a part that isn't there.
fn or_dash(part: Option<String>) -> String {
    part.unwrap_or_else(|| String::from("-"))
}

/// Writes an attribute, as `<const>` or `<close>`, or nothing.
fn attrib_text(attrib: Option<&str>) -> String {
    attrib.map_or_else(String::new, |name| format!("<{name}>"))
}

/// The source text of each binary operator.
fn binary_text(operator: BinaryOp) -> &'static str {
    match operator {
        BinaryOp::Or => "or",
        BinaryOp::And => "and",
        BinaryOp::BitOr => "|",
        BinaryOp::BitXor => "^",
        BinaryOp::BitAnd => "&",
        BinaryOp::ShiftLeft => "<<",
        BinaryOp::ShiftRight => ">>",
        BinaryOp::Add => "+",
        BinaryOp::Subtract => "-",
        BinaryOp::Multiply => "*",
        BinaryOp::Divide => "/",
        BinaryOp::FloorDivide => "//",
        BinaryOp::Modulo => "%",
        BinaryOp::Power => "**",
    }
}

/// The source text of each unary operator.
fn unary_text(operator: UnaryOp) -> &'static str {
    match operator {
        UnaryOp::Not => "not",
        UnaryOp::Negate => "-",
        UnaryOp::Plus => "+",
        UnaryOp::BitNot => "~",
        UnaryOp::Length => "#",
    }
}

/// The source text of each comparison operator, with one space between the words of two.
fn compare_text(operator: CompareOp) -> &'static str {
    match operator {
        CompareOp::Less => "<",
        CompareOp::LessEqual => "<=",
        CompareOp::Greater => ">",
        CompareOp::GreaterEqual => ">=",
        CompareOp::Equal => "==",
        CompareOp::NotEqual => "!=",
        CompareOp::In => "in",
        CompareOp::NotIn => "not in",
        CompareOp::Is => "is",
        CompareOp::IsNot => "is not",
    }
}

/// Writes the shape of a compact tree.
struct Compact<'t>(&'t CompactTree);

impl Compact<'_> {
    fn chunk(&self) -> String {
        format!("{} eof{}", self.block(self.0.block()), at(self.0.eof()))
    }

    fn block(&self, block: List<StatId>) -> String {
        list(self.0[block].iter().map(|&id| self.stat(id)))
    }

    fn name(&self, name: Name) -> String {
        format!("{}{}", quoted(&self.0[name.text]), at(name.span))
    }

    fn names(&self, names: List<Name>) -> String {
        list(self.0[names].iter().map(|&name| self.name(name)))
    }

    fn exprs(&self, exprs: List<ExprId>) -> String {
        list(self.0[exprs].iter().map(|&id| self.expr(id)))
    }

    fn att_names(&self, names: List<AttName>) -> String {
        list(self.0[names].iter().map(|att_name| {
            let attrib = att_name.attrib.map(|attrib| match attrib {
                Attrib::Const => "const",
                Attrib::Close => "close",
            });
            format!("{}{}", self.name(att_name.name), attrib_text(attrib))
        }))
    }

    fn function(&self, id: FunctionId) -> String {
        let function = self.0[id];
        let vararg = function.vararg.map(|vararg| match vararg {
            Vararg::Unnamed => String::from("..."),
            Vararg::Named(name) => format!("...{}", self.name(name)),
        });
        format!(
            "(function{} {} {} {})",
            at(function.span),
            self.names(function.params),
            or_dash(vararg),
            self.block(function.body)
        )
    }

    fn call(&self, call: Call) -> String {
        match call {
            Call::Function { callee, args } => format!(
                "fn {} (args{} {})",
                self.expr(callee),
                at(args.span),
                self.exprs(args.values)
            ),
            Call::Method {
                object,
                method,
                args,
            } => format!(
                "method {} {} (args{} {})",
                self.expr(object),
                self.name(method),
                at(args.span),
                self.exprs(args.values)
            ),
        }
    }

    fn stat(&self, id: StatId) -> String {
        let stat = self.0[id];
        let here = at(stat.span);
        match stat.kind {
            StatKind::Assign { targets, values } => {
                let targets = self.0[targets].iter().map(|target| {
                    let span = target.span;
                    match target.kind {
                        TargetKind::Name(name) => {
                            format!("(name{} {})", at(span), quoted(&self.0[name]))
                        }
                        TargetKind::Field { object, field } => format!(
                            "(field{} {} {})",
                            at(span),
                            self.expr(object),
                            self.name(field)
                        ),
                        TargetKind::Index { object, index } => format!(
                            "(index{} {} {})",
                            at(span),
                            self.expr(object),
                            self.expr(index)
                        ),
                    }
                });
                format!("(assign{here} {} {})", list(targets), self.exprs(values))
            }
            StatKind::Call(call) => format!("(call-stat{here} {})", self.call(call)),
            StatKind::Label(name) => format!("(label{here} {})", self.name(name)),
            StatKind::Break => format!("(break{here})"),
            StatKind::Goto(name) => format!("(goto{here} {})", self.name(name)),
            StatKind::Do(body) => format!("(do{here} {})", self.block(body)),
            StatKind::While { condition, body } => format!(
                "(while{here} {} {})",
                self.expr(condition),
                self.block(body)
            ),
            StatKind::Repeat { body, condition } => format!(
                "(repeat{here} {} {})",
                self.block(body),
                self.expr(condition)
            ),
            StatKind::If {
                branches,
                else_body,
            } => {
                let branches = self.0[branches].iter().map(|branch| {
                    format!(
                        "(branch {} {})",
                        self.expr(branch.condition),
                        self.block(branch.body)
                    )
                });
                let else_body = else_body.map(|body| self.block(body));
                format!("(if{here} {} {})", list(branches), or_dash(else_body))
            }
            StatKind::NumericFor {
                name,
                start,
                stop,
                step,
                body,
            } => format!(
                "(numeric-for{here} {} {} {} {} {})",
                self.name(name),
                self.expr(start),
                self.expr(stop),
                or_dash(step.map(|step| self.expr(step))),
                self.block(body)
            ),
            StatKind::GenericFor {
                names,
                values,
                body,
            } => format!(
                "(generic-for{here} {} {} {})",
                self.names(names),
                self.exprs(values),
                self.block(body)
            ),
            StatKind::Function {
                path,
                method,
                function,
            } => format!(
                "(function-stat{here} {} {} {})",
                self.names(path),
                or_dash(method.map(|method| self.name(method))),
                self.function(function)
            ),
            StatKind::LocalFunction { name, function } => format!(
                "(local-function{here} {} {})",
                self.name(name),
                self.function(function)
            ),
            StatKind::GlobalFunction { name, function } => format!(
                "(global-function{here} {} {})",
                self.name(name),
                self.function(function)
            ),
            StatKind::Local { names, values } => format!(
                "(local{here} {} {})",
                self.att_names(names),
                self.exprs(values)
            ),
            StatKind::Global { names, values } => format!(
                "(global{here} {} {})",
                self.att_names(names),
                self.exprs(values)
            ),
            StatKind::GlobalAll { attrib } => {
                let attrib = attrib.map(|attrib| match attrib {
                    Attrib::Const => String::from("<const>"),
                    Attrib::Close => String::from("<close>"),
                });
                format!("(global-all{here} {})", or_dash(attrib))
            }
            StatKind::Nonlocal(names) => format!("(nonlocal{here} {})", self.names(names)),
            StatKind::Return(values) => format!("(return{here} {})", self.exprs(values)),
        }
    }

    fn parts(&self, parts: List<FStringPart>) -> String {
        list(self.0[parts].iter().map(|part| match *part {
            FStringPart::Text(text) => quoted(&self.0[text]),
            FStringPart::Field(field) => {
                let conversion = field.conversion.map(|conversion| match conversion {
                    Conversion::Str => String::from("!s"),
                    Conversion::Repr => String::from("!r"),
                });
                format!(
                    "(field-part {} {} {} {})",
                    self.expr(field.expr),
                    or_dash(field.expr_text.map(|text| quoted(&self.0[text]))),
                    or_dash(conversion),
                    or_dash(field.spec.map(|spec| self.parts(spec)))
                )
            }
        }))
    }

    fn expr(&self, id: ExprId) -> String {
        let expr = self.0[id];
        let here = at(expr.span);
        match expr.kind {
            ExprKind::Nil => format!("(nil{here})"),
            ExprKind::True => format!("(true{here})"),
            ExprKind::False => format!("(false{here})"),
            ExprKind::Number(number) => format!("(number{here} {number:?})"),
            ExprKind::String(_) => format!("(string{here})"),
            ExprKind::Vararg => format!("(vararg{here})"),
            ExprKind::Name(name) => format!("(name{here} {})", quoted(&self.0[name])),
            ExprKind::Paren(inner) => format!("(paren{here} {})", self.expr(inner)),
            ExprKind::Field { object, field } => {
                format!("(field{here} {} {})", self.expr(object), self.name(field))
            }
            ExprKind::Index { object, index } => {
                format!("(index{here} {} {})", self.expr(object), self.expr(index))
            }
            ExprKind::Call(call) => format!("(call{here} {})", self.call(call)),
            ExprKind::Binary {
                left,
                operator,
                operator_span,
                right,
            } => format!(
                "(binary{here} {}{} {} {})",
                binary_text(operator),
                at(operator_span),
                self.expr(left),
                self.expr(right)
            ),
            ExprKind::Unary { operator, operand } => format!(
                "(unary{here} {} {})",
                unary_text(operator),
                self.expr(operand)
            ),
            ExprKind::Compare { first, rest } => {
                let rest = self.0[rest].iter().map(|comparison| {
                    format!(
                        "{}{} {}",
                        compare_text(comparison.operator),
                        at(comparison.operator_span),
                        self.expr(comparison.operand)
                    )
                });
                format!("(compare{here} {} {})", self.expr(first), list(rest))
            }
            ExprKind::Conditional {
                then_value,
                condition,
                else_value,
            } => format!(
                "(conditional{here} {} {} {})",
                self.expr(then_value),
                self.expr(condition),
                self.expr(else_value)
            ),
            ExprKind::Function(function) => {
                format!("(function-expr{here} {})", self.function(function))
            }
            ExprKind::Table(fields) => {
                let fields = self.0[fields].iter().map(|field| match *field {
                    TableField::Index { key, value } => {
                        format!("(index-field {} {})", self.expr(key), self.expr(value))
                    }
                    TableField::Named { name, value } => {
                        format!("(named-field {} {})", self.name(name), self.expr(value))
                    }
                    TableField::Positional(value) => format!("(positional {})", self.expr(value)),
                });
                format!("(table{here} {})", list(fields))
            }
            ExprKind::List(elements) => format!("(list{here} {})", self.exprs(elements)),
            ExprKind::Tuple(elements) => format!("(tuple{here} {})", self.exprs(elements)),
            ExprKind::FString(parts) => format!("(fstring{here} {})", self.parts(parts)),
        }
    }
}

/// Writes the shape of a lossless tree, through its typed views, as [`Compact`] writes the shape
/// of a compact tree. A part that a view doesn't find is `?`.
struct Lossless<'t>(&'t SyntaxTree);

/// Appends the tokens of `node` and of everything inside it to `out`, in order.
fn tokens_of<'t>(node: SyntaxNode<'t>, out: &mut Vec<SyntaxToken<'t>>) {
    for child in node.children() {
        match child {
            SyntaxElement::Node(node) => tokens_of(node, out),
            SyntaxElement::Token(token) => out.push(token),
        }
    }
}

/// Returns the span of `node`, from the start of its first token to the end of its last.
fn node_span(node: SyntaxNode<'_>) -> Option<(u32, u32)> {
    let mut tokens = Vec::new();
    tokens_of(node, &mut tokens);
    Some((tokens.first()?.span().start, tokens.last()?.span().end))
}

/// Writes the span of `node`, as [`at`] writes a span.
fn node_at(node: SyntaxNode<'_>) -> String {
    node_span(node).map_or_else(
        || String::from("@?"),
        |(start, end)| format!("@{start}..{end}"),
    )
}

/// Writes the value of an f-string's text, which in the sources these tests compare holds no
/// escape: each `{{` and `}}` is one brace.
fn text_value(token: SyntaxToken<'_>) -> String {
    let text = String::from_utf8_lossy(token.text());
    format!("{:?}", text.replace("{{", "{").replace("}}", "}"))
}

impl<'t> Lossless<'t> {
    fn chunk(&self) -> String {
        let chunk = ast::Chunk::cast(self.0.root());
        let block = chunk.and_then(|chunk| chunk.block());
        let eof = chunk
            .and_then(|chunk| chunk.eof())
            .map_or_else(|| String::from("?"), |token| at(token.span()));
        format!("{} eof{eof}", self.block(block))
    }

    fn block(&self, block: Option<ast::Block<'t>>) -> String {
        block.map_or_else(
            || String::from("?"),
            |block| list(block.statements().filter_map(|stat| self.stat(stat))),
        )
    }

    fn name(&self, token: Option<SyntaxToken<'t>>) -> String {
        token.map_or_else(
            || String::from("?"),
            |token| format!("{}{}", quoted(token.text()), at(token.span())),
        )
    }

    fn names(&self, names: Option<ast::NameList<'t>>) -> String {
        names.map_or_else(
            || String::from("?"),
            |names| list(names.names().map(|name| self.name(Some(name)))),
        )
    }

    fn exprs(&self, exprs: Option<ast::ExprList<'t>>) -> String {
        exprs.map_or_else(
            || String::from("[]"),
            |exprs| list(exprs.exprs().map(|expr| self.expr(Some(expr)))),
        )
    }

    fn att_names(&self, names: Option<ast::AttNameList<'t>>) -> String {
        let Some(names) = names else {
            return String::from("?");
        };
        let default = names.attrib().and_then(|attrib| attrib.name());
        list(names.names().map(|att_name| {
            let attrib = att_name
                .attrib()
                .and_then(|attrib| attrib.name())
                .or(default)
                .map(|name| String::from_utf8_lossy(name.text()).into_owned());
            format!(
                "{}{}",
                self.name(att_name.name()),
                attrib_text(attrib.as_deref())
            )
        }))
    }

    fn function(&self, body: Option<ast::FuncBody<'t>>) -> String {
        let Some(body) = body else {
            return String::from("?");
        };
        let params = body.params();
        let names = params.map_or_else(
            || String::from("?"),
            |params| list(params.params().map(|param| self.name(param.name()))),
        );
        let vararg = params.and_then(|params| params.vararg()).map(|vararg| {
            vararg.name().map_or_else(
                || String::from("..."),
                |name| format!("...{}", self.name(Some(name))),
            )
        });
        format!(
            "(function{} {} {} {})",
            node_at(body.syntax()),
            names,
            or_dash(vararg),
            self.block(body.block())
        )
    }

    fn args(&self, args: Option<ast::Args<'t>>) -> String {
        let Some(args) = args else {
            return String::from("?");
        };
        let values = if let Some(string) = args.string() {
            format!("[(string{})]", at(string.span()))
        } else if let Some(table) = args.table() {
            list([self.expr(Some(ast::Expr::Table(table)))])
        } else {
            self.exprs(args.exprs())
        };
        format!("(args{} {values})", node_at(args.syntax()))
    }

    fn call(&self, call: Option<ast::Expr<'t>>) -> String {
        match call {
            Some(ast::Expr::Call(call)) => {
                format!("fn {} {}", self.expr(call.callee()), self.args(call.args()))
            }
            Some(ast::Expr::MethodCall(call)) => format!(
                "method {} {} {}",
                self.expr(call.object()),
                self.name(call.method()),
                self.args(call.args())
            ),
            _ => String::from("?"),
        }
    }

    fn stat(&self, stat: ast::Stat<'t>) -> Option<String> {
        let here = node_at(stat.syntax());
        Some(match stat {
            ast::Stat::Empty(_) => return None,
            ast::Stat::Assign(stat) => format!(
                "(assign{here} {} {})",
                self.exprs(stat.targets()),
                self.exprs(stat.values())
            ),
            ast::Stat::Call(stat) => format!("(call-stat{here} {})", self.call(stat.call())),
            ast::Stat::Label(stat) => format!("(label{here} {})", self.name(stat.name())),
            ast::Stat::Break(_) => format!("(break{here})"),
            ast::Stat::Goto(stat) => format!("(goto{here} {})", self.name(stat.label())),
            ast::Stat::Do(stat) => format!("(do{here} {})", self.block(stat.block())),
            ast::Stat::While(stat) => format!(
                "(while{here} {} {})",
                self.expr(stat.condition()),
                self.block(stat.block())
            ),
            ast::Stat::Repeat(stat) => format!(
                "(repeat{here} {} {})",
                self.block(stat.block()),
                self.expr(stat.condition())
            ),
            ast::Stat::If(stat) => {
                let first = format!(
                    "(branch {} {})",
                    self.expr(stat.condition()),
                    self.block(stat.block())
                );
                let rest = stat.elseif_clauses().map(|clause| {
                    format!(
                        "(branch {} {})",
                        self.expr(clause.condition()),
                        self.block(clause.block())
                    )
                });
                let else_body = stat.else_clause().map(|clause| self.block(clause.block()));
                format!(
                    "(if{here} {} {})",
                    list(std::iter::once(first).chain(rest)),
                    or_dash(else_body)
                )
            }
            ast::Stat::NumericFor(stat) => format!(
                "(numeric-for{here} {} {} {} {} {})",
                self.name(stat.variable()),
                self.expr(stat.start()),
                self.expr(stat.stop()),
                or_dash(stat.step().map(|step| self.expr(Some(step)))),
                self.block(stat.block())
            ),
            ast::Stat::GenericFor(stat) => format!(
                "(generic-for{here} {} {} {})",
                self.names(stat.names()),
                self.exprs(stat.values()),
                self.block(stat.block())
            ),
            ast::Stat::Function(stat) => {
                let name = stat.name();
                let path = name.map_or_else(
                    || String::from("?"),
                    |name| list(name.path().map(|part| self.name(Some(part)))),
                );
                let method = name
                    .and_then(|name| name.method())
                    .map(|method| self.name(Some(method)));
                format!(
                    "(function-stat{here} {path} {} {})",
                    or_dash(method),
                    self.function(stat.body())
                )
            }
            ast::Stat::LocalFunction(stat) => format!(
                "(local-function{here} {} {})",
                self.name(stat.name()),
                self.function(stat.body())
            ),
            ast::Stat::GlobalFunction(stat) => format!(
                "(global-function{here} {} {})",
                self.name(stat.name()),
                self.function(stat.body())
            ),
            ast::Stat::Local(stat) => format!(
                "(local{here} {} {})",
                self.att_names(stat.names()),
                self.exprs(stat.values())
            ),
            ast::Stat::Global(stat) if stat.star().is_some() => {
                let attrib = stat
                    .attrib()
                    .and_then(|attrib| attrib.name())
                    .map(|name| format!("<{}>", String::from_utf8_lossy(name.text())));
                format!("(global-all{here} {})", or_dash(attrib))
            }
            ast::Stat::Global(stat) => format!(
                "(global{here} {} {})",
                self.att_names(stat.names()),
                self.exprs(stat.values())
            ),
            ast::Stat::Nonlocal(stat) => format!("(nonlocal{here} {})", self.names(stat.names())),
            ast::Stat::Return(stat) => format!("(return{here} {})", self.exprs(stat.values())),
            _ => String::from("?"),
        })
    }

    fn parts(&self, parts: impl Iterator<Item = ast::FStringPart<'t>>) -> String {
        list(parts.map(|part| match part {
            ast::FStringPart::Text(token) => text_value(token),
            ast::FStringPart::Field(field) => {
                let expr = field.expr();
                let expr_text = field.equal().and_then(|_| {
                    let (start, end) = node_span(expr?.syntax())?;
                    let source = self.0.source();
                    let range = usize::try_from(start).ok()?..usize::try_from(end).ok()?;
                    Some(quoted(source.get(range)?))
                });
                let conversion = field
                    .conversion()
                    .map(|token| String::from_utf8_lossy(token.text()).into_owned());
                let spec = field.spec().map(|spec| self.parts(spec.parts()));
                format!(
                    "(field-part {} {} {} {})",
                    self.expr(expr),
                    or_dash(expr_text),
                    or_dash(conversion),
                    or_dash(spec)
                )
            }
        }))
    }

    /// Writes a comparison chain's first operand, then each operator, with its span, and the
    /// operand after it.
    fn compare(&self, compare: ast::CompareExpr<'t>) -> String {
        let mut first = None;
        let mut rest = Vec::new();
        let mut operator: Vec<SyntaxToken<'t>> = Vec::new();
        for child in compare.syntax().children() {
            match child {
                SyntaxElement::Token(token) => operator.push(token),
                SyntaxElement::Node(node) => {
                    let operand = self.expr(ast::Expr::cast(node));
                    match (operator.first(), operator.last()) {
                        (Some(start), Some(end)) => {
                            let words: Vec<String> = operator
                                .iter()
                                .map(|token| String::from_utf8_lossy(token.text()).into_owned())
                                .collect();
                            rest.push(format!(
                                "{}@{}..{} {operand}",
                                words.join(" "),
                                start.span().start,
                                end.span().end
                            ));
                        }
                        _ => first = Some(operand),
                    }
                    operator.clear();
                }
            }
        }
        format!("{} {}", or_dash(first), list(rest))
    }

    fn expr(&self, expr: Option<ast::Expr<'t>>) -> String {
        let Some(expr) = expr else {
            return String::from("?");
        };
        let here = node_at(expr.syntax());
        match expr {
            ast::Expr::Name(expr) => format!(
                "(name{here} {})",
                or_dash(expr.name().map(|name| quoted(name.text())))
            ),
            ast::Expr::Paren(expr) => format!("(paren{here} {})", self.expr(expr.expr())),
            ast::Expr::Field(expr) => format!(
                "(field{here} {} {})",
                self.expr(expr.object()),
                self.name(expr.field())
            ),
            ast::Expr::Index(expr) => format!(
                "(index{here} {} {})",
                self.expr(expr.object()),
                self.expr(expr.index())
            ),
            ast::Expr::Call(_) | ast::Expr::MethodCall(_) => {
                format!("(call{here} {})", self.call(Some(expr)))
            }
            ast::Expr::Literal(expr) => match expr.token() {
                Some(token) => match token.kind() {
                    TokenKind::Nil => format!("(nil{here})"),
                    TokenKind::True => format!("(true{here})"),
                    TokenKind::False => format!("(false{here})"),
                    TokenKind::String => format!("(string{here})"),
                    _ => format!(
                        "(number{here} {})",
                        or_dash(numeral_value(token.text()).map(|number| format!("{number:?}")))
                    ),
                },
                None => String::from("?"),
            },
            ast::Expr::Vararg(_) => format!("(vararg{here})"),
            ast::Expr::Binary(expr) => {
                let operator = expr.operator().map(|token| {
                    format!(
                        "{}{}",
                        String::from_utf8_lossy(token.text()),
                        at(token.span())
                    )
                });
                format!(
                    "(binary{here} {} {} {})",
                    or_dash(operator),
                    self.expr(expr.left()),
                    self.expr(expr.right())
                )
            }
            ast::Expr::Unary(expr) => format!(
                "(unary{here} {} {})",
                or_dash(
                    expr.operator()
                        .map(|token| String::from_utf8_lossy(token.text()).into_owned())
                ),
                self.expr(expr.operand())
            ),
            ast::Expr::Compare(expr) => format!("(compare{here} {})", self.compare(expr)),
            ast::Expr::Conditional(expr) => format!(
                "(conditional{here} {} {} {})",
                self.expr(expr.then_value()),
                self.expr(expr.condition()),
                self.expr(expr.else_value())
            ),
            ast::Expr::Function(expr) => {
                format!("(function-expr{here} {})", self.function(expr.body()))
            }
            ast::Expr::Table(expr) => {
                let fields = expr.fields().map(|field| match field {
                    ast::TableField::Index(field) => format!(
                        "(index-field {} {})",
                        self.expr(field.key()),
                        self.expr(field.value())
                    ),
                    ast::TableField::Named(field) => format!(
                        "(named-field {} {})",
                        self.name(field.name()),
                        self.expr(field.value())
                    ),
                    ast::TableField::Positional(field) => {
                        format!("(positional {})", self.expr(field.value()))
                    }
                    _ => String::from("?"),
                });
                format!("(table{here} {})", list(fields))
            }
            ast::Expr::List(expr) => format!(
                "(list{here} {})",
                list(expr.elements().map(|element| self.expr(Some(element))))
            ),
            ast::Expr::Tuple(expr) => format!(
                "(tuple{here} {})",
                list(expr.elements().map(|element| self.expr(Some(element))))
            ),
            ast::Expr::FString(expr) => format!("(fstring{here} {})", self.parts(expr.parts())),
            _ => String::from("?"),
        }
    }
}

/// Sources of every statement and expression, in each form the parser reads, that the shape
/// tests compare.
const FORMS: &[&str] = &[
    "x = 1",
    "a, t.b, t[1] = 1, 2, 3",
    "f()",
    "f(1, 2)",
    "f 's'",
    "f {1}",
    "o:m(1)",
    "o:m 's'",
    "o:m {}",
    "::top::",
    "while true do break end",
    "goto top",
    "do end",
    "do x = 1 end",
    "repeat x = 1 until a",
    "if a then end",
    "if a then b() elseif c then d() elseif e then else f() end",
    "if a then else end",
    "for i = 1, 2 do end",
    "for i = 1, 2, 3 do x = i end",
    "for k, v in pairs(t), 1 do end",
    "function f() end",
    "function a.b.c:d(x, y, ...) end",
    "function f(...) end",
    "function f(...args) return args end",
    "local function f(a) return a end",
    "global function g() end",
    "local x",
    "local <const> a, b <close>, c = 1, 2",
    "local a <const>, b",
    "global x, y",
    "global <const> x = 1",
    "global *",
    "global <const> *",
    "nonlocal a, b",
    "return",
    "return;",
    "return 1, 2",
    "return 1;",
    ";;x = 1;",
    "x = nil, true, false, 12, 1.5, 0x10, 's', [[long]], ...",
    "x = (a), (f()), (...)",
    "x = a.b[c].d:e().f",
    "x = a or b and c",
    "x = a | b ^ c & d << e >> f + g - h * i / j // k % l ** m",
    "x = not a, -b, +c, ~d, #e, - -f",
    "x = a < b <= c > d >= e == f != g in h not in i is j is not k",
    "x = a < b, a is not b, a not in b",
    "x = a if b else c if d else e",
    "x = function(a, b) return a + b end",
    "x = {}",
    "x = {1, 2; k = 3, [4] = 5,}",
    "x = {[ [1] ] = 2, [ 3 ]}",
    "x = []",
    "x = [1, 2,]",
    "x = [ [1]]",
    "x = ()",
    "x = (1,)",
    "x = (1, 2)",
    "x = (1, 2,)",
    "x = f\"a{b}c{{d}}\"",
    "x = f\"{a=}{b!r}{c!s:>8}{d=!r:{w}.2}{e:}\"",
    "x = f[[long {x} text]]",
    "x = f\"{f\"{a}\"}\"",
    "x = f\"\"",
];

#[test]
fn holds_each_statement_and_expression_as_the_lossless_tree_does() {
    for source in FORMS {
        let (compact, lossless) = shapes(source.as_bytes(), Origin::String);
        assert!(!compact.contains('?'), "{source}: {compact}");
        assert_eq!(compact, lossless, "{source}");
    }
}

#[test]
fn holds_what_the_lossless_tree_holds_for_generated_programs() {
    Config::default()
        .with_cases(2_000)
        .assert(generate_program, |program| {
            let (compact, lossless) = shapes(program.as_bytes(), Origin::String);
            if compact == lossless && !compact.contains('?') {
                Ok(())
            } else {
                Err(format!("compact: {compact}\nlossless: {lossless}"))
            }
        });
}

#[test]
fn holds_what_the_lossless_tree_holds_for_every_conformance_script() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/conformance");
    let mut paths: Vec<_> = std::fs::read_dir(&root)
        .expect("the conformance folder exists")
        .map(|entry| entry.expect("readable").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "lua"))
        .collect();
    paths.sort();
    assert!(paths.len() >= 3);
    for path in paths {
        let source = std::fs::read(&path).expect("readable");
        let (compact, lossless) = shapes(&source, Origin::File);
        assert!(!compact.contains('?'), "{}: {compact}", path.display());
        assert_eq!(compact, lossless, "{}", path.display());
    }
}

/// Returns the single value that `source`, an assignment of one value, assigns.
fn value_of(tree: &CompactTree) -> ExprKind {
    let [stat] = tree[tree.block()] else {
        panic!("the source is one statement");
    };
    let StatKind::Assign { values, .. } = tree[stat].kind else {
        panic!("the statement is an assignment");
    };
    let [value] = tree[values] else {
        panic!("the assignment has one value");
    };
    tree[value].kind
}

/// Returns the value of the string that `source`, an assignment of one string, assigns.
fn string_value(source: &str) -> Vec<u8> {
    let tree = compact(source);
    let ExprKind::String(value) = value_of(&tree) else {
        panic!("{source} assigns a string");
    };
    tree[value].to_vec()
}

#[test]
fn reads_each_escape_of_a_short_string() {
    let cases: [(&str, &[u8]); 9] = [
        (r#"x = "plain""#, b"plain"),
        (r#"x = 'a\a\b\f\n\r\t\vz'"#, b"a\x07\x08\x0C\n\r\t\x0Bz"),
        (r#"x = "\\\"\'""#, b"\\\"'"),
        (r#"x = "\65\066\0679\x41\x7a""#, b"ABC9Az"),
        (
            r#"x = "\u{41}\u{E9}\u{7FFFFFFF}""#,
            b"A\xC3\xA9\xFD\xBF\xBF\xBF\xBF\xBF",
        ),
        ("x = \"a\\z  \n\t  b\"", b"ab"),
        ("x = \"a\\\nb\\\r\nc\"", b"a\nb\nc"),
        (r#"x = "\0\255""#, b"\0\xFF"),
        (r#"x = """#, b""),
    ];
    for (source, value) in cases {
        assert_eq!(string_value(source), value, "{source}");
    }
}

#[test]
fn reads_a_long_string_without_its_first_line_break_and_with_each_line_break_as_a_line_feed() {
    let cases: [(&str, &[u8]); 5] = [
        ("x = [[a]]", b"a"),
        ("x = [==[\r\nab\r\ncd\n\re\rf\ng]==]", b"ab\ncd\ne\nf\ng"),
        ("x = [[\n\nx]]", b"\nx"),
        ("x = [=[\\n ]] {{ ]=]", b"\\n ]] {{ "),
        ("x = [[]]", b""),
    ];
    for (source, value) in cases {
        assert_eq!(string_value(source), value, "{source:?}");
    }
}

#[test]
fn reads_each_numerals_value() {
    let cases = [
        ("x = 12", Number::Integer(12)),
        ("x = 0x10", Number::Integer(16)),
        ("x = 0xffffffffffffffff", Number::Integer(-1)),
        ("x = 1.5", Number::Float(1.5)),
        ("x = 0x1p4", Number::Float(16.0)),
        (
            "x = 9223372036854775808",
            Number::Float(9_223_372_036_854_775_808.0),
        ),
        ("x = 1e309", Number::Float(f64::INFINITY)),
    ];
    for (source, number) in cases {
        assert_eq!(
            value_of(&compact(source)),
            ExprKind::Number(number),
            "{source}"
        );
    }
}

/// Returns the parts of the f-string that `source`, an assignment of one f-string, assigns, with
/// each text as its bytes and each field as the field's expression's text, if it keeps one.
fn fstring_parts(source: &str) -> Vec<(Vec<u8>, Option<Vec<u8>>)> {
    let tree = compact(source);
    let ExprKind::FString(parts) = value_of(&tree) else {
        panic!("{source} assigns an f-string");
    };
    tree[parts]
        .iter()
        .map(|part| match *part {
            FStringPart::Text(text) => (tree[text].to_vec(), None),
            FStringPart::Field(field) => (
                b"{}".to_vec(),
                field.expr_text.map(|text| tree[text].to_vec()),
            ),
        })
        .collect()
}

#[test]
fn reads_the_text_of_an_f_string_as_a_string_of_its_form_with_braces_doubled() {
    let text = |bytes: &[u8]| (bytes.to_vec(), None);
    let field = (b"{}".to_vec(), None);
    assert_eq!(
        fstring_parts(r#"x = f"a\tb{{c}}d{x}e\x7B\u{7D}""#),
        [text(b"a\tb{c}d"), field.clone(), text(b"e{}")]
    );
    // A long f-string has no escapes, and its line breaks read as a long string's.
    assert_eq!(
        fstring_parts("x = f[==[\r\nline\r\nnext {{x}} \\t{y}]==]"),
        [text(b"line\nnext {x} \\t"), field.clone()]
    );
    assert_eq!(fstring_parts("x = f'{x}{{}}'"), [field, text(b"{}")]);
}

#[test]
fn reads_a_format_spec_as_text_and_fields_with_the_escapes_of_its_f_string() {
    let tree = compact(r#"x = f"{x:>{w}.2\x41}""#);
    let ExprKind::FString(parts) = value_of(&tree) else {
        panic!("the value is an f-string");
    };
    let [FStringPart::Field(field)] = tree[parts] else {
        panic!("the f-string is one field");
    };
    let spec = field.spec.expect("the field has a spec");
    let [
        FStringPart::Text(first),
        FStringPart::Field(width),
        FStringPart::Text(last),
    ] = tree[spec]
    else {
        panic!("the spec is text, a field, and text");
    };
    assert_eq!(&tree[first], b">");
    assert_eq!(&tree[name_of(&tree, width.expr)], b"w");
    assert_eq!(&tree[last], b".2A");
}

#[test]
fn keeps_the_source_text_of_a_fields_expression_before_an_equal_sign() {
    let field = |text: &[u8]| (b"{}".to_vec(), Some(text.to_vec()));
    assert_eq!(
        fstring_parts("x = f\"{a + b=}{ c =}{d -- note\n=}{e}\""),
        [
            field(b"a + b"),
            field(b"c"),
            field(b"d"),
            (b"{}".to_vec(), None)
        ]
    );
}

#[test]
fn reads_each_conversion_of_a_field() {
    let tree = compact(r#"x = f"{a!s}{b!r}{c}""#);
    let ExprKind::FString(parts) = value_of(&tree) else {
        panic!("the value is an f-string");
    };
    let conversions: Vec<Option<Conversion>> = tree[parts]
        .iter()
        .map(|part| match *part {
            FStringPart::Field(field) => field.conversion,
            FStringPart::Text(_) => panic!("the f-string is three fields"),
        })
        .collect();
    assert_eq!(
        conversions,
        [Some(Conversion::Str), Some(Conversion::Repr), None]
    );
}

#[test]
fn gives_each_declared_name_its_own_attribute_or_else_the_lists() {
    let attribs = |source: &str| -> Vec<Option<Attrib>> {
        let tree = compact(source);
        let [stat] = tree[tree.block()] else {
            panic!("{source} is one statement");
        };
        let (StatKind::Local { names, .. } | StatKind::Global { names, .. }) = tree[stat].kind
        else {
            panic!("{source} is a declaration");
        };
        tree[names].iter().map(|name| name.attrib).collect()
    };
    let (constant, close) = (Some(Attrib::Const), Some(Attrib::Close));
    assert_eq!(
        attribs("local <const> a, b <close>, c"),
        [constant, close, constant]
    );
    assert_eq!(attribs("local a <close>, b"), [close, None]);
    assert_eq!(attribs("global <const> a, b"), [constant, constant]);
    assert_eq!(attribs("global a"), [None]);
}

#[test]
fn keeps_the_spans_that_give_each_line_puc_rio_names() {
    let tree = compact("f\n(\n1\n+\n2\n)\nlocal function g\n(a)\nend\n");
    let [call, function] = tree[tree.block()] else {
        panic!("the source is two statements");
    };
    let line = |offset: u32| tree.lines().line_col(offset).0;
    let StatKind::Call(Call::Function { args, .. }) = tree[call].kind else {
        panic!("the first statement is a call");
    };
    // PUC-Rio takes a call's line from its `(`, and an operator's line from the operator.
    assert_eq!((line(tree[call].span.start), line(args.span.start)), (1, 2));
    let [sum] = tree[args.values] else {
        panic!("the call has one argument");
    };
    let ExprKind::Binary { operator_span, .. } = tree[sum].kind else {
        panic!("the argument is a sum");
    };
    assert_eq!(
        (line(tree[sum].span.start), line(operator_span.start)),
        (3, 4)
    );
    assert_eq!(line(tree[sum].span.end), 5);
    // A local function's first line is its `(`, and its last is its `end`.
    let StatKind::LocalFunction { function, .. } = tree[function].kind else {
        panic!("the second statement is a local function");
    };
    let span = tree[function].span;
    assert_eq!((line(span.start), line(span.end)), (8, 9));
    let eof = tree.eof();
    assert_eq!((eof.start, eof.end, line(eof.start)), (37, 37, 10));
}

#[test]
fn keeps_no_empty_statement() {
    let tree = compact(";;; x = 1 ;;");
    assert_eq!(tree[tree.block()].len(), 1);
    let empty = compact(";");
    assert!(empty[empty.block()].is_empty());
}

#[test]
fn refuses_a_source_with_a_lexical_error() {
    // The unfinished comment is in no token, and the malformed numeral is an error token.
    for source in [&b"x = 1 --[[ never closed"[..], b"x = 0x", b"x = \"\\q\""] {
        let lexed = lexed(source);
        let events = parse(source, &lexed);
        assert_eq!(
            CompactTree::build(source, &lexed, &events),
            Err(Error::SourceHasErrors),
            "{}",
            String::from_utf8_lossy(source)
        );
    }
}

#[test]
fn refuses_events_that_report_a_syntax_error() {
    let source = b"x = = 1";
    let lexed = lexed(source);
    let events = parse(source, &lexed);
    assert!(events.iter().any(|event| matches!(event, Event::Error(_))));
    assert_eq!(
        CompactTree::build(source, &lexed, &events),
        Err(Error::SourceHasErrors)
    );
}

/// Every node kind, so that the notation of [`events`] can name each one.
const KINDS: [NodeKind; 57] = [
    N::Chunk,
    N::Block,
    N::Error,
    N::EmptyStat,
    N::AssignStat,
    N::CallStat,
    N::LabelStat,
    N::BreakStat,
    N::GotoStat,
    N::DoStat,
    N::WhileStat,
    N::RepeatStat,
    N::IfStat,
    N::ElseifClause,
    N::ElseClause,
    N::NumericForStat,
    N::GenericForStat,
    N::FunctionStat,
    N::FuncName,
    N::LocalFunctionStat,
    N::GlobalFunctionStat,
    N::LocalStat,
    N::GlobalStat,
    N::NonlocalStat,
    N::ReturnStat,
    N::AttNameList,
    N::AttName,
    N::Attrib,
    N::NameList,
    N::FuncBody,
    N::ParamList,
    N::Param,
    N::VarargParam,
    N::ExprList,
    N::NameExpr,
    N::ParenExpr,
    N::FieldExpr,
    N::IndexExpr,
    N::CallExpr,
    N::MethodCallExpr,
    N::Args,
    N::LiteralExpr,
    N::VarargExpr,
    N::BinaryExpr,
    N::UnaryExpr,
    N::CompareExpr,
    N::ConditionalExpr,
    N::FunctionExpr,
    N::TableExpr,
    N::IndexField,
    N::NamedField,
    N::PositionalField,
    N::ListExpr,
    N::TupleExpr,
    N::FStringExpr,
    N::FStringField,
    N::FormatSpec,
];

/// Writes the events of `notation` over the tokens of `source`, lexed as a string: `(Kind`
/// starts a node of that kind, `)` finishes the node that is open, and `.` names the source's
/// next token.
fn events(source: &[u8], notation: &str) -> (Lexed, Vec<Event>) {
    let lexed = lexed(source);
    let mut tokens = lexed.tokens().iter();
    let spaced = notation.replace('(', " (").replace(')', " ) ");
    let events = spaced
        .split_whitespace()
        .map(|word| match word {
            ")" => Event::Finish,
            "." => {
                let token = tokens
                    .next()
                    .expect("the notation names no token past the end");
                Event::Token(token.kind, token.span)
            }
            _ => {
                let name = word.strip_prefix('(').expect("a word that starts a node");
                let kind = KINDS
                    .into_iter()
                    .find(|kind| format!("{kind:?}") == name)
                    .expect("a node kind's name");
                Event::Start(kind)
            }
        })
        .collect();
    (lexed, events)
}

/// Builds the compact tree of `source` from the events of `notation`.
fn build(source: &str, notation: &str) -> Result<CompactTree, Error> {
    let (lexed, events) = events(source.as_bytes(), notation);
    CompactTree::build(source.as_bytes(), &lexed, &events)
}

#[test]
fn builds_the_tree_of_the_notations_events_as_of_the_parsers() {
    // The notation writes the parser's events, so the tests below change one node at a time.
    let source = "return a, b";
    let notation = "(Chunk (Block (ReturnStat . (ExprList (NameExpr .) . (NameExpr .)))) .)";
    let lexed = lexed(source.as_bytes());
    assert_eq!(
        events(source.as_bytes(), notation).1,
        parse(source.as_bytes(), &lexed)
    );
    assert!(build(source, notation).is_ok());
}

#[test]
fn refuses_an_empty_node_of_every_kind_but_a_block() {
    for kind in KINDS {
        let notation = format!("(Chunk (Block ({kind:?})) .)");
        // An empty block is a block, but no statement, so the block around it is refused.
        let event = if kind == N::Block { 1 } else { 2 };
        assert_eq!(
            build("", &notation),
            Err(Error::MalformedNode { event }),
            "{kind:?}"
        );
    }
}

#[test]
fn refuses_each_node_whose_parts_dont_make_its_kind() {
    // Each case changes the parser's events for its source in one node, whose `Start` is the
    // event the error names.
    let cases = [
        // A call statement that holds no call, and an assignment to a literal.
        ("f", "(Chunk (Block (CallStat (NameExpr .))) .)", 2),
        (
            "1 = 2",
            "(Chunk (Block (AssignStat (ExprList (LiteralExpr .)) . (ExprList (LiteralExpr .)))) .)",
            2,
        ),
        // An `if` with a name where its clauses go.
        (
            "if a then b end",
            "(Chunk (Block (IfStat . (NameExpr .) . (Block) (NameExpr .) .)) .)",
            2,
        ),
        // A numeric `for` with a step but no comma before it.
        (
            "for i = 1, 2 3 do end",
            "(Chunk (Block (NumericForStat . . . (LiteralExpr .) . (LiteralExpr .) (LiteralExpr .) . (Block) .)) .)",
            2,
        ),
        // A function's name with no `.` between its parts.
        (
            "function a b() end",
            "(Chunk (Block (FunctionStat . (FuncName . .) (FuncBody (ParamList . .) (Block) .))) .)",
            4,
        ),
        // Declarations: a name where a declared name goes, two declared names with no comma, an
        // unknown attribute, and a value with no `=`.
        (
            "local a",
            "(Chunk (Block (LocalStat . (AttNameList (NameExpr .)))) .)",
            4,
        ),
        (
            "local a b",
            "(Chunk (Block (LocalStat . (AttNameList (AttName .) (AttName .)))) .)",
            4,
        ),
        (
            "local a <foo>",
            "(Chunk (Block (LocalStat . (AttNameList (AttName . (Attrib . . .))))) .)",
            7,
        ),
        (
            "local a b",
            "(Chunk (Block (LocalStat . (AttNameList (AttName .)) (NameExpr .))) .)",
            2,
        ),
        (
            "global a b",
            "(Chunk (Block (GlobalStat . (AttNameList (AttName .)) (NameExpr .))) .)",
            2,
        ),
        // A name list that holds an expression where a name goes.
        (
            "nonlocal a",
            "(Chunk (Block (NonlocalStat . (NameList (NameExpr .)))) .)",
            4,
        ),
        // A `nonlocal` statement whose first name isn't `nonlocal`.
        ("x a", "(Chunk (Block (NonlocalStat . (NameList .))) .)", 2),
        // Parameters: two with no comma, a comma before a vararg with no parameter, and a name
        // where a parameter goes.
        (
            "function f(a b) end",
            "(Chunk (Block (FunctionStat . (FuncName .) (FuncBody (ParamList . (Param .) (Param .) .) (Block) .))) .)",
            8,
        ),
        (
            "function f(, ...) end",
            "(Chunk (Block (FunctionStat . (FuncName .) (FuncBody (ParamList . . (VarargParam .) .) (Block) .))) .)",
            8,
        ),
        (
            "function f(a) end",
            "(Chunk (Block (FunctionStat . (FuncName .) (FuncBody (ParamList . (NameExpr .) .) (Block) .))) .)",
            8,
        ),
        // Expression lists: a block among the values, two values with no comma, and a comma after
        // the last value; and a parameter list with a comma after its last parameter.
        (
            "return a,",
            "(Chunk (Block (ReturnStat . (ExprList (NameExpr .) . (Block)))) .)",
            4,
        ),
        (
            "return a b",
            "(Chunk (Block (ReturnStat . (ExprList (NameExpr .) (NameExpr .)))) .)",
            4,
        ),
        (
            "return a,",
            "(Chunk (Block (ReturnStat . (ExprList (NameExpr .) .))) .)",
            4,
        ),
        (
            "function f(a,) end",
            "(Chunk (Block (FunctionStat . (FuncName .) (FuncBody (ParamList . (Param .) . .) (Block) .))) .)",
            8,
        ),
        // Arguments that are a name, and a literal that is a name.
        (
            "f a",
            "(Chunk (Block (CallStat (CallExpr (NameExpr .) (Args (NameExpr .))))) .)",
            7,
        ),
        (
            "return a",
            "(Chunk (Block (ReturnStat . (ExprList (LiteralExpr .)))) .)",
            5,
        ),
        // Operators that aren't: a comma as a binary operator, `*` as a unary one, and `+` as a
        // comparison; then a comparison with an operand too many, and one with no operator.
        (
            "return a, b",
            "(Chunk (Block (ReturnStat . (ExprList (BinaryExpr (NameExpr .) . (NameExpr .))))) .)",
            5,
        ),
        (
            "return * a",
            "(Chunk (Block (ReturnStat . (ExprList (UnaryExpr . (NameExpr .))))) .)",
            5,
        ),
        (
            "return a + b",
            "(Chunk (Block (ReturnStat . (ExprList (CompareExpr (NameExpr .) . (NameExpr .))))) .)",
            5,
        ),
        (
            "return a < b c",
            "(Chunk (Block (ReturnStat . (ExprList (CompareExpr (NameExpr .) . (NameExpr .) (NameExpr .))))) .)",
            5,
        ),
        // Tables: a name where a field goes, and two fields with no separator.
        (
            "return {a}",
            "(Chunk (Block (ReturnStat . (ExprList (TableExpr . (NameExpr .) .)))) .)",
            5,
        ),
        (
            "return {1 2}",
            "(Chunk (Block (ReturnStat . (ExprList (TableExpr . (PositionalField (LiteralExpr .)) (PositionalField (LiteralExpr .)) .)))) .)",
            5,
        ),
        // Lists and tuples with no comma between two elements, and a tuple of one element and no
        // comma, which is a parenthesized expression.
        (
            "return [a b]",
            "(Chunk (Block (ReturnStat . (ExprList (ListExpr . (NameExpr .) (NameExpr .) .)))) .)",
            5,
        ),
        (
            "return (a b)",
            "(Chunk (Block (ReturnStat . (ExprList (TupleExpr . (NameExpr .) (NameExpr .) .)))) .)",
            5,
        ),
        (
            "return (a)",
            "(Chunk (Block (ReturnStat . (ExprList (TupleExpr . (NameExpr .) .)))) .)",
            5,
        ),
        // F-strings: a field's tokens outside a field, a spec's text with no spec, and a field's
        // tokens in a spec.
        (
            "return f\"{a}\"",
            "(Chunk (Block (ReturnStat . (ExprList (FStringExpr . . (NameExpr .) . .)))) .)",
            5,
        ),
        (
            "return f\"{a:b}\"",
            "(Chunk (Block (ReturnStat . (ExprList (FStringExpr . (FStringField . (NameExpr .) . . .) .)))) .)",
            7,
        ),
        (
            "return f\"{a:{b}}\"",
            "(Chunk (Block (ReturnStat . (ExprList (FStringExpr . (FStringField . (NameExpr .) (FormatSpec . . (NameExpr .) .) .) .)))) .)",
            12,
        ),
    ];
    for (source, notation, event) in cases {
        assert_eq!(
            build(source, notation),
            Err(Error::MalformedNode { event }),
            "{source}"
        );
    }
}

#[test]
fn refuses_a_root_that_isnt_a_chunk_and_a_chunk_inside_a_node_left_open() {
    assert_eq!(
        build("", "(Block (Chunk (Block) .))"),
        Err(Error::MalformedNode { event: 0 })
    );
    // The inner chunk takes the last token, and the outer one is never finished.
    assert_eq!(
        build("", "(Chunk (Chunk (Block) .)"),
        Err(Error::EventsEndEarly)
    );
}

#[test]
fn refuses_events_that_make_no_tree() {
    let cases = [
        ("", "", Error::EventsEndEarly),
        ("", "(Chunk (Block) .", Error::EventsEndEarly),
        (
            "",
            ". (Chunk (Block))",
            Error::EventOutsideRoot { event: 0 },
        ),
        (
            "",
            ") (Chunk (Block) .)",
            Error::EventOutsideRoot { event: 0 },
        ),
        (
            "",
            "(Chunk (Block) .) (Chunk",
            Error::EventOutsideRoot { event: 5 },
        ),
        (
            "",
            "(Chunk (Block) .) )",
            Error::EventOutsideRoot { event: 5 },
        ),
    ];
    for (source, notation, error) in cases {
        assert_eq!(build(source, notation), Err(error), "{notation}");
    }
    // A token event must name the next token by its kind and its span.
    let lexed = lexed(b"a");
    let [name, eof] = *lexed.tokens() else {
        panic!("`a` is one token, then the end");
    };
    let wrong = [
        Event::Token(TokenKind::String, name.span),
        Event::Token(name.kind, eof.span),
        Event::Token(eof.kind, eof.span),
    ];
    for token in wrong {
        let events = [
            Event::Start(N::Chunk),
            Event::Start(N::Block),
            token.clone(),
        ];
        assert_eq!(
            CompactTree::build(b"a", &lexed, &events),
            Err(Error::TokenOutOfOrder { event: 2 }),
            "{token:?}"
        );
    }
}

#[test]
fn describes_each_error_it_refuses_with() {
    assert_eq!(
        Error::SourceHasErrors.to_string(),
        "the source has a lexical or syntax error"
    );
    assert_eq!(
        Error::MalformedNode { event: 7 }.to_string(),
        "the node that event 7 starts doesn't hold the parts of its kind"
    );
}

#[test]
fn builds_and_drops_a_chain_of_100_000_additions_and_fields_without_recursion() {
    const LENGTH: usize = 100_000;
    let sum = format!("x = a{}", " + a".repeat(LENGTH));
    let tree = compact(&sum);
    let mut expr = match value_of(&tree) {
        ExprKind::Binary { left, .. } => left,
        other => panic!("the value is a sum, not {other:?}"),
    };
    let mut depth = 1;
    while let ExprKind::Binary { left, operator, .. } = tree[expr].kind {
        assert_eq!(operator, BinaryOp::Add);
        expr = left;
        depth += 1;
    }
    assert_eq!(depth, LENGTH);
    let copy = tree.clone();
    assert_eq!(copy, tree);
    drop(copy);

    let fields = format!("x = a{}", ".b".repeat(LENGTH));
    let tree = compact(&fields);
    let mut expr = match value_of(&tree) {
        ExprKind::Field { object, .. } => object,
        other => panic!("the value is a field, not {other:?}"),
    };
    let mut depth = 1;
    while let ExprKind::Field { object, .. } = tree[expr].kind {
        expr = object;
        depth += 1;
    }
    assert_eq!(depth, LENGTH);
}

#[test]
fn builds_and_drops_100_000_nested_parentheses_without_recursion() {
    // The parser calls itself for each parenthesis until M1.8 limits the depth, so these events
    // are written out.
    const DEPTH: usize = 100_000;
    let source = format!("x = {}a{}", "(".repeat(DEPTH), ")".repeat(DEPTH));
    let lexed = lexed(source.as_bytes());
    let mut tokens = lexed
        .tokens()
        .iter()
        .map(|token| Event::Token(token.kind, token.span));
    let mut next = || tokens.next().expect("a token");
    let mut events = vec![
        Event::Start(N::Chunk),
        Event::Start(N::Block),
        Event::Start(N::AssignStat),
        Event::Start(N::ExprList),
        Event::Start(N::NameExpr),
        next(),
        Event::Finish,
        Event::Finish,
        next(),
        Event::Start(N::ExprList),
    ];
    for _ in 0..DEPTH {
        events.extend([Event::Start(N::ParenExpr), next()]);
    }
    events.extend([Event::Start(N::NameExpr), next(), Event::Finish]);
    for _ in 0..DEPTH {
        events.extend([next(), Event::Finish]);
    }
    events.extend([
        Event::Finish,
        Event::Finish,
        Event::Finish,
        next(),
        Event::Finish,
    ]);
    let tree =
        CompactTree::build(source.as_bytes(), &lexed, &events).expect("the events make a tree");
    let mut expr = match value_of(&tree) {
        ExprKind::Paren(inner) => inner,
        other => panic!("the value is in parentheses, not {other:?}"),
    };
    let mut depth = 1;
    while let ExprKind::Paren(inner) = tree[expr].kind {
        expr = inner;
        depth += 1;
    }
    assert_eq!(depth, DEPTH);
    assert_eq!(&tree[name_of(&tree, expr)], b"a");
}

/// Returns the bytes of the name that `expr` is.
fn name_of(tree: &CompactTree, expr: ExprId) -> Bytes {
    let ExprKind::Name(name) = tree[expr].kind else {
        panic!("the expression is a name");
    };
    name
}
