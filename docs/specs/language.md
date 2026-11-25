# Subsystem: `language`

Lunaferra's language derives from Lua 5.5. It keeps Lua's statements, values, tables, metatables,
closures, coroutines, and standard library, and takes these from Python instead: its operators, its
indexes that start at 0, and its ranges that stop before their end (ADR-0023: a language of its
own, with Python's operators and 0-based indexes); and f-strings, locals by default, lists, tuples,
and sets, and Python's rule for which values are false (ADR-0025: four more of Python's features).
It also adds compound assignment, optional type annotations, classes, and a strict mode with arity
enforcement and undeclared-variable errors. This spec defines what differs from Lua 5.5; the
[Lua 5.5 Reference Manual](https://www.lua.org/manual/5.5/) defines everything else. The parser
lands in M1, and compound assignment, types, classes, and strict mode in M4. ADR-0006 (types),
ADR-0007 (classes, proposed), ADR-0008 (strict mode, proposed), ADR-0024 (the details Python's
operators and 0-based indexes leave open), and ADR-0026 (the details of ADR-0025's features)
shaped it.

## Why a language of its own

The owner chose Python's operators and indexes that start at 0, for simplicity. No superset of
Lua 5.5 can have them: Lua already gives `^`, `..`, `~=`, and `~` meanings of their own, and a Lua
script that reads `t[1]` expects a list's first element. A mode that each chunk turns on would keep
old scripts running, but every table passed between the two modes would need its integer keys
shifted. So Lunaferra gives up running Lua 5.5 scripts unchanged.

It keeps everything else from Lua 5.5, so porting a script changes its expressions and its index
arithmetic, not its structure. That also keeps PUC-Rio Lua 5.5.1 as the reference for the shared
part: each conformance script has a Lua 5.5 twin that PUC-Rio runs to record the expected output
([`04-quality.md`](../04-quality.md#conformance-scripts)). The cost is that existing Lua code and
Lua tools, such as lua-language-server and StyLua, don't work on Lunaferra files without changes.

## The Lua 5.5 baseline

Lunaferra implements all of Lua 5.5 that this spec doesn't change, including what is new since 5.4:

- `global` declarations, in every form: `global x`, `global x <const> = 1`, `global function f`,
  `global *`, and `global <const> *`, with the scoping rules that [Scoping](#scoping-adr-0025)
  gives in place of §2.2 and §3.3.7 of the manual;
- read-only control variables in numeric and generic `for` loops;
- named varargs, such as `function f(...args)`, where `args` is a read-only local holding the
  vararg table;
- a limit of 15 objects in a chain of `__call` metamethods;
- an error object of `nil` replaced by a string message;
- floats printed with enough digits to read back the same value;
- `table.create`, the `lax` argument to `utf8.codes`, `utf8.codepoint`, and `utf8.len`, and the end
  position that `utf8.offset` also returns.

## Tokens and keywords

| Token | Meaning |
| :- | :- |
| `**` | Power |
| `!=` | Not equal |
| `^` | Bitwise XOR, which is power in Lua |
| `+=` `-=` `*=` `/=` `//=` `%=` `**=` `&=` `\|=` `^=` `<<=` `>>=` | Compound assignment |
| `f"..."` `f'...'` `f[[...]]` | Formatted string literal (f-string) |
| `[` ... `]` | List literal, where an expression starts |
| `?` | Optional parameter or type |
| `->` | Return type in a function type |

- Lua's `..`, `~=`, and binary `~` aren't operators. The parser reports each with a message that
  names its replacement, `+`, `!=`, or `^`, which M1.5 records here. `...` stays the vararg token,
  and unary `~` stays bitwise NOT.
- The reserved words are Lua 5.5's 23 and `is` (ADR-0024). `in`, `not`, `if`, and `else`
  are already reserved in Lua.
- A comment wins over every new token: `--` always starts a comment, so `x -=- 1` is `x` followed
  by `-=`, then `-` and `1`, while `x--=1` is `x` followed by a comment.
- The lexer takes the longest token it can: `**=` before `**`, and `**` before `*`.
- `nonlocal` stays an ordinary name too, a keyword only at the start of a statement and followed
  by a name (ADR-0026).
- `class`, `extends`, `type`, and `super` stay ordinary names. `class` is a keyword only at the
  start of a statement, or right after `local` or `global`, and followed by a name; `type` only at
  the start of a statement and followed by a name; and `extends` only after the name in a class
  declaration. `local class Point` declares a class, while `local class = 1` still declares a local
  named `class`. `super` is a keyword only inside a class method, followed by `.` or `:`. A function
  named `class` or `type` keeps working: `class "Point"` is still a call, and so is `type(x)`.

## Operators (ADR-0023)

| Operation | Lua 5.5 | Lunaferra |
| :- | :- | :- |
| Power | `a ^ b` | `a ** b` |
| Not equal | `a ~= b` | `a != b` |
| Bitwise XOR | `a ~ b` | `a ^ b` |
| Join two strings | `a .. b` | `a + b` |
| Membership | none | `k in t`, `k not in t` |
| Identity | `rawequal(a, b)` | `a is b`, `a is not b` |
| Conditional | `c and x or y`, which gives `y` when `x` is `false` or `nil` | `x if c else y` |
| Unary plus | none | `+a` |

`-`, `*`, `/`, `//`, `%`, `&`, `|`, `<<`, `>>`, unary `-`, `~`, and `#`, `==`, `<`, `<=`, `>`, `>=`,
`and`, `or`, and `not` keep their spellings. Their meanings follow Python where the two languages
differ, as the list below says.

### Precedence

From the loosest binding to the tightest, as in Python:

| Level | Operators | Grouping |
| :- | :- | :- |
| 1 | `x if c else y` | Right to left |
| 2 | `or` | Left to right |
| 3 | `and` | Left to right |
| 4 | unary `not` | |
| 5 | `in`, `not in`, `is`, `is not`, `<`, `<=`, `>`, `>=`, `!=`, `==` | Chained |
| 6 | `\|` | Left to right |
| 7 | `^` | Left to right |
| 8 | `&` | Left to right |
| 9 | `<<`, `>>` | Left to right |
| 10 | `+`, `-` | Left to right |
| 11 | `*`, `/`, `//`, `%` | Left to right |
| 12 | unary `-`, `+`, `~`, `#` | |
| 13 | `**` | Right to left |

- `**` binds more tightly than a unary operator on its left, and less tightly than one on its
  right: `-x ** 2` is `-(x ** 2)`, and `2 ** -1` is 0.5. Lua's `^` groups the same way.
- `not` binds more loosely than a comparison: `not a == b` is `not (a == b)`, where Lua reads
  `(not a) == b`.
- Joining strings has the precedence of `+`, where Lua's `..` bound more loosely than `+`. Lua's
  `"total: " .. a + b` adds first; Lunaferra writes `"total: " + tostring(a + b)`.

```text
exp        ::= orexp ['if' orexp 'else' exp]
orexp      ::= andexp {'or' andexp}
andexp     ::= notexp {'and' notexp}
notexp     ::= 'not' notexp | comparison
comparison ::= bitor {compop bitor}
compop     ::= '<' | '<=' | '>' | '>=' | '==' | '!=' | 'in' | 'not' 'in' | 'is' | 'is' 'not'
bitor      ::= bitxor {'|' bitxor}
bitxor     ::= bitand {'^' bitand}
bitand     ::= shift {'&' shift}
shift      ::= sum {('<<' | '>>') sum}
sum        ::= term {('+' | '-') term}
term       ::= unary {('*' | '/' | '//' | '%') unary}
unary      ::= ('-' | '+' | '~' | '#') unary | power
power      ::= simpleexp ['**' unary]
simpleexp  ::= 'nil' | 'false' | 'true' | Numeral | LiteralString | FString | '...' | functiondef
             | prefixexp | tableconstructor | listcons | tuplecons
listcons   ::= '[' [exp {',' exp} [',']] ']'
tuplecons  ::= '(' ')' | '(' exp ',' [exp {',' exp} [',']] ')'
```

These rules replace the `exp` rule of §9 of the manual. `prefixexp`, `functiondef`, and
`tableconstructor` keep their Lua 5.5 definitions, so `(x)` stays a parenthesized expression, and
only a comma makes a tuple. [Formatted string literals](#formatted-string-literals-adr-0025)
defines `FString`.

### Meanings

- **`+`** adds two numbers and joins two strings into a new string. For any other pair it calls
  `__add` from the first operand that has one, so `"x" + t` works when `t` has `__add`. A string
  and a number raise an error: nothing converts between them, so `"10" + 1` is an error, and code
  writes `tonumber("10") + 1` or `"n = " + tostring(n)`.
- **`-`, `*`, `/`, `//`, and `%`** take numbers, or call their metamethods. A string operand raises
  an error. `/` always gives a float; `//` and `%` round toward minus infinity, as in Lua and
  Python.
- **`**`** gives an integer when both operands are integers and the exponent is not negative,
  wrapping around on overflow as `*` does: `2 ** 10` is 1024. Any other pair gives a float, so
  `2 ** -1` is 0.5 and `2.0 ** 2` is 4.0. Its metamethod is `__pow`.
- **Division by zero** raises an error, with `/`, `//`, or `%`, for integers and floats alike:
  `1 / 0` and `1.0 % 0.0` both do. Other float operations keep the results of IEEE 754, the
  floating-point standard, so `math.huge - math.huge` is `nan`.
- **`&`, `|`, `^`, `~`, `<<`, and `>>`** take integers, or call `__band`, `__bor`, `__bxor`,
  `__bnot`, `__shl`, and `__shr`. A float raises an error, even one with an integer value such as
  `3.0` (ADR-0024). `>>` keeps the sign, so `-8 >> 1` is -4. A shift count of 64 or more
  gives 0, or -1 for `>>` of a negative number, and a negative count raises an error.
- **`==`, `<`, `<=`, `>`, and `>=`** work as in Lua 5.5, with `__eq`, `__lt`, and `__le`, and
  always give a boolean. `a != b` is `not (a == b)`. Comparing a number with a string raises an
  error with `<`, `<=`, `>`, and `>=`, and gives `false` with `==`, as in both Lua and Python.
- **Chained comparisons:** `a op1 b op2 c` means `a op1 b and b op2 c`, except that `b` is
  evaluated once. Any number of comparisons chain the same way, and the chain stops at the first
  that is false.
- **`k in v`** calls `v`'s `__contains` metamethod when it has one (ADR-0024), and
  converts the result to a boolean. Otherwise, for a table `v`, it is true when `rawget(v, k)` isn't
  `nil`; for strings `k` and `v`, when `k` occurs in `v` as plain text, not as a pattern. Any other
  pair raises an error. On a table it tests keys, as Python's `in` does for a dictionary, so
  `3 in {1, 2, 3}` is false: that table's keys are 0, 1, and 2. On a list or a tuple it tests
  values, so `3 in [1, 2, 3]` is true, and on a set its members. `k not in v` is `not (k in v)`.
- **`a is b`** is `rawequal(a, b)`: true for the same table, function, thread, or userdata, and for
  equal strings, numbers, booleans, or `nil`, without calling `__eq`. `a is not b` is
  `not (a is b)`.
- **`x if c else y`** evaluates `c`, then exactly one of `x` and `y`: `x` when `c` is true, as
  [Truth](#truth-adr-0025) defines it, and `y` otherwise.
- **Unary `+`** returns a number unchanged and raises an error for any other value. It has no
  metamethod (ADR-0024).
- **`#`** gives a string's length in bytes or a collection's length, calls `__len`, or gives a
  table's border, as [Indexes and ranges](#indexes-and-ranges-adr-0023) defines it.
- **Lists, tuples, and sets** take `==`, the orderings, `+`, `*`, and the set operators, as
  [Lists, tuples, and sets](#lists-tuples-and-sets-adr-0025) says.
- **`__concat`** is no longer a metamethod, because `..` is gone. `__contains`, `__bool`, and
  `__format` are new.

M2.9 records each operator's edge cases and error messages in [`vm.md`](vm.md#the-interpreter).

### Conditional expressions

An `if` statement can follow a complete expression, because Lua needs no separator between
statements: `x = f if c then g() end` is an assignment, then an `if` statement. A conditional
expression puts its `if` in the same place. When `if` follows an expression that could end a
statement, the parser reads the condition as an `orexp` and then looks at the next token. `else`
makes a conditional expression. Any other token means the `if` starts an `if` statement, and the
parser goes on reading that statement's condition from what it has read. The condition is parsed
the same way on both paths, so the parser never reads a token twice.

```lua
local label = "even" if n % 2 == 0 else "odd"
local x = f if c then g() end     -- an assignment, then an if statement
```

## Indexes and ranges (ADR-0023)

- A table constructor's positional fields take the keys 0, 1, 2, and so on: `{10, 20, 30}` puts 10
  at `t[0]` and 30 at `t[2]`.
- `#t` returns a border of the table: an n where `t[n - 1]` isn't `nil` and `t[n]` is, or 0 when
  `t[0]` is `nil`. For a sequence, a table whose non-negative integer keys are exactly 0 to n − 1,
  `#t` is n, so `t[#t] = v` appends `v`.
- `ipairs(t)` gives 0 and `t[0]`, then 1 and `t[1]`, and stops at the first `nil`.
- `select(i, ...)` returns the arguments from position i on, counting from 0, and `select(-1, ...)`
  the last. `select("#", ...)` still counts them. `table.pack(...)` and a named vararg table store
  the arguments at 0 to n − 1, with their count in the field `n`.
- A string position counts bytes from 0. A negative position counts back from the end, with -1 the
  last byte, as in Lua and Python.
- A range covers its start and stops before its end, which makes it a half-open range, as
  Python's `range` and slices are. The numeric `for` follows it: `for v = start, stop, step do`
  runs for `start`, `start + step`, and so on, while `v` is below `stop` for a positive step or
  above it for a negative one. The step defaults to 1, and a step of 0 raises an error. For
  integers, Lunaferra computes the number of iterations before the loop starts, as Lua 5.5 does, so
  the control variable never overflows.
- Library ranges stop before their end too. `string.sub(s, i, j)` returns the bytes at positions
  i to j − 1, as Python's `s[i:j]` does, and [`stdlib.md`](stdlib.md#positions-and-ranges) lists
  each function's positions and defaults.
- Some numbers aren't positions and still count from 1 (ADR-0024): line and column
  numbers, argument numbers in messages such as `bad argument #1`, stack levels, pattern capture
  numbers, and the fields of `os.date`. `arg[0]` is the script, and `arg[1]` its first argument.

```lua
local primes = [2, 3, 5, 7]
print(primes[0])               --> 2
print(#primes)                 --> 4
for i = 0, #primes do
  print(i, primes[i])          --> 0 2, then 1 3, 2 5, and 3 7
end
print(("Lunaferra"):sub(0, 5))  --> Ferro
print(("Lunaferra"):sub(-3))    --> lua
```

## Formatted string literals (ADR-0025)

An f-string is a string literal with an `f` touching its opening quote or long bracket: `f"..."`,
`f'...'`, or `f[[...]]`. Each field in braces holds an expression. Lunaferra evaluates the fields
from left to right, formats each one, and joins the parts into one string.

```text
FString ::= 'f' LiteralString     -- the f touches the quote or the long bracket
field   ::= '{' exp ['='] ['!' ('s' | 'r')] [':' spec] '}'
spec    ::= {a character other than '{' or '}' | field}
```

```lua
local name, n, price = "Ada", 3, 2.5
print(f"{name} has {n} items")      --> Ada has 3 items
print(f"total: {n * price:.2f}")   --> total: 7.50
print(f"[{n:>4}]")                  --> [   3]
print(f"{n=}, {name!r}")            --> n=3, "Ada"
print(f"{name:upper()}")            --> ADA, a method call, not a spec
print(f"{{n}} is {n}")              --> {n} is 3
```

- `{{` and `}}` stand for `{` and `}`. The text between fields keeps the escapes of its string
  form, so a long-bracket f-string has none.
- A field's expression is any expression, and may hold strings and braces of its own. At its top
  level, `!s` or `!r` before `:` or `}` is a conversion, and `:` starts the format spec unless a
  name and `(` follow it, which make a method call (ADR-0026).
- `=` after the expression writes the expression's text, then `=`, then the value.
- Without a spec, a field writes `tostring` of its value. `!r` writes a string quoted, as
  `string.format("%q")` writes it, and any other value as `tostring` does. `=` uses `!r` unless the
  field gives a conversion or a spec.
- After `:` comes Python's format specification mini-language,
  `[[fill]align][sign][z][#][0][width][grouping][.precision][type]`, for numbers and strings. A
  value whose metatable has `__format` is formatted by `__format(value, spec)` (ADR-0026). A spec
  may hold fields of its own, as in `{x:>{width}}`.
  [`stdlib.md`](stdlib.md#formatted-strings) gives the details.
- Formatting a field is a conversion the code asks for, so f-strings keep ADR-0023's rule that
  nothing converts between strings and numbers on its own.
- `f"x"` is no longer a call of a function named `f`; that call is written `f("x")` or `f "x"`.

## Scoping (ADR-0025)

Every function, the main chunk included, owns the names its body assigns. A name resolves, from
the innermost scope out, to:

1. a parameter, a `for` loop's variable, or a `local` whose block is still open;
2. a local of the current function, which it has because the function assigns the name;
3. the same, in each enclosing function in turn, which a closure captures as an upvalue;
4. a global, read and written through `_ENV`, as §2.2 of the manual describes.

- An assignment, plain or compound, or a `function name` statement makes the name a local of the
  whole function it is in, unless something on the list above, or a `global` or `nonlocal`
  declaration, already names it. The local exists from the function's start, even when the
  assignment comes later or inside a block, so `if c then x = 1 else x = 2 end print(x)` prints
  the local.
- Reading such a local before the function assigns it raises an error,
  `local 'x' read before assignment` (ADR-0026). Assigning `nil` counts as assigning it.
- `global x` declares that `x` names a global, so assignments to it write the global. Lua 5.5's
  forms `global x <const> = 1`, `global function f`, `global *`, and `global <const> *` stay.
- `nonlocal x` declares that `x` names the variable `x` of the nearest enclosing function that has
  one, so assignments write that variable instead of making a local.
- A `global` or `nonlocal` declaration covers the whole function it is in and the functions nested
  in it, and comes before the function's first use of the name. `global *` makes every name that
  the function assigns, and nothing else declares, a global, which restores Lua's rule for code
  being ported (ADR-0026).
- `local` keeps Lua's meaning: a local that lasts to the end of its block, with `<const>` or
  `<close>` if wanted. Each pass of a loop gets a new `local`, while a function-wide local is one
  variable for the whole call, so closures made in a loop share it, as in Python.
- Reading a name that no scope declares reads a global, as in Lua and Python. Lua 5.5's implicit
  `global *` at the start of every chunk is gone (ADR-0026), and strict mode's
  [undeclared-variable errors](#undeclared-variable-errors) are the way to forbid such reads.

```lua
count = 0                   -- a local of the chunk
function bump()             -- bump is a local of the chunk too
  nonlocal count            -- without it, count += 1 would read bump's own unassigned count
  count += 1
end
global function report()    -- a global, which other chunks can call
  print(f"count = {count}")
end
```

## Lists, tuples, and sets (ADR-0025)

| Type | Built by | Changes | Holds | As a key |
| :- | :- | :- | :- | :- |
| `list` | `[1, 2]`, `[]` | Yes | Any values, `nil` included, in order | Not allowed |
| `tuple` | `(1, 2)`, `(1,)`, `()` | No | Any values, `nil` included, in order | By value, when its elements can be keys |
| `set` | `set([1, 2])`, `set()` | Yes | Distinct values that can be keys, in no order | Not allowed |

Tables stay, as dictionaries, records, modules, and metatables. Most of what follows comes from
ADR-0026.

- `#` gives a collection's exact length. A list or a tuple is indexed by an integer from `-#c` to
  `#c - 1`, a negative index counting from the end; any other index raises an error, as Python's
  `IndexError` does. `xs[i] = v` replaces an element, and `xs:append(v)` adds one. A tuple and a
  set can't be assigned into.
- `in` tests a list's or a tuple's values with `==`, and a set's members.
- `==` compares lists, tuples, and sets by value. `<`, `<=`, `>`, and `>=` compare lists and tuples
  element by element, and sets as subsets, as in Python. `is` still tests identity.
- `+` joins two lists or two tuples into a new one, and `*` with an integer repeats one. `|`, `&`,
  `-`, and `^` give the union, intersection, difference, and symmetric difference of two sets.
- A tuple whose elements can all be keys hashes by value, so `t[(1, 2)]` finds any equal tuple. A
  list or a set used as a table key or a set element raises an error. Tables, functions, threads,
  and userdata still hash by handle (ADR-0020).
- Each type has one metatable, shared the way strings share theirs, whose `__index` is the type's
  library, so `xs:append(v)` calls `list.append`. `setmetatable` on a collection raises an error.
  [`stdlib.md`](stdlib.md#lists-tuples-and-sets) lists the functions.
- A generic `for` over a list, tuple, or set visits its values: `for v in xs do`. `ipairs(xs)` gives
  positions too. A set visits its members in an order the hash seed fixes, as a table's keys are.
- `list(c)`, `tuple(c)`, and `set(c)` build a collection from another one or from a table's
  sequence, and an empty one with no argument.
- The last expression in a list or tuple literal expands to all its results, as in a table
  constructor, so `[...]` collects the varargs.
- Multiple results stay Lua's: `return a, b` returns two values, not a tuple, and `a, b = t`
  assigns the tuple to `a`. `t:unpack()` gives a tuple's elements as results.
- `type` returns `"list"`, `"tuple"`, or `"set"`.
- `[[` always starts a long string, so a list whose first element is a list is written with a
  space: `[ [1, 2], [3] ]`. A `(` or `[` at the start of a line continues the expression before it,
  as `(` does in Lua, so `f()` followed by a line that starts with `[1]` indexes `f()`'s result.

```lua
local xs = [3, 1, 2]
xs:append(nil)
print(#xs, xs[-2])                  --> 4  2
local seen = set([(0, 0), (1, 2)])
print((1, 2) in seen, [1] == [1])   --> true  true
for v in (10, 20) do print(v) end   --> 10, then 20
```

## Truth (ADR-0025)

Python's rule decides which values are false; ADR-0024 had kept Lua's, where only `nil` and
`false` are. These values are false:

- `nil` and `false`;
- `0`, `0.0`, and `-0.0`;
- `""`;
- an empty list, tuple, or set;
- a table with no entries and no metatable, as an empty Python dictionary is (ADR-0026);
- a table or userdata whose metatable's `__bool` returns `false` (ADR-0026).

Every other value is true, including `nan`, every function and thread, a class instance, and a
table that has a metatable but no `__bool`, which acts as an object. `__bool` must return a
boolean, or the test raises an error.

- `if`, `while`, `repeat ... until`, `not`, `and`, `or`, a conditional expression, and `assert`
  test truth this way, and so do the results of `__eq`, `__lt`, `__le`, and `__contains`, and of
  `table.sort`'s comparator.
- `and` and `or` still return one of their operands: `0 or 5` is 5, and `"" and f()` is `""`. So
  `x = x or default` now replaces a 0 or an empty string too.
- Library functions that take `nil` or `false` to mean "no value", such as `string.gsub` with a
  replacement function and `load` with a reader, keep testing for exactly those two (ADR-0026).

## Compound assignment (ADR-0023)

```text
stat ::= var compoundop exp
compoundop ::= '+=' | '-=' | '*=' | '/=' | '//=' | '%=' | '**=' | '&=' | '|=' | '^=' | '<<=' | '>>='
```

- `v op= e` means `v = v op (e)`, except that the parts of `v` are evaluated once. For
  `t[f()] += g()`, Lunaferra evaluates `t`, then calls `f` once, reads `t[k]`, calls `g`, adds,
  and stores into `t[k]`.
- The operator and the store use the usual rules, metamethods included: `t.n += 1` may call
  `__index`, `__add`, and `__newindex`, in that order. `s += "!"` joins strings, and `x ^= 1` is
  XOR.
- Each is a statement with one target. `a, b += 1` is a syntax error, and none is an expression,
  so `x = (y += 1)` is a syntax error. Python has no `++`, and neither does Lunaferra.
- Assigning to a `<const>` variable or a `for` loop's control variable is the same compile error
  plain assignment gives.

## Type annotations (ADR-0006)

Annotations go after a name and its attribute, and after a parameter list for the return type. The
compiler drops them, apart from the hints it records for the JIT. [`types.md`](types.md) defines
what the type checker does with them.

```text
attnamelist ::= Name [attrib] [':' type] {',' Name [attrib] [':' type]}
param       ::= Name ['?'] [':' type]
varargparam ::= '...' [Name] [':' type]
funcbody    ::= [generics] '(' [parlist] ')' [':' rettype] block 'end'
stat        ::= 'type' Name [generics] '=' type

generics    ::= '<' Name {',' Name} '>'
rettype     ::= type | '(' [type {',' type}] ')'
type        ::= optional {'|' optional}
optional    ::= primary ['?']
primary     ::= 'nil' | 'true' | 'false' | String
              | Name {'.' Name} [typeargs]
              | '(' [type {',' type}] ')' '->' rettype
              | '{' type '}'
              | '{' '[' type ']' ':' type '}'
              | '{' Name ':' type {fieldsep Name ':' type} [fieldsep] '}'
              | '(' type ')'
typeargs    ::= '<' type {',' type} '>'
```

```lua
type Point = { x: number, y: number }

local origin <const>: Point = { x = 0, y = 0 }

function distance(a: Point, b: Point?): number
  b = b or origin
  return math.sqrt((a.x - b.x) ** 2 + (a.y - b.y) ** 2)
end
```

- `local x <const>: number = 1` puts the attribute before the type, because `<` after a type would
  read as the start of type arguments.
- The built-in type names are `any`, `unknown`, `never`, `nil`, `boolean`, `number`, `integer`,
  `string`, `function`, `table`, `thread`, `userdata`, `list`, `tuple`, and `set`. `list<T>`,
  `set<T>`, and `tuple<A, B>` give the element types, and `{T}` stays the type of a table used as a
  sequence.
- `type` declares an alias with the scope of a local. An alias is visible to the type checker
  only, and the compiler emits nothing for it.

## Classes (ADR-0007, proposed)

This section stays a proposal until the owner accepts ADR-0007, and M4.4 settles its open
questions first.

```text
stat      ::= ['local' | 'global'] 'class' Name ['extends' Name {'.' Name}] {member} 'end'
member    ::= Name [':' type] ['=' exp]
            | 'function' Name funcbody
```

```lua
class Point
  x: number = 0
  y: number = 0

  function new(self, x: number, y: number)
    self.x = x
    self.y = y
  end

  function length(self): number
    return math.sqrt(self.x ** 2 + self.y ** 2)
  end
end

class Point3 extends Point
  z: number = 0

  function new(self, x: number, y: number, z: number)
    super.new(self, x, y)
    self.z = z
  end
end

local p = Point3(3, 4, 12)
print(p:length())  --> 5.0, because length is Point's method and reads only x and y
```

- `class Name` assigns the class to `Name` as `function Name` would. `local class Name` declares a
  local, and `global class Name` a global.
- Calling the class creates an instance. Lunaferra evaluates each field's default in declaration
  order, parent fields first, then calls `new` with the instance and the call's arguments, and
  returns the instance. A default is evaluated for each instance, so `items: {string} = {}` gives
  every instance its own table.
- An instance has exactly the fields its class and its ancestors declare, in a fixed layout.
  Reading or writing any other field raises an error.
- Methods are found on the class, then on each ancestor in turn. `super.m(self, ...)` calls the
  parent's method `m`.
- A method named after a metamethod, such as `__tostring`, `__eq`, or `__add`, works as that
  metamethod for instances.
- Without a `__tostring` method, an instance prints as its class name and its ID, such as
  `Point: #7` (ADR-0019: objects print as IDs, never addresses).
- An instance refers to its class, and a class to its parent, each with a counted reference.
  Nothing refers back, so classes and instances make no reference loops, and an instance is
  freed as soon as it is dropped, without the loop collector (ADR-0018).
- Open questions for M4.4: what `type`, `pairs`, `rawget`, `in`, `getmetatable`, and `setmetatable`
  do with an instance; whether classes have static functions; and how code checks that a value is
  an instance of a class.

## Strict mode (ADR-0008, proposed)

Strict mode is two checks, each off unless turned on:

- A chunk turns both on with `--!strict` as its first line, or its second line when the first
  starts with `#!`.
- A host turns either one on, or both, in `ChunkOptions` ([`lunaferra.md`](lunaferra.md)).

### Undeclared-variable errors

A strict chunk compiles as if it started with a `global` declaration that lists the names the
state's libraries and its host define. An assignment always declares its name, as a local or
through `global` ([Scoping](#scoping-adr-0025)), so the check is on reads: reading a name that is
neither a local, declared with `global`, nor one of those names is a compile-time error. PUC-Rio Lua
5.5.1's message for an undeclared name is recorded in M4.6, and Lunaferra uses the same wording.

```lua
--!strict
print("hello")      -- print is defined by the base library
count = 1           -- fine: a local of the chunk
print(cuont)        -- compile error: cuont is not declared
global total = 0    -- declares the global total
total += 1          -- fine
```

### Arity enforcement

A function compiled in strict mode checks, when it is called, that it got as many arguments as it
has parameters. The check runs in the called function, so it holds for every caller: strict code,
non-strict code, metamethods, and the host.

- Parameters marked `?` are optional and must come after the required ones.
- A function with `...` accepts any number of extra arguments.
- A method defined with `:` counts its implicit `self`.
- Iterators called by a generic `for` get two arguments, the state and the control value, so a
  strict iterator declares two parameters, marking unused ones with `?`.
- A wrong count raises `bad argument count to 'f' (expected 2, got 3)`, or
  `(expected 2 to 3, got 1)` when some parameters are optional.

```lua
--!strict
local function area(w: number, h: number?)
  return w * (h or w)
end
area(2)        --> 4
area(2, 3)     --> 6
area(2, 3, 4)  -- error: bad argument count to 'area' (expected 1 to 2, got 3)
```

## Invariants

1. **Precedence:** every expression groups as the precedence table says.
2. **Erasure:** deleting every type annotation from a program changes neither its output nor its
   bytecode, apart from the JIT hints.
3. **Single evaluation:** in a compound assignment, the target's prefix, its key, and the
   right-hand side are each evaluated exactly once, in that order; in a chained comparison, each
   middle operand is evaluated exactly once; a conditional expression evaluates its condition
   once and exactly one branch; and an f-string evaluates each field once, from left to right.
4. **Half-open ranges:** for every string `s` and integers `i` and `j`, `string.sub(s, i, j)`
   returns the bytes Python's `s[i:j]` returns, and for integers `a`, `b`, and a nonzero `c`,
   `for v = a, b, c do` runs once for each value of Python's `range(a, b, c)`, in the same order.
5. **Strict mode only adds errors:** a program that runs to the end in strict mode prints the same
   output with strict mode off.
6. **Classes make no loops:** creating and dropping any number of instances returns the heap to its
   size before, with no collection.
7. **f-strings:** an f-string gives the string that joining its text with `format(v, spec)` of each
   field's value gives.
8. **No accidental globals:** an assignment writes a global only where a `global` declaration
   covers it.
9. **Truth:** a value is false exactly when [Truth](#truth-adr-0025) lists it, in every place that
   tests truth.
10. **Equal tuples hash alike:** two tuples that compare equal are one key in a table and one member
    in a set.

## Tests

- Invariant 1: a property test in `lunaferra-syntax` generates expressions with `lunaferra-testkit`,
  prints each parse fully parenthesized, and compares it with the grouping that a separate
  precedence-climbing function in the test gives (M1.5).
- Invariant 2: a property test in `lunaferra-compiler` compiles generated typed programs with and
  without their annotations and compares the bytecode (M4.3).
- Invariant 3: `tests/extensions/compound_assignment.lua` (M4.2) and
  `tests/extensions/operators.lua` (M2.9) count calls through metamethods and functions with side
  effects.
- Invariant 4: a property test in `lunaferra-stdlib` compares `string.sub` with a slice function
  written in the test from Python's rules (M3.3), and one in `lunaferra` runs generated numeric
  `for` loops and compares their iterations with a `range` function written the same way (M2.14).
- Invariant 5: the strict-mode tests run every script in `tests/extensions/strict/` in both modes
  and compare the output of those that finish (M4.6, M4.7).
- Invariant 6: a test in `lunaferra-vm` records the heap's size, creates and drops 100,000 instances
  of a class with a parent, and checks the size again (M4.5).
- Invariant 7: a property test in `lunaferra` generates f-strings and compares each with the same
  fields formatted by `format` and joined (M3.4).
- Invariant 8: a property test in `lunaferra-compiler` generates functions that assign names and
  checks that only names under a `global` declaration compile to global writes (M2.5).
- Invariant 9: `tests/extensions/truth.lua` tests every kind of value in each place that tests
  truth (M2.9).
- Invariant 10: a property test in `lunaferra-vm` builds equal tuples separately and checks that a
  table and a set each hold one entry for them (M2.18).
- Each grammar rule has a parser test for the accepted form and for each nearby error, with the
  exact message.
