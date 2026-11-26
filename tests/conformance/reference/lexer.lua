-- The Lua 5.5 twin of ../lexer.lua, the same script, since it uses no syntax that Lunaferra
-- changes. `cargo xtask record` runs it on PUC-Rio Lua 5.5.1 and saves what it prints to
-- lexer.out. Its header has as many lines as the script's, so each statement stays on the same
-- line in both.

-- Integers, in decimal and in hexadecimal, which wraps around past 64 bits.
print(0, 007, 1234567890, 9223372036854775807)
print(0xff, 0XFF, 0xA, 0x7fffffffffffffff, 0xffffffffffffffff, 0x10000000000000001)

-- A decimal integer too large for an integer is a float.
print(9223372036854775808, 9223372036854775808 == 9.2233720368547758e18)

-- Floats, in decimal and in hexadecimal, each rounded to the nearest float.
print(1.5, .5, 5., 3e2, 3E-2, 1e+3, .5e1, 1e309, 1e-400)
print(0x1p4, 0x.8, 0xA.8p0, 0x1P-2, 0x1.fffffffffffff8p1023, 0x1p-1074 == 4.9406564584124654e-324)
print(0x1.00000000000008p0 == 1.0, 0x1.00000000000018p0 == 1.0000000000000004)

-- Short strings and their escapes.
print("a\tb", 'it\'s', "q\"q", "back\\slash", '\'', "\"")
print("\a" == "\7", "\b" == "\8", "\f" == "\12", "\n" == "\10", "\r" == "\13", "\t" == "\9",
  "\v" == "\11")
print("\65\066\0677", "\x41\x62\x4A", "\u{48}\u{49}", "\u{E9}" == "\195\169")
print(#"\u{7F}", #"\u{80}", #"\u{800}", #"\u{10000}", #"\u{200000}", #"\u{4000000}",
  #"\u{7FFFFFFF}", "\u{7FFFFFFF}" == "\253\191\191\191\191\191")
print(#"\0\00\000", "\0" == "\x00", "a\z
       b", "line one\
line two")

-- Long strings, whose first line break isn't part of their text.
print([[a]], [==[b]]c]=]==], #[[
x]], #[[
]], [[]] == "")
print([=[
two
lines]=])

--[[ A long comment ]] print("after a long comment") --[==[ and one
over two lines ]==]
print("after a short comment") -- a short comment
