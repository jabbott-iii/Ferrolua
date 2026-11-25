# Subsystem: `language`

Ferrolua's language is Lua 5.5 plus five extensions: compound assignment and `++`, optional type
annotations, classes, and a strict mode with arity enforcement and undeclared-variable errors. It
is a superset (ADR-0004: every valid Lua 5.5 script runs unchanged, and strict checks are opt-in
per chunk), so every program PUC-Rio Lua 5.5.1 accepts means the same on Ferrolua. This spec
defines the extensions; the [Lua 5.5 Reference Manual](https://www.lua.org/manual/5.5/) defines
everything else. The parser lands in M1, and the extensions in M4. ADR-0005 (operators), ADR-0006
(types), ADR-0007 (classes, proposed), and ADR-0008 (strict mode, proposed) shaped it.

## Why a superset

Lua programs are often small scripts that a host loads, so most users won't rewrite them. A
superset lets those scripts run unchanged and lets a project adopt each extension when it wants
one. The cost is that every extension must fit into gaps in Lua's grammar. Each new token is a
sequence of characters that can't appear outside a string or comment in valid Lua 5.5, and each
new keyword is contextual: it is a keyword only where a name couldn't appear in valid Lua.

## The Lua 5.5 baseline

Ferrolua implements all of Lua 5.5, including what is new since 5.4:

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

`global` is a reserved word, so a Lua 5.4 script that names a variable `global` doesn't parse, as on
PUC-Rio Lua 5.5.1.

## New tokens and contextual keywords

| Token | Meaning |
| :- | :- |
| `++` | Increment statement |
| `+=` `-=` `*=` `/=` `//=` `%=` `^=` `..=` `&=` `\|=` `<<=` `>>=` | Compound assignment |
| `?` | Optional parameter or type |
| `->` | Return type in a function type |

- A comment wins over every new token: `--` always starts a comment, so `x -=- 1` is `x` followed
  by `-=`, then `-` and `1`, while `x--=1` is `x` followed by a comment.
- `..=` is read before `...` only when the third character is `=`, so `...==` is still `...`
  then `==`.
- `class`, `extends`, `type`, and `super` stay ordinary names. `class` is a keyword only at the
  start of a statement, or right after `local` or `global`, and followed by a name; `type` only at
  the start of a statement and followed by a name; and `extends` only after the name in a class
  declaration. `local class Point` declares a class, while `local class = 1` still declares a local
  named `class`. `super` is a keyword only inside a class method, followed by `.` or `:`. A function
  named `class` or `type` keeps working: `class "Point"` is still a call, and so is `type(x)`.

## Compound assignment and `++` (ADR-0005)

```text
stat ::= var compoundop exp
       | var '++'
compoundop ::= '+=' | '-=' | '*=' | '/=' | '//=' | '%=' | '^=' | '..=' | '&=' | '|=' | '<<=' | '>>='
```

- `v op= e` means `v = v op (e)`, except that the parts of `v` are evaluated once. For
  `t[f()] += g()`, Ferrolua evaluates `t`, then calls `f` once, reads `t[k]`, calls `g`, adds,
  and stores into `t[k]`.
- The operator and the store use Lua's usual rules, metamethods included: `t.n += 1` may call
  `__index`, `__add`, and `__newindex`, in that order.
- `v++` means `v += 1`.
- Each is a statement with one target. `a, b += 1` is a syntax error, and neither form is an
  expression, so `x = y++` is a syntax error.
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
  return math.sqrt((a.x - b.x) ^ 2 + (a.y - b.y) ^ 2)
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
    return math.sqrt(self.x ^ 2 + self.y ^ 2)
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
- Open questions for M4.4: what `type`, `pairs`, `rawget`, `getmetatable`, and `setmetatable` do with
  an instance; whether classes have static functions; and how code checks that a value is an
  instance of a class.

## Strict mode (ADR-0008, proposed)

Strict mode is two checks, each off unless turned on:

- A chunk turns both on with `--!strict` as its first line, or its second line when the first
  starts with `#!`.
- A host turns either one on, or both, in `ChunkOptions` ([`ferrolua.md`](ferrolua.md)).

### Undeclared-variable errors

A strict chunk compiles as if it started with a `global` declaration that lists the names the
state's libraries and its host define. Every other free name must be declared with `global` before
it is used, under Lua 5.5's rules, and an undeclared name is a compile-time error. PUC-Rio Lua 5.5.1's
message for an undeclared name is recorded in M4.6, and Ferrolua uses the same wording.

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

1. **Superset:** every program PUC-Rio Lua 5.5.1 accepts produces the same tokens on Ferrolua,
   comments aside, and prints the same output.
2. **Erasure:** deleting every type annotation from a program changes neither its output nor its
   bytecode, apart from the JIT hints.
3. **Single evaluation:** in a compound assignment, the target's prefix, its key, and the
   right-hand side are each evaluated exactly once, in that order.
4. **Strict mode only adds errors:** a program that runs to the end in strict mode prints the same
   output with strict mode off.
5. **Classes make no loops:** creating and dropping any number of instances returns the heap to its
   size before, with no collection.

## Tests

- Invariant 1: a property test in `ferrolua-syntax` generates Lua 5.5 programs with
  `ferrolua-testkit` and checks that the token stream is the one the Lua 5.5 grammar alone gives
  (M4.1). The conformance scripts check the output part.
- Invariant 2: a property test in `ferrolua-compiler` compiles generated typed programs with and
  without their annotations and compares the bytecode (M4.3).
- Invariant 3: `tests/extensions/compound_assignment.lua` counts calls through metamethods and
  functions with side effects (M4.2).
- Invariant 4: the strict-mode tests run every script in `tests/extensions/strict/` in both modes
  and compare the output of those that finish (M4.6, M4.7).
- Invariant 5: a test in `ferrolua-vm` records the heap's size, creates and drops 100,000 instances
  of a class with a parent, and checks the size again (M4.5).
- Each grammar rule has a parser test for the accepted form and for each nearby error, with the
  exact message.
