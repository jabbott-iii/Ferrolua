-- The Lua 5.5 twin of ../syntax_errors.lua, the same script, since it uses no syntax that
-- Lunaferra changes: each chunk is plain Lua 5.5. `cargo xtask record` runs it on PUC-Rio
-- Lua 5.5.1 and saves what it prints to syntax_errors.out. Its header has as many lines as the
-- script's, so each statement stays on the same line in both, though the chunks' own line
-- numbers, which the messages name, don't depend on it.

local function check(chunk)
  local loaded, message = load(chunk, "=input")
  print(message)
end

-- Numbers, strings, and comments the lexer can't read.
check[[x = 0x]]
check[[x = "abc]]
check[[x = [[abc]]
check[[--[[ x]]
check[[x = [=x]]
check[[x = "\q"]]
check[[x = "\xZZ"]]
check[[x = "\u12"]]
check[[x = "\u{12"]]
check[[x = "\u{110000000}"]]
check[[x = "\300"]]

-- Tokens that no rule expects where they are.
check[[x = = 1]]
check[[@]]
check[[x = 1 + * 2]]
check[[f(]]
check[[x.1]]
check[[a.b:c = 1]]
check[[f(1, 2,)]]
check[[x = 1 end]]
check[[x = function() end end]]
check[[return return]]
check[[goto 1]]
check[[::a:: ::b]]
check[[for 1 do end]]
check[[for i do end]]
check[[function f(1) end]]
check[[local function f(..., a) end]]
check[[if x then elseif then end]]

-- Constructs left open, on the line where they open and on an earlier one.
check[[if x then]]
check[[if x then
y = 1]]
check[[while x do
if y then
end]]
check[[repeat x = 1]]
check[[x = {1, 2]]
check[[f(1
2)]]

-- Rules that a statement breaks while its tokens are all in place.
check[[x = 1 break]]
check[[function f() return ... end]]
check[[local x <foo> = 1]]
check[[local a <close>, b <close> = 1, 2]]

-- Several errors, of which PUC-Rio reports the first: a lexical error comes first when the
-- parser reaches the token that holds it before it finds a syntax error.
check[[x = = 1
y = = 2
z = = 3]]
check[[local t = {
  a = 1,
  b = = 2,
}]]
check[[f(function()
  x = = 1
end)
y = = 2]]
check[[x = 1 0x]]
check[[@ 0x]]
