# Subsystem: `language`

Ferrolua's language derives from Lua 5.5. It keeps Lua's statements, values, tables, metatables,
closures, coroutines, and standard library, and changes three things: its operators follow
Python's, its indexes start at 0, and its ranges stop before their end (ADR-0023: a language of its
own, with Python's operators and 0-based indexes). It adds four features: compound assignment,
optional type annotations, classes, and a strict mode with arity enforcement and undeclared-variable
errors. This spec defines what differs from Lua 5.5; the
[Lua 5.5 Reference Manual](https://www.lua.org/manual/5.5/) defines everything else. The parser
lands in M1, and the four features in M4. ADR-0006 (types), ADR-0007 (classes, proposed), ADR-0008
(strict mode, proposed), and ADR-0024 (the details Python's operators and 0-based indexes leave
open) shaped it.

## Why a language of its own

The owner chose Python's operators and indexes that start at 0, for simplicity. No superset of
Lua 5.5 can have them: Lua already gives `^`, `..`, `~=`, and `~` meanings of their own, and a Lua
script that reads `t[1]` expects a list's first element. A mode that each chunk turns on would keep
old scripts running, but every table passed between the two modes would need its integer keys
shifted. So Ferrolua gives up running Lua 5.5 scripts unchanged.

It keeps everything else from Lua 5.5, so porting a script changes its expressions and its index
arithmetic, not its structure. That also keeps PUC-Rio Lua 5.5.1 as the reference for the shared
part: each conformance script has a Lua 5.5 twin that PUC-Rio runs to record the expected output
([`04-quality.md`](../04-quality.md#conformance-scripts)). The cost is that existing Lua code and
Lua tools, such as lua-language-server and StyLua, don't work on Ferrolua files without changes.

## The Lua 5.5 baseline

Ferrolua implements all of Lua 5.5 that this spec doesn't change, including what is new since 5.4:

- `global` declarations, in every form: `global x`, `global x <const> = 1`, `global function f`,
  `global *`, and `global <const> *`, with the scoping rules in §2.2 and §3.3.7 of the manual;
- read-only control variables in numeric and generic `for` loops;
- named varargs, such as `function f(...args)`, where `args` is a read-only local holding the
  vararg table;
- a limit of 15 objects in a chain of `__call` metamethods;
- an error object of `nil` replaced by a string message;
- floats printed with enough digits to read back the same value;
- `table.create`, the `lax` argument to `utf8.codes`, `utf8.codepoint`, and `utf8.len`, and the end
  position that `utf8.offset` also returns.

Truth stays Lua's (ADR-0024): only `nil` and `false` are false, and `0`, `""`, and an
empty table are true, unlike in Python.

## Tokens and keywords

| Token | Meaning |
| :- | :- |
| `**` | Power |
| `!=` | Not equal |
| `^` | Bitwise XOR, which is power in Lua |
| `+=` `-=` `*=` `/=` `//=` `%=` `**=` `&=` `\|=` `^=` `<<=` `>>=` | Compound assignment |
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
- `class`, `extends`, `type`, and `super` stay ordinary names. `class` is a keyword only at the
  start of a statement, or right after `local` or `global`, and followed by a name; `type` only at
  the start of a statement and followed by a name; and `extends` only after the name in a class
  declaration. `local class Point` declares a class, while `local class = 1` still declares a local
  named `class`. `super` is a keyword only inside a class method, followed by `.` or `:`. A function
  named `class` or `type` keeps working: `class "Point"` is still a call, and so is `type(x)`.

## Operators (ADR-0023)

| Operation | Lua 5.5 | Ferrolua |
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
  `"total: " .. a + b` adds first; Ferrolua writes `"total: " + tostring(a + b)`.

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
simpleexp  ::= 'nil' | 'false' | 'true' | Numeral | LiteralString | '...' | functiondef
             | prefixexp | tableconstructor
```

These rules replace the `exp` rule of §9 of the manual. `prefixexp`, `functiondef`, and
`tableconstructor` keep their Lua 5.5 definitions.

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
  pair raises an error. It tests keys, as Python's `in` does for a dictionary, so
  `3 in {1, 2, 3}` is false: that table's keys are 0, 1, and 2. `k not in v` is `not (k in v)`.
- **`a is b`** is `rawequal(a, b)`: true for the same table, function, thread, or userdata, and for
  equal strings, numbers, booleans, or `nil`, without calling `__eq`. `a is not b` is
  `not (a is b)`.
- **`x if c else y`** evaluates `c`, then exactly one of `x` and `y`: `x` when `c` is neither `nil`
  nor `false`, and `y` otherwise.
- **Unary `+`** returns a number unchanged and raises an error for any other value. It has no
  metamethod (ADR-0024).
- **`#`** gives a string's length in bytes, calls `__len`, or gives a table's border, as
  [Indexes and ranges](#indexes-and-ranges-adr-0023) defines it.
- **`__concat`** is no longer a metamethod, because `..` is gone. `__contains` is new.

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
  integers, Ferrolua computes the number of iterations before the loop starts, as Lua 5.5 does, so
  the control variable never overflows.
- Library ranges stop before their end too. `string.sub(s, i, j)` returns the bytes at positions
  i to j − 1, as Python's `s[i:j]` does, and [`stdlib.md`](stdlib.md#positions-and-ranges) lists
  each function's positions and defaults.
- Some numbers aren't positions and still count from 1 (ADR-0024): line and column
  numbers, argument numbers in messages such as `bad argument #1`, stack levels, pattern capture
  numbers, and the fields of `os.date`. `arg[0]` is the script, and `arg[1]` its first argument.

```lua
local primes = {2, 3, 5, 7}
print(primes[0])               --> 2
print(#primes)                 --> 4
for i = 0, #primes do
  print(i, primes[i])          --> 0 2, then 1 3, 2 5, and 3 7
end
print(("Ferrolua"):sub(0, 5))  --> Ferro
print(("Ferrolua"):sub(-3))    --> lua
```

## Compound assignment (ADR-0023)

```text
stat ::= var compoundop exp
compoundop ::= '+=' | '-=' | '*=' | '/=' | '//=' | '%=' | '**=' | '&=' | '|=' | '^=' | '<<=' | '>>='
```

- `v op= e` means `v = v op (e)`, except that the parts of `v` are evaluated once. For
  `t[f()] += g()`, Ferrolua evaluates `t`, then calls `f` once, reads `t[k]`, calls `g`, adds,
  and stores into `t[k]`.
- The operator and the store use the usual rules, metamethods included: `t.n += 1` may call
  `__index`, `__add`, and `__newindex`, in that order. `s += "!"` joins strings, and `x ^= 1` is
  XOR.
- Each is a statement with one target. `a, b += 1` is a syntax error, and none is an expression,
  so `x = (y += 1)` is a syntax error. Python has no `++`, and neither does Ferrolua.
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
  `string`, `function`, `table`, `thread`, and `userdata`.
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
- Calling the class creates an instance. Ferrolua evaluates each field's default in declaration
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
- A host turns either one on, or both, in `ChunkOptions` ([`ferrolua.md`](ferrolua.md)).

### Undeclared-variable errors

A strict chunk compiles as if it started with a `global` declaration that lists the names the
state's libraries and its host define. Every other free name must be declared with `global` before
it is used, under Lua 5.5's rules, and an undeclared name is a compile-time error. PUC-Rio Lua
5.5.1's message for an undeclared name is recorded in M4.6, and Ferrolua uses the same wording.

```lua
--!strict
print("hello")      -- print is defined by the base library
count = 1           -- compile error: count is not declared
global total = 0    -- declares total
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
   middle operand is evaluated exactly once; and a conditional expression evaluates its condition
   once and exactly one branch.
4. **Half-open ranges:** for every string `s` and integers `i` and `j`, `string.sub(s, i, j)`
   returns the bytes Python's `s[i:j]` returns, and for integers `a`, `b`, and a nonzero `c`,
   `for v = a, b, c do` runs once for each value of Python's `range(a, b, c)`, in the same order.
5. **Strict mode only adds errors:** a program that runs to the end in strict mode prints the same
   output with strict mode off.
6. **Classes make no loops:** creating and dropping any number of instances returns the heap to its
   size before, with no collection.

## Tests

- Invariant 1: a property test in `ferrolua-syntax` generates expressions with `ferrolua-testkit`,
  prints each parse fully parenthesized, and compares it with the grouping that a separate
  precedence-climbing function in the test gives (M1.5).
- Invariant 2: a property test in `ferrolua-compiler` compiles generated typed programs with and
  without their annotations and compares the bytecode (M4.3).
- Invariant 3: `tests/extensions/compound_assignment.lua` (M4.2) and
  `tests/extensions/operators.lua` (M2.9) count calls through metamethods and functions with side
  effects.
- Invariant 4: a property test in `ferrolua-stdlib` compares `string.sub` with a slice function
  written in the test from Python's rules (M3.3), and one in `ferrolua` runs generated numeric
  `for` loops and compares their iterations with a `range` function written the same way (M2.14).
- Invariant 5: the strict-mode tests run every script in `tests/extensions/strict/` in both modes
  and compare the output of those that finish (M4.6, M4.7).
- Invariant 6: a test in `ferrolua-vm` records the heap's size, creates and drops 100,000 instances
  of a class with a parent, and checks the size again (M4.5).
- Each grammar rule has a parser test for the accepted form and for each nearby error, with the
  exact message.
