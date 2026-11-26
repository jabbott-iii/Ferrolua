# Subsystem: `lunaferra-syntax`

`lunaferra-syntax` turns Lunaferra source into tokens and trees. It has one parser that emits
events, and two builders that consume them: a lossless syntax tree for the tools and a compact tree
for the compiler (ADR-0009: one parser for the compiler and the tools). It also defines
spans, line maps, and the `Diagnostic` type every tool reports ([`diagnostics.md`](diagnostics.md)).
It accepts the language of [`language.md`](language.md), Lua 5.5's syntax with Python's operators
(ADR-0023: a language of its own, with Python's operators and 0-based indexes), never panics on any
input, and enforces the size and nesting limits in [`sandbox.md`](sandbox.md). It lands in M1; the
syntax of compound assignment, types, classes, and strict mode lands in M4.

## Why one parser with two outputs

The compiler wants a fast parse of valid code and a small tree. The formatter needs every comment
and space; the language server needs a tree for code that is half typed. Two parsers would accept
different programs over time. One parser that reports what it sees as events lets each consumer
build only what it needs, and both see the same program. The cost is one layer of indirection
that a parser written only for the compiler wouldn't have.

## Source and spans

Lua source is bytes, not necessarily UTF-8, so every position is a byte offset, and every column
counts bytes. Built in M1.1 as described here:

```rust
pub const MAX_SOURCE_LEN: u32 = u32::MAX - 1;

pub struct SourceId(u32);   // SourceId::new(index) makes one, and id.index() reads it back
pub struct Span { pub source: SourceId, pub start: u32, pub end: u32 }   // end excluded
pub struct LineMap { /* byte offsets of line starts */ }

impl LineMap {
    pub fn new(source: &[u8]) -> Result<LineMap, Error>;   // Error::SourceTooLong
    pub fn line_col(&self, offset: u32) -> (u32, u32);    // 1-based, columns in bytes
}
```

Lines end at `\n`, `\r`, `\r\n`, or `\n\r`, as `inclinenumber` in PUC-Rio Lua 5.5.1's `llex.c`
counts them: it takes the other byte of a pair along when it comes next, so `\n\n` and `\r\r` end
two lines. A line break belongs to the line it ends, so the end of a source that ends with one is
on an empty last line. `line_col` gives an offset past the end of the source the position of the
end, so an offset from damaged input can't make it panic.

A source longer than `MAX_SOURCE_LEN` bytes is rejected before lexing: `LineMap::new` returns
`Error::SourceTooLong`. The limit is one byte below `u32::MAX`, so that every offset, the end of
the source included, and every 1-based line and column fit in a `u32`. In a source of `u32::MAX`
bytes with no line break, the column just past the last byte would be `2^32`. The source size
limit of [`sandbox.md`](sandbox.md#limits), 16 MB by default, is far lower; M1.8, the task that
adds the parser's size and nesting limits, will enforce it.

## Lexer

The lexer reads Lua 5.5's tokens (§3.1 of the manual), apart from `..` and `~=`, and the new ones
of [`language.md`](language.md#tokens-and-keywords), such as `**`, `!=`, and `**=`:

- names, the 24 reserved words, which are Lua 5.5's 23 and `is`, and the contextual keywords of
  `language.md`, which it reads as names;
- integers and floats in decimal and hexadecimal, including hexadecimal floats such as `0x1p4`,
  with the same overflow rules as PUC-Rio: a decimal integer that doesn't fit becomes a float;
- short strings with every escape, `\z`, `\xXX`, `\ddd`, and `\u{XXX}` up to `7FFFFFFF`, which is
  2^31 − 1, and long strings and comments with any level of `=` signs;
- f-strings: an `f` touching a short or long string starts one (ADR-0025: f-strings, locals by
  default, lists, tuples, and sets, and Python's truth). The lexer splits it into text parts and
  fields, and lexes each field's expression into ordinary tokens, counting brackets and skipping
  strings to find the field's end, so the parser reads a field as an expression;
- in a file, a UTF-8 byte order mark at the start, then a first line starting with `#`, which are
  skipped as PUC-Rio's `luaL_loadfilex` skips them for `lua`, `dofile`, `loadfile`, and
  `require`. A string that `load` reads skips neither, so there `#` is the length operator, as in
  PUC-Rio.

Each token keeps its leading and trailing trivia, the spaces, newlines, and comments around it, so
the lossless tree can reproduce the source exactly.

Built in M1.2 as described here:

```rust
pub enum Origin { File, String }   // a file skips its byte order mark and `#` line
pub fn lex(source: &[u8], id: SourceId, origin: Origin) -> Result<Lexed, Error>;   // SourceTooLong

pub struct Lexed { /* tokens, trivia, errors, and the line map */ }
impl Lexed {
    pub fn tokens(&self) -> &[Token];   // the last is always TokenKind::Eof
    pub fn trivia(&self) -> &[Trivia];
    pub fn leading(&self, token: &Token) -> &[Trivia];
    pub fn trailing(&self, token: &Token) -> &[Trivia];
    pub fn errors(&self) -> &[LexError];
    pub fn lines(&self) -> &LineMap;
}

pub struct Token { pub kind: TokenKind, pub span: Span, /* where its trivia is */ }
pub struct Trivia { pub kind: TriviaKind, pub span: Span }
pub enum TriviaKind { Whitespace, LineBreak, Comment, LongComment, Shebang, ByteOrderMark }

pub struct LexError { pub kind: LexErrorKind, pub span: Span, pub line: u32, pub near: Near }
pub enum Near { Text(Vec<u8>), Eof }
impl LexError {
    pub fn message(&self) -> Vec<u8>;   // such as `malformed number near '0x'`
}

pub enum Number { Integer(i64), Float(f64) }
pub fn numeral_value(text: &[u8]) -> Option<Number>;   // as `luaO_str2num` reads a numeral
```

### Tokens and trivia

Every byte belongs to exactly one token or one piece of trivia. A token's trailing trivia is the
spaces and comments after it on its line, up to the next line break; its leading trivia is the
rest of the trivia since the token before it, and the `Eof` token's leading trivia holds what
follows the last token. A long comment that starts on a token's line is trailing trivia, even when
it ends on a later line. Spaces inside an f-string's text are text, so the tokens there have no
trivia.

`TokenKind` has a kind for each reserved word, `Name`, `Integer`, `Float`, `String` for short and
long strings, the parts of an f-string, and each operator and symbol. Three more kinds carry what
the parser reports: `DotDot` and `TildeEqual` are Lua's `..` and `~=`, read so the parser can name
`+` and `!=`; `Unknown` is one byte that starts no token, such as `@`, a lone `!`, or a byte
outside ASCII, which PUC-Rio's parser reports as an `unexpected symbol`; and `Error` holds bytes
the lexer couldn't read. The lexer takes the longest token it can, so `list<list<T>>` ends with
`>>`; the parser of type annotations (M4.3) splits it.

A numeral reaches as far as PUC-Rio's `read_numeral` reads: every hexadecimal digit and `.` that
follows, an exponent mark with its sign, and one letter or `_` touching the end, so `3x` and `1..2`
are each one malformed numeral, while `3-4` is three tokens. `numeral_value` then gives its value
as `luaO_str2num` does: an integer when it is one, with hexadecimal integers wrapping around
modulo 2^64, and otherwise a float if C's `strtod` reads all of it. Lunaferra converts decimal
floats with the standard library and hexadecimal floats with its own code, both rounding to the
nearest float as glibc's `strtod` does, so `1e309` is infinite and `0x1p-1075` is 0.

### f-strings as tokens

An f-string comes out as a run of tokens. For `f"a{x!r:>8}b"` they are `FStringStart` (`f"`),
`FStringText` (`a`), `FStringFieldStart` (`{`), `Name` (`x`), `FStringConversion` (`!r`),
`FStringSpec` (`:`), `FStringText` (`>8`), `FStringFieldEnd` (`}`), `FStringText` (`b`), and
`FStringEnd` (`"`). `FStringStart` of a long f-string takes the line break after the bracket, which
isn't part of the text, as in a long string. A spec may hold fields of its own, which come out the
same way, and a field may hold f-strings of its own. The lexer keeps the f-strings, fields, and
specs it is inside on a stack, not in recursive calls, so 100,000 nested f-strings can't overflow
the call stack. [`language.md`](language.md#formatted-string-literals-adr-0025) gives the rules
that decide where a field and its spec end.

### Lexical errors

Each lexical error has the words of PUC-Rio Lua 5.5.1's `llex.c`, such as
`unfinished long string (starting at line 3) near <eof>`, apart from the one for a single `}` in an
f-string, which `language.md` records. Each also has the line PUC-Rio's lexer is on when it finds
the mistake: for an unfinished string, the line where the string's line or the source ends.
The text after `near` is what PUC-Rio's buffer holds. For a numeral or a long bracket, that is its
bytes so far; for a short string, it is the opening quote and the bytes that each byte and escape
so far stands for, followed by the bytes of the escape that failed, so `"\65\q"` gives
`invalid escape sequence near '"A\q'`. As in PUC-Rio, that text stops at its first zero byte.

PUC-Rio stops at the first error. Lunaferra's lexer goes on, so the tools see the whole source: it
makes the bytes an `Error` token, reports the first error in each token and in each stretch of an
f-string's text, and, after an escape that fails, goes on from the byte that failed, so the quote
in `"\x"` still ends the string. Only the first error of a source is the one PUC-Rio reports.

Two of `llex.c`'s errors aren't reproduced, because each needs a source of gigabytes, which no test
can build. `chunk has too many lines` needs 2^31 − 1 lines, so a source of at least 2 GB.
`lexical element too long` needs a token of two thirds of PUC-Rio's `MAX_SIZE`: about 2.9 GB on a
32-bit target such as the web, and more than any source on a 64-bit one. Both are far past the
16 MB source limit of [`sandbox.md`](sandbox.md#limits), but within `Options::trusted()`, which
sets none; M1.8, the task that enforces the source limit, decides how to handle them.

## Parser and events

The parser is hand-written, with recursive descent for statements and precedence climbing for
expressions, using the precedence table of [`language.md`](language.md#precedence). It emits
events, which list the tree in the order a reader meets its parts: `Start` opens a node inside the
node that is open, `Token` adds the source's next token to the node that is open, `Finish`
closes it, and `Error` reports a syntax error.

Built in M1.4 as described here, with the statements and the expressions that statements are built
from; M1.5 added the rest of the expressions:

```rust
pub fn parse(source: &[u8], lexed: &Lexed) -> Vec<Event>;

pub enum Event {
    Start(NodeKind),
    Token(TokenKind, Span),
    Finish,
    Error(ParseError),   // a syntax error, with PUC-Rio's message and line
}

pub struct ParseError {
    pub kind: ParseErrorKind,
    pub span: Span,
    pub line: u32,
    pub near: Option<Near>,   // None for the errors PUC-Rio writes without `near`
}
impl ParseError {
    pub fn message(&self) -> Vec<u8>;   // such as `'then' expected near <eof>`
}
pub enum ParseErrorKind {
    Expected(TokenKind), Unclosed { expected: TokenKind, opener: TokenKind, line: u32 },
    UnexpectedSymbol, SyntaxError, EqualOrInExpected, NameOrVarargExpected,
    FunctionArgumentsExpected, BreakOutsideLoop, VarargOutsideVarargFunction,
    UnknownAttribute { name: Vec<u8> }, MultipleToBeClosed, GlobalToBeClosed,
    LuaConcatenation, LuaNotEqual, LuaBitwiseXor,   // Lunaferra's own, for `..`, `~=`, and `~`
}
```

The parser follows PUC-Rio Lua 5.5.1's `lparser.c` rule by rule, so it accepts the same statements
and reports each mistake in the same words, on the same line. It also makes the checks that
`lparser.c` makes while it parses and that need nothing but the statement and the constructs
around it: `break outside loop`, `cannot use '...' outside a vararg function`, an unknown
attribute, a second `<close>` variable in one `local` statement, and a `<close>` attribute in a
`global` declaration. The checks that need names resolved are the compiler's (M2.5, M2.7): a
label defined twice, a `goto` with no visible label, and an assignment to a `<const>` variable.
PUC-Rio's `variable 'x' not declared`, which Lua 5.5's implicit `global *` gives after a `global`
declaration, has no place in Lunaferra, which drops that rule (ADR-0026). Lunaferra reserves
`global`, so its messages for a broken `global` statement are those of PUC-Rio built with
`LUA_COMPAT_GLOBAL` set to 0, such as `<name> expected near <eof>` for `global` alone.

`nonlocal` is a name everywhere but at the start of a statement before a name, so the parser looks
one token ahead to read it as a keyword ([`language.md`](language.md#scoping-adr-0025)).

Expressions follow the grammar of [`language.md`](language.md#precedence), read by one function
that climbs the precedence table, as PUC-Rio's `subexpr` climbs Lua's: it reads an operand, then
each binary operator that binds more tightly than the level it was called for, with that
operator's right operand. Each operator but `**` groups from left to right, so a chain of 100,000
`+` is a loop, not 100,000 nested calls, and a chain of comparisons is one node, whatever its
length. Unary `not` may start only an operand of `and`, `or`, or `not`, as in Python. The operands
are literals, `...`, f-strings, function expressions, table constructors, lists, tuples, and the
prefix expressions of Lua, a name or a parenthesized expression followed by fields, indexes,
calls, and method calls. A call's argument may be `(...)`, a string, or a table constructor, as in
Lua. A tuple is not a prefix expression, so no field, index, or call follows it. After an operand,
Lua's `..`, `~=`, and binary `~` each stop the parse with a message that names the operator to
write instead, which `language.md` records.

A table field that starts with `[` is read as PUC-Rio reads a key, `[`, an expression, and `]`,
until the token after them: `=` makes the field a key and a value, and anything else makes the
brackets a list literal, which the field's value starts with. A `,` after the first expression
makes a list at once. Either way, no token is read twice.

### Markers

A node's `Start` must come before its first token, but the parser often learns what a node is only
after reading part of it: `t.x` is a field only once the `.` follows `t`, and `f(x)` is a call
statement only once no `=` follows it. So the parser writes its events to a list of its own first.
Starting a node writes a placeholder, and completing it gives the placeholder a kind. A node can
also start before a node the parser has already completed, which links the completed node to the
new one, and an abandoned node, such as the name list that a numeric `for` turns out not to have,
adds nothing. When the parse is done, each placeholder becomes the `Start` events of its own node
and of every node linked to it, outermost first. That is how the parser wraps a field, an index, a
call, or a method call around the expression before it, a call statement or an assignment around
its first expression, and a binary, chained, or conditional node around its left operand. Each
step takes constant time, so a chain
of 100,000 fields parses in time proportional to its length.

An `if` after an expression may start a conditional expression or an `if` statement
([`language.md`](language.md#conditional-expressions)). The parser reads the `if` and the
condition as a conditional expression's. When `else` follows, it starts the conditional node
around the value before the `if`. When anything else follows, and the statement may end after the
expression, which it may after the last value of an assignment, a `local` or `global`
declaration, or after the condition of `until`, the parser moves the events of the `if` and the
condition out of the list it is writing and keeps them aside. The statement then finishes without
them, and the block's next statement is an `if` statement that starts with those events and goes
on from the condition. Each event moves once, so this takes time in proportion to the condition's
length. Anywhere else, such as after `return` or in an argument, the `if` can only start a
conditional expression, so a token other than `else` there is an `'else' expected` error.

After `(` and an expression, a comma makes a tuple and `)` makes a parenthesized expression, and
`()` is the empty tuple; the parser decides after that expression in the same way. List and
tuple literals, and the fields of f-strings, parse as
[`language.md`](language.md#lists-tuples-and-sets-adr-0025) and
[`language.md`](language.md#formatted-string-literals-adr-0025) define them.

<a id="stopping-at-the-first-error"></a>

### Recovery

PUC-Rio stops at a source's first error, but an editor needs the whole file, so from M1.6 the
parser goes on after each error, and the events still make a tree over every token. Built in M1.6
as described here.

When the parser meets a token it doesn't expect, it emits an `Error` event, then puts the tokens
from that one up to the next *stop* in an `Error` node, inside the node it was reading. A stop is
a token that can start or end a statement, or close what is open:

- anywhere, `end`, `else`, `elseif`, `until`, `then`, `do`, `;`, `::`, and the reserved words
  that start only a statement: `local`, `global`, `return`, `break`, `goto`, `while`, `for`, and
  `repeat`;
- in a block, a name, `function`, or `if` at the start of a line, which may start the next
  statement; inside the brackets of a table, a list, a tuple, a call's arguments, an index, or an
  f-string's field, where such a name may start the next element instead, a `,`;
- a `)`, `]`, `}`, or end of an f-string or of a field, unless the skipped tokens opened it, so a
  call skipped whole, `f(function() ... end)` included, doesn't stop the skipping inside it;
- the end of the source.

Then the parser *unwinds*: its cursor shows the end of the source, so each node it had started
finishes as far as it got, and a node started while unwinding, which holds nothing, is left out, so
a part that a broken statement lacks is missing, not empty. The unwinding ends at a block's list of
statements, which goes on from the stop, or where the parser next asks for the token at the stop,
when that token ends a part of a construct, such as `then`, `do`, `end`, `,`, or `)`. So
`if x + then y() end` keeps its block, and `f(1 2)` puts `2` in an error node and keeps its `)`. A
block that unwinding reaches before any statement fills it, as in `if x` alone, is a part the `if`
lacks, and is left out. An expression that starts a statement but is neither a call nor assigned
to, and a `for` whose first name is followed by neither `=` nor `,` or `in`, leave their statement
without a kind, so it becomes an `Error` node.

The checks of a rule that a statement breaks while its tokens are all in place, such as `break`
outside a loop or an unknown attribute, report their error and skip nothing. A token that can
start no statement where one must start, such as `)` or `then`, is skipped, and so is a token that
ends a block where the main chunk must end, which gives `<eof> expected`.

Two rules keep one mistake from giving several errors, so each token gets at most one error:

- An error at the token where the last recovery stopped, before the parser has read another, isn't
  reported. So `while x do until` reports only `'end' expected near 'until'`, not also the
  `<eof> expected` of the main chunk at the same `until`.
- A token the parser doesn't expect isn't reported when it holds a lexical error, which the lexer
  has reported, and neither is the end of the source after an unfinished string or comment that
  reaches it, because what that swallowed can't be found missing. A token the lexer couldn't read,
  such as `0x` or a string with a bad escape, stands where it is as an operand, in an `Error` node,
  so `print("\q")` has one error, and an f-string's text with an error is text.

The recovery is a heuristic. When a missing token, such as the `(` of `local function f`, leaves a
block to be read, the statements that follow become that block, so if no `end` closes it, its
missing `end` is a second error. Recovery skips each token at most once, and looks at each token it
skips once more, to find the stop.

PUC-Rio's lexer reads one token at a time, as the parser asks for it, so a lexical error comes
before a syntax error when the parser reaches the token that holds it first. Lunaferra's lexer has
already read the whole source, so `source_errors` puts the two lists together in that order: a
syntax error comes where the parser was when it found it, a lexical error at the first token that
ends after the error starts, and a lexical error before a syntax error at the same token. The first
error it gives is the one PUC-Rio reports, so `x = 1 0x` gives `malformed number near '0x'`, while
`@ 0x` gives `unexpected symbol near '@'`.

From M1.8, a depth counter will stop nesting at PUC-Rio Lua 5.5.1's limit, with its message,
and M1.8 records both here. Until then, the parser calls itself once for each level of a nested
block or expression, with no limit.

## Lossless tree

The lossless tree is built from the events. Every byte of the source belongs to exactly one token,
and an error node marks each place the parser recovered. Typed views over the generic nodes give
the tools named accessors, such as `IfStat::condition()`. The formatter, the analysis crates, and
the language server use it.

Built in M1.3 as described here:

```rust
impl SyntaxTree {
    pub fn build(source: &[u8], lexed: &Lexed, events: &[Event]) -> Result<SyntaxTree, Error>;
    pub fn root(&self) -> SyntaxNode<'_>;
    pub fn source(&self) -> &[u8];
}

impl<'t> SyntaxNode<'t> {     // a handle that borrows its tree, cheap to copy
    pub fn kind(&self) -> NodeKind;
    pub fn children(&self) -> Children<'t>;   // an iterator of SyntaxElement, in source order
    pub fn text(&self) -> Vec<u8>;            // its tokens, each with its trivia
}

impl<'t> SyntaxToken<'t> {
    pub fn kind(&self) -> TokenKind;
    pub fn span(&self) -> Span;
    pub fn text(&self) -> &'t [u8];
    pub fn leading(&self) -> &'t [Trivia];
    pub fn trailing(&self) -> &'t [Trivia];
}

pub enum SyntaxElement<'t> { Node(SyntaxNode<'t>), Token(SyntaxToken<'t>) }

// The typed views, in the module `ast`:
pub trait AstNode<'t>: Sized {
    fn cast(node: SyntaxNode<'t>) -> Option<Self>;   // None for a node of another kind
    fn syntax(&self) -> SyntaxNode<'t>;
}
pub struct Chunk<'t>;       // block() and eof()
pub struct Block<'t>;       // statements()
pub struct ErrorNode<'t>;
pub enum Stat<'t> { /* one variant for each statement's view, such as If(IfStat) */ }
pub enum Expr<'t> { /* one variant for each expression's view, such as Call(CallExpr) */ }
```

M1.4 added a view for each node kind that it added, with `Stat` and `Expr` over the views of every
statement and of every expression, and M1.5 added the views of the other expressions and their
parts, with `TableField` over the three kinds of table field. [Statement nodes](#statement-nodes)
and [Expression nodes](#expression-nodes) list the kinds and what each holds.

`build` accepts a list of events only when it makes one tree over every token of the source: the
first event opens the root, every later one falls inside the root, each `Finish` closes the node
opened last that is still open, and the `Token` events name each token of the `Lexed` once, in
order, by kind and span, up to and including `Eof`. An `Error` event adds nothing to the tree,
whose error node marks the place, but it too must fall inside the root. Otherwise it returns
`Error::EventOutsideRoot` or `Error::TokenOutOfOrder`, with the position of the event at fault,
or `Error::EventsEndEarly`. So every tree that `build` returns holds every token of its source,
and gives back the source byte for byte. Because a `Token` event must match its token exactly,
the parser can't yet split one token in two; the parser of type annotations will need that to
read the `>>` that ends `list<list<T>>` as two `>` (M4.3).

A node's text is the bytes of the tokens inside it, each with its leading and trailing trivia, so
the root's text is the whole source. The tree keeps its own copy of the source and the trivia, so
it outlives the buffers it was built from. It is stored as one flat list in which each node comes
first, then everything inside it, and records where its descendants end. Building, walking,
printing, and dropping a tree therefore never recurse, so 100,000 nested parentheses can't
overflow the call stack, and `children` steps over a child node's descendants in one step.

A view's accessors look among its node's children for each part, and give `None` for a part that
isn't there, which is how the tools read a source with syntax errors. Every statement and every
expression has its node kind and its view, and the syntax that M4 adds will get theirs with it.

### Statement nodes

Each node holds its parts in source order, with every keyword and symbol as a token. `Name`,
`String`, and `Eof` are tokens; the rest are nodes.

```text
Chunk          ::= Block Eof
Block          ::= {statement}
EmptyStat      ::= ';'
AssignStat     ::= ExprList '=' ExprList
CallStat       ::= CallExpr | MethodCallExpr
LabelStat      ::= '::' Name '::'
BreakStat      ::= 'break'
GotoStat       ::= 'goto' Name
DoStat         ::= 'do' Block 'end'
WhileStat      ::= 'while' expression 'do' Block 'end'
RepeatStat     ::= 'repeat' Block 'until' expression
IfStat         ::= 'if' expression 'then' Block {ElseifClause} [ElseClause] 'end'
ElseifClause   ::= 'elseif' expression 'then' Block
ElseClause     ::= 'else' Block
NumericForStat ::= 'for' Name '=' expression ',' expression [',' expression] 'do' Block 'end'
GenericForStat ::= 'for' NameList 'in' ExprList 'do' Block 'end'
FunctionStat   ::= 'function' FuncName FuncBody
FuncName       ::= Name {'.' Name} [':' Name]
LocalFunctionStat  ::= 'local' 'function' Name FuncBody
GlobalFunctionStat ::= 'global' 'function' Name FuncBody
LocalStat      ::= 'local' AttNameList ['=' ExprList]
GlobalStat     ::= 'global' AttNameList ['=' ExprList] | 'global' [Attrib] '*'
NonlocalStat   ::= Name NameList            -- the first Name is `nonlocal`
ReturnStat     ::= 'return' [ExprList] [';']
AttNameList    ::= [Attrib] AttName {',' AttName}
AttName        ::= Name [Attrib]
Attrib         ::= '<' Name '>'
NameList       ::= Name {',' Name}
FuncBody       ::= ParamList Block 'end'
ParamList      ::= '(' [Param {',' Param} [',' VarargParam] | VarargParam] ')'
Param          ::= Name
VarargParam    ::= '...' [Name]
ExprList       ::= expression {',' expression}
NameExpr       ::= Name
ParenExpr      ::= '(' expression ')'
FieldExpr      ::= expression '.' Name
IndexExpr      ::= expression '[' expression ']'
CallExpr       ::= expression Args
MethodCallExpr ::= expression ':' Name Args
Args           ::= '(' [ExprList] ')' | String | TableExpr
LiteralExpr    ::= 'nil' | 'true' | 'false' | Integer | Float | String
VarargExpr     ::= '...'
```

The targets of an assignment are an `ExprList` of names, fields, and indexes, as Lua's `varlist`
is. `Param` and `AttName` are nodes, though each holds one name so far, because the type
annotations of M4.3 join them there.

### Expression nodes

The expressions that statements are built from are among the [statement nodes](#statement-nodes);
the rest are these. Built in M1.5 as described here:

```text
BinaryExpr      ::= expression binop expression
UnaryExpr       ::= unop expression
CompareExpr     ::= expression compop expression {compop expression}
ConditionalExpr ::= expression 'if' expression 'else' expression
FunctionExpr    ::= 'function' FuncBody
TableExpr       ::= '{' [field {fieldsep field} [fieldsep]] '}'
IndexField      ::= '[' expression ']' '=' expression
NamedField      ::= Name '=' expression
PositionalField ::= expression
ListExpr        ::= '[' [expression {',' expression} [',']] ']'
TupleExpr       ::= '(' ')' | '(' expression ',' [expression {',' expression} [',']] ')'
FStringExpr     ::= FStringStart {FStringText | FStringField} FStringEnd
FStringField    ::= FStringFieldStart expression ['='] [FStringConversion] [FormatSpec]
                    FStringFieldEnd
FormatSpec      ::= FStringSpec {FStringText | FStringField}

field    ::= IndexField | NamedField | PositionalField
fieldsep ::= ',' | ';'
binop    ::= 'or' | 'and' | '|' | '^' | '&' | '<<' | '>>' | '+' | '-' | '*' | '/' | '//' | '%'
           | '**'
unop     ::= 'not' | '-' | '+' | '~' | '#'
compop   ::= '<' | '<=' | '>' | '>=' | '==' | '!=' | 'in' | 'not' 'in' | 'is' | 'is' 'not'
```

Every comparison is a `CompareExpr`, even one with a single operator, so the compiler reads `a < b`
and `a < b < c` alike, and no `BinaryExpr` holds a comparison operator. Its view's `operators`
gives each operator as a `CompareOp`, with `not in` and `is not` one operator each. A positional
field is a node of its own, though it holds only its value, so that `TableExpr::fields` gives
every field in order as a `TableField`. `FStringExpr::parts` and `FormatSpec::parts` give the
text tokens and the fields in order as `FStringPart`s.

## Compact tree

The compact tree is built from the same events, holds only what the compiler needs, and is built
only when there are no errors. It keeps no keyword, symbol, space, comment, or empty statement,
and it holds names and literals as values: each numeral's [`Number`](#lexer), and each string's
bytes with its escapes read. It records the span of each part the compiler takes a line from,
for debug information, and from M4 it keeps type annotations as compact type expressions, so the
compiler can store JIT hints without the type checker.

Built in M1.7 as described here:

```rust
impl CompactTree {
    pub fn build(source: &[u8], lexed: &Lexed, events: &[Event]) -> Result<CompactTree, Error>;
    pub fn block(&self) -> List<StatId>;   // the chunk's statements
    pub fn eof(&self) -> Span;             // the end of the source
    pub fn lines(&self) -> &LineMap;
}
// `tree[id]` gives a part, and `tree[list]` a slice, for each of these:
impl Index<StatId> for CompactTree { type Output = Stat; }
impl Index<ExprId> for CompactTree { type Output = Expr; }
impl Index<FunctionId> for CompactTree { type Output = Function; }
impl Index<Bytes> for CompactTree { type Output = [u8]; }   // a name or a string's value
impl Index<List<ExprId>> for CompactTree { type Output = [ExprId]; }   // and each other list

// In the module `compact`:
pub struct Stat { pub kind: StatKind, pub span: Span }
pub struct Expr { pub kind: ExprKind, pub span: Span }
pub enum StatKind {
    Assign { targets: List<Target>, values: List<ExprId> }, Call(Call), Label(Name), Break,
    Goto(Name), Do(List<StatId>), While { .. }, Repeat { .. }, If { branches, else_body },
    NumericFor { .. }, GenericFor { .. }, Function { path, method, function },
    LocalFunction { .. }, GlobalFunction { .. }, Local { names, values }, Global { names, values },
    GlobalAll { attrib }, Nonlocal(List<Name>), Return(List<ExprId>),
}
pub enum ExprKind {
    Nil, True, False, Number(Number), String(Bytes), Vararg, Name(Bytes), Paren(ExprId),
    Field { .. }, Index { .. }, Call(Call), Binary { left, operator, operator_span, right },
    Unary { .. }, Compare { first, rest: List<Comparison> }, Conditional { .. },
    Function(FunctionId), Table(List<TableField>), List(List<ExprId>), Tuple(List<ExprId>),
    FString(List<FStringPart>),
}
```

The tree is stored as one flat list for each kind of part, so building, cloning, and dropping it
never recurse, and a chain of 100,000 additions, each the left operand of the next, can't overflow
the call stack. A node refers to another by an id, and to several by a `List`, a range of one of
the tree's lists. The compiler that reads it will need a stack of its own, not recursion, for
such a chain.

What the compact tree keeps, and what it leaves out, apart from the tokens of each node:

- **Spans** (the owner's choice, 2026-11-26, over one line for each node). Each statement,
  expression, assignment target, function body, and call's arguments keeps its span, from the
  start of its first token to the end of its last, and each binary operator and each comparison
  keeps its operator's span, both tokens of `not in` and `is not` included. The tree keeps the
  line map, so the compiler can take whichever line PUC-Rio Lua 5.5.1 gives: a call's from its
  `(`, an operator's from the operator, a generic `for`'s from the first value after `in`, a
  function statement's first line from its `function`, that of a function expression or a local
  or global function from its `(`, and a function's last line from its `end`.
- **Values** (the owner's choice, 2026-11-26, over spans the compiler decodes). A name is its
  bytes. A short string is the bytes its characters and escapes stand for, as `read_string`
  saves them; a long string is its text without the line break right after the opening bracket,
  with each other line break as `\n`, as `read_long_string` saves it. The parser's
  `near` text of a string is its value between its delimiters, so both come from one function. An
  f-string's text reads the same way for its form, with each `{{` and `}}` as one brace; a spec's
  text holds no brace. A field with `=` keeps its expression's source text, from the start of its
  first token to the end of its last, so the text of `{ x =}` is `x`, which the field writes
  before `=` and the value.
- **What only the syntax shows.** An empty statement and a `;` after `return` are left out. A
  call's arguments are a list of values, whether written as `(...)`, a string, or a table. Each
  declared name has its attribute: its own, or else the one before the first name. An assignment's
  targets are each a name, a field, or an index, a `Target` of its own, and a call statement holds
  a `Call`, so the compiler meets no other kind there. `(x)` stays a parenthesized expression,
  because it keeps only the first value of a call or `...`.

`build` refuses a source with a lexical error and events with an `Error` event,
`Error::SourceHasErrors`; events that make no tree, with the errors of `SyntaxTree::build`; and a
node that doesn't hold the parts of its kind, in order and with every keyword and symbol, as
[Statement nodes](#statement-nodes) and [Expression nodes](#expression-nodes) list them,
`Error::MalformedNode`, with the position of its `Start` event. An `Error` node is always
malformed, and so is a root that isn't a chunk. So `build` accepts the events of every source
`source_errors` finds no error in, and never panics on others.

The enums of the compact tree, `StatKind`, `ExprKind`, and the kinds of their parts, are
exhaustive, unlike `NodeKind`, `TokenKind`, and the views of `ast` (the owner's choice,
2026-11-26). So the compiler, a crate of its own, can match every kind with no `_` arm, which no
test could reach, and a kind that M4 adds will fail the compiler's build until it handles it. Adding one breaks the crate's API, which
matters only once the crates are released. Its structs are `#[non_exhaustive]`, so M4 can add
fields, such as a declared name's type.

## Syntax errors

For `load` and the compiler, a syntax error produces PUC-Rio's format,
`<chunkname>:<line>: <message> near '<token>'`, such as `input:3: 'end' expected near <eof>`. An
error in syntax that Lunaferra shares with Lua 5.5 has the same message and token text as on PUC-Rio
Lua 5.5.1; the messages for Lua's removed operators and for Lunaferra's own syntax are recorded in
`language.md`. For the tools, each error is a `P` diagnostic, with a code for its kind, its
message, its span, and, where certain, a fix, as [`diagnostics.md`](diagnostics.md) defines.

Built in M1.6 as described here:

```rust
pub enum SourceError<'a> { Lex(&'a LexError), Parse(&'a ParseError) }
impl SourceError<'_> {
    pub fn message(&self) -> Vec<u8>;       // PUC-Rio's message, as bytes
    pub fn line(&self) -> u32;              // the line PUC-Rio's message names
    pub fn span(&self) -> Span;
    pub fn diagnostic(&self) -> Diagnostic;
}

// Every error, lexical and syntactic, in the order PUC-Rio's parser meets them.
pub fn source_errors<'a>(lexed: &'a Lexed, events: &'a [Event]) -> Vec<SourceError<'a>>;
// The same errors as diagnostics, for the tools.
pub fn diagnostics(lexed: &Lexed, events: &[Event]) -> Vec<Diagnostic>;

impl LexError { pub fn diagnostic(&self) -> Diagnostic; }
impl ParseError { pub fn diagnostic(&self) -> Diagnostic; }
```

`Event::Error` keeps the `ParseError`, which the `Diagnostic` doesn't replace, because `load`
needs what only it has: the message as bytes, since the text after `near` is a copy of the source
and need not be valid UTF-8, and PUC-Rio's line, which for an attribute's error is the line of the
`>`, not of the span (the owner's choice, 2026-11-26). A diagnostic's message is the same text,
with each sequence of bytes that isn't valid UTF-8 replaced by U+FFFD, and its span is the error's.
The diagnostics of Lua's `~=` and binary `~` have a fix that writes `!=` or `^` in their place,
which parses wherever the old operator stopped the parse. The one for `..` has none, because `+`
binds more tightly than `..` did and joins only strings, so the edit could change what the
expression computes.

`ParseError::message` gives the message after the chunk name and the line, as `LexError::message`
does, and `ParseError::line` gives the line, both as PUC-Rio's `lparser.c` and `llex.c` give them:

- The line is the one where the token the parser stopped at ends, which is the line PUC-Rio's
  lexer is on once it has read that token: for a long string over three lines, the third.
- The checks of attributes and of `<close>` variables write no `near`, as PUC-Rio's
  `luaK_semerror` doesn't, and name the line where the last token read ends, such as the `>` of
  the attribute. The messages for Lua's `..`, `~=`, and binary `~` name the operator themselves, so
  they write no `near` either, and name the line where the operator ends.
- After `near` comes the token's text as PUC-Rio's lexer keeps it: a short string's quotes around
  the bytes its characters and escapes stand for, so `"a\65"` is written `'"aA"'`; a long string's
  brackets around its text, with each line break as `\n` and none right after the opening bracket;
  `<eof>` for the end of the source; a byte that isn't printable ASCII as `<\N>`, with `N` in
  decimal, such as `'<\200>'`; and any other token's own bytes. As in PUC-Rio, the text stops at
  its first zero byte, and an unexpected zero byte, which PUC-Rio reads as the token 0, gives
  `unexpected symbol` with no `near` at all.
- `X expected` names a reserved word or a symbol in quotes, such as `'end'`, and the other kinds as
  `luaX_token2str` does, such as `<name>` and `<eof>`. With `(to close Y at line N)`, it names the
  construct a missing `end`, `until`, or `)` closes, when that construct opened on an earlier
  line.

## Invariants

1. Printing the lossless tree's tokens and trivia gives back the source, byte for byte, for every
   input, valid or not.
2. The parser never panics, and finishes in time proportional to the input's length.
3. For valid input, the compact tree built from the events matches the lossless tree's structure.
4. For every syntax error in syntax Lunaferra shares with Lua 5.5, Lunaferra reports PUC-Rio Lua
   5.5.1's message and line.
5. Nesting one level past PUC-Rio Lua 5.5.1's limit stops with PUC-Rio's message.
6. Every expression groups as the precedence table says ([`language.md`](language.md#invariants),
   invariant 1).
7. The line map gives every byte offset the line that PUC-Rio Lua 5.5.1's lexer counts for it, for
   any bytes, valid UTF-8 or not, and a source longer than `MAX_SOURCE_LEN` bytes is rejected.
8. `numeral_value` gives every numeral the value PUC-Rio Lua 5.5.1 gives it, with each float
   rounded to the nearest float and a tie to even.
9. `SyntaxTree::build` accepts exactly the lists of events that make one tree over every token of
   the source, and walking the tree it builds gives back those events, apart from the `Error`
   events, which add nothing to the tree.
10. The parser's events make a tree for every input. Its `Error` events come in source order, at
    most one at each token, and none of the errors for a token it didn't expect is at a token that
    holds a lexical error.
11. The first error that `source_errors` gives is the one PUC-Rio Lua 5.5.1 reports for the same
    source, wherever the syntax is Lua's.

## Tests

- Invariants 1 and 2: the `lexer` and `parser` fuzz targets check the round trip and the absence of
  panics on generated and mutated input, and a hostile-input test checks time on inputs from 1 KB to
  16 MB, including 100,000 nested parentheses. Until those land (M1.8, M1.9),
  `crates/syntax/tests/lexer.rs` checks the lexer's half of invariant 1: a property test writes
  out the tokens and trivia of 5,000 generated sources, lexed both as a string and as a file, and
  compares them with the source, and other tests do the same for every conformance script and for
  100,000 nested f-strings. `crates/syntax/tests/tree.rs` checks the tree's half: a property test
  builds trees of random shape over the tokens of 2,000 generated sources and compares each root's
  text with its source, and other tests do the same for every conformance script and for a tree
  nested 100,000 levels deep. `crates/syntax/tests/parser.rs` checks the parser's half on 5,000
  generated sources of statement keywords, operators, names, symbols, and broken tokens, and on
  every conformance script and its twin, and parses a chain of 100,000 fields and one of 100,000
  calls. `crates/syntax/tests/expression.rs` parses a chain of 100,000 `+`, one of 100,000 `<`, and
  a list, a tuple, a table, and an f-string of 100,000 elements each.
- Invariant 3: `crates/syntax/tests/compact.rs` writes the shape of each tree, every statement and
  expression with its span and its parts, once from the compact tree and once from the lossless
  tree through its typed views, and checks that the two agree for 2,000 generated programs of
  every statement and expression, for every conformance script, and for each form of each
  statement and expression. Other tests there check each escape of a short string, the line breaks
  of a long one, numerals, f-string text and specs, a field's text before `=`, conversions,
  attributes, the spans a line comes from, every list of events that `build` refuses, and chains
  of 100,000 additions, 100,000 fields, and 100,000 nested parentheses, built and dropped without
  recursion.
- Invariant 4: `tests/conformance/syntax_errors.lua` loads broken chunks with `load` and prints each
  message, recorded from its Lua 5.5 twin on PUC-Rio Lua 5.5.1; it runs once the conformance runner
  does (M2.14), and invariant 11's test checks its messages until then. For the lexer's errors,
  `crates/syntax/tests/lexer.rs` compares the first error of each of 54 broken chunks with the
  message that `load` gave for it on PUC-Rio Lua 5.5.1 on 2026-11-25. For the parser's errors,
  `crates/syntax/tests/parser.rs` compares the first error of each of 188 broken chunks, with
  every kind of `ParseError` and each statement's nearby mistakes, with the message and line that
  `load` gave on PUC-Rio Lua 5.5.1 on 2026-11-25, from a build with `LUA_COMPAT_GLOBAL` set to 0 for
  the `global` statements, and checks that a lexical error comes first where PUC-Rio's does.
  `crates/syntax/tests/expression.rs` does the same for 48 broken chunks of Lua's operators,
  tables, function expressions, and an `if` after a value, with the message and line that `load`
  gave on PUC-Rio Lua 5.5.1 on 2026-11-26, and checks Lunaferra's own messages, which
  [`language.md`](language.md) records. A property test parses 2,000 generated programs of every
  statement and every expression, and checks that none has an error, and other tests check that
  each form PUC-Rio accepts parses, and to which nodes.
- Invariant 5: hostile-input tests at the limit and one level past it, for blocks, parentheses,
  tables, and function bodies.
- Invariant 6: a property test in `crates/syntax/tests/expression.rs` generates 3,000 expressions
  from the grammar of [`language.md`](language.md#precedence), with every operator, `not`, chained
  comparisons, conditional expressions, and parentheses, and writes beside each the grouping that
  the grammar gives it. A precedence-climbing function written in the test from the precedence
  table groups the same tokens again, and the test checks that the parser's tree groups them as
  both do. Other tests check the examples of the precedence section, and each level against the
  levels next to it.
- Invariant 7: `crates/syntax/tests/source.rs` checks the line and column of every offset of every
  source of up to seven bytes made from `a`, `\n`, `\r`, and 0xFF, a byte that never appears in
  valid UTF-8, against a model of `inclinenumber`. It also checks each line ending and a source of
  `u32::MAX` bytes, and unit tests in `crates/syntax/src/source.rs` check the length limit at
  `MAX_SOURCE_LEN` and one byte past it. `crates/syntax/tests/lexer.rs` checks the count after a
  file's skipped first line that holds a `\r`, as PUC-Rio gave it for such a file.
- Invariant 8: unit tests in `crates/syntax/src/lexer/numeral.rs` compare 19 hexadecimal floats
  near the limits and ties of rounding with the bits PUC-Rio Lua 5.5.1 gave them, read back 2,000
  floats written exactly in hexadecimal, and compare 2,000 more of 65 to 128 bits, half of them
  ties, with a model that rounds in a `u128`. `tests/conformance/lexer.lua` prints numerals of each
  form, recorded from its twin on PUC-Rio Lua 5.5.1, and runs once the conformance runner does
  (M2.14).
- Invariant 9: a property test in `crates/syntax/tests/tree.rs` gives `SyntaxTree::build` 3,000
  lists of events, many of them changed by one edit that may break the tree, such as an event
  removed or two swapped, and compares its answer with a second working-out of the rule. For each
  tree it builds, the test walks the tree and compares the events it gives with the list. Other
  tests in the file check each error on a list of its own, and that an `Error` event outside the
  root is refused.
- Invariant 10: the property test of the parser's events in `crates/syntax/tests/parser.rs`
  checks the tree of each of 5,000 generated sources of statement keywords, operators, names,
  symbols, and broken tokens, that their `Error` events come in source order at different spans,
  and that none for a token the parser didn't expect is at a token that holds a lexical error.
  `crates/syntax/tests/recovery.rs` checks where the parser goes on after each kind of stop,
  inside brackets and out, that it reports no error twice at one token or at a token that holds a
  lexical error, that a rule a statement breaks skips nothing, that a file with three separate
  errors gets three diagnostics and keeps the statements between them, and that 100,000 broken
  lines give 100,000 diagnostics.
- Invariant 11: `crates/syntax/tests/recovery.rs` takes each of the 43 chunks that
  `tests/conformance/syntax_errors.lua` loads, and compares its first error from `source_errors`
  with the line PUC-Rio Lua 5.5.1 printed for its twin, recorded in
  `tests/conformance/reference/syntax_errors.out`. `crates/syntax/tests/diagnostic.rs` checks that a
  lexical error comes before a syntax error at the same token, and the code, message, span, and
  fix of the diagnostic of each kind of error.
