//! A generator of valid programs of every statement and expression, which the parser's tests and
//! the compact tree's tests share. A module in a folder of its own, so Cargo doesn't build it as a
//! test of its own.

use lunaferra_testkit::Source;

/// Chooses a number below `bound`, as a `usize`.
pub(crate) fn below(source: &mut Source, bound: usize) -> usize {
    let bound = u64::try_from(bound).expect("bounds are small");
    usize::try_from(source.below(bound)).expect("choices are below their bound")
}

/// The names that generated programs use. `nonlocal` is a name everywhere but at the start of a
/// statement before a name.
const NAMES: [&str; 4] = ["a", "b", "nonlocal", "c"];

/// How deep a generated program nests, and what its innermost function allows.
#[derive(Clone, Copy)]
struct Scope {
    depth: u32,
    in_loop: bool,
    vararg: bool,
}

/// Generates a valid program of Lua 5.5's statements and of every expression.
///
/// Each loop goes on, and each optional part is added, only for a choice of 1, so that a choice
/// of 0, which the runner gives once a shrunk case's choices run out, ends the program. An index
/// opens with `[ `, because `[[` always starts a long string.
pub(crate) fn generate_program(source: &mut Source) -> String {
    let mut out = String::new();
    let scope = Scope {
        depth: 3,
        in_loop: false,
        vararg: true,
    };
    generate_block(source, scope, &mut out);
    out
}

fn generate_block(source: &mut Source, scope: Scope, out: &mut String) {
    while source.below(3) != 0 {
        generate_stat(source, scope, out);
        out.push_str(if source.below(4) == 1 { ";\n" } else { "\n" });
    }
    if source.below(4) == 1 {
        out.push_str("return");
        if source.below(2) == 1 {
            out.push(' ');
            generate_exprs(source, scope, out);
        }
        out.push('\n');
    }
}

fn name(source: &mut Source) -> &'static str {
    NAMES[below(source, NAMES.len())]
}

/// Generates a statement. One that starts with `(` gets a `;` before it, because Lua reads a `(`
/// after an expression as a call, so `x = c\n(d).e = 1` would be `x = c(d).e` and a stray `=`.
fn generate_stat(source: &mut Source, scope: Scope, out: &mut String) {
    let mut stat = String::new();
    write_stat(source, scope, &mut stat);
    if stat.starts_with('(') {
        out.push(';');
    }
    out.push_str(&stat);
}

fn write_stat(source: &mut Source, scope: Scope, out: &mut String) {
    let inner = Scope {
        depth: scope.depth.saturating_sub(1),
        ..scope
    };
    let looped = Scope {
        in_loop: true,
        ..inner
    };
    let choices = if scope.depth == 0 { 6 } else { 17 };
    match below(source, choices) {
        0 => {
            generate_target(source, scope, out);
            while source.below(3) == 1 {
                out.push_str(", ");
                generate_target(source, scope, out);
            }
            out.push_str(" = ");
            generate_exprs(source, scope, out);
        }
        1 => generate_call(source, scope, out),
        2 => {
            out.push_str("::");
            out.push_str(name(source));
            out.push_str("::");
        }
        3 => {
            out.push_str("goto ");
            out.push_str(name(source));
        }
        4 => {
            out.push_str(if scope.in_loop { "break" } else { ";" });
        }
        5 => {
            out.push_str("local ");
            generate_attnames(source, out);
            if source.below(2) == 1 {
                out.push_str(" = ");
                generate_exprs(source, scope, out);
            }
        }
        6 => {
            out.push_str("do\n");
            generate_block(source, inner, out);
            out.push_str("end");
        }
        7 => {
            out.push_str("while ");
            generate_expr(source, scope, out);
            out.push_str(" do\n");
            generate_block(source, looped, out);
            out.push_str("end");
        }
        8 => {
            out.push_str("repeat\n");
            generate_block(source, looped, out);
            out.push_str("until ");
            generate_expr(source, scope, out);
        }
        9 => {
            out.push_str("if ");
            generate_expr(source, scope, out);
            out.push_str(" then\n");
            generate_block(source, inner, out);
            while source.below(3) == 1 {
                out.push_str("elseif ");
                generate_expr(source, scope, out);
                out.push_str(" then\n");
                generate_block(source, inner, out);
            }
            if source.below(2) == 1 {
                out.push_str("else\n");
                generate_block(source, inner, out);
            }
            out.push_str("end");
        }
        10 => {
            out.push_str("for ");
            out.push_str(name(source));
            out.push_str(" = ");
            generate_expr(source, scope, out);
            out.push_str(", ");
            generate_expr(source, scope, out);
            if source.below(2) == 1 {
                out.push_str(", ");
                generate_expr(source, scope, out);
            }
            out.push_str(" do\n");
            generate_block(source, looped, out);
            out.push_str("end");
        }
        11 => {
            out.push_str("for ");
            out.push_str(name(source));
            while source.below(2) == 1 {
                out.push_str(", ");
                out.push_str(name(source));
            }
            out.push_str(" in ");
            generate_exprs(source, scope, out);
            out.push_str(" do\n");
            generate_block(source, looped, out);
            out.push_str("end");
        }
        12 => {
            out.push_str("function ");
            out.push_str(name(source));
            while source.below(3) == 1 {
                out.push('.');
                out.push_str(name(source));
            }
            if source.below(3) == 1 {
                out.push(':');
                out.push_str(name(source));
            }
            generate_body(source, inner, out);
        }
        13 => {
            out.push_str(if source.below(2) == 1 {
                "local function "
            } else {
                "global function "
            });
            out.push_str(name(source));
            generate_body(source, inner, out);
        }
        14 => {
            out.push_str("global ");
            if source.below(3) == 1 {
                out.push_str(if source.below(2) == 1 {
                    "*"
                } else {
                    "<const> *"
                });
            } else {
                generate_attnames(source, out);
                if source.below(2) == 1 {
                    out.push_str(" = ");
                    generate_exprs(source, scope, out);
                }
            }
        }
        15 => {
            out.push_str("nonlocal ");
            out.push_str(name(source));
            while source.below(2) == 1 {
                out.push_str(", ");
                out.push_str(name(source));
            }
        }
        _ => out.push(';'),
    }
}

fn generate_attnames(source: &mut Source, out: &mut String) {
    if source.below(3) == 1 {
        out.push_str("<const> ");
    }
    out.push_str(name(source));
    if source.below(3) == 1 {
        out.push_str(" <const>");
    }
    while source.below(2) == 1 {
        out.push_str(", ");
        out.push_str(name(source));
    }
}

fn generate_body(source: &mut Source, scope: Scope, out: &mut String) {
    out.push('(');
    let mut vararg = false;
    if source.below(2) == 1 {
        out.push_str(name(source));
        while source.below(2) == 1 {
            out.push_str(", ");
            out.push_str(name(source));
        }
        if source.below(3) == 1 {
            out.push_str(", ...");
            vararg = true;
        }
    } else if source.below(2) == 1 {
        out.push_str("...");
        if source.below(2) == 1 {
            out.push_str(name(source));
        }
        vararg = true;
    }
    out.push_str(")\n");
    let body = Scope {
        in_loop: false,
        vararg,
        ..scope
    };
    generate_block(source, body, out);
    out.push_str("end");
}

fn generate_exprs(source: &mut Source, scope: Scope, out: &mut String) {
    generate_expr(source, scope, out);
    while source.below(3) == 1 {
        out.push_str(", ");
        generate_expr(source, scope, out);
    }
}

/// The binary operators, comparisons included, with a space on each side, because `- -` must
/// not become a comment.
const BINARY: [&str; 25] = [
    " or ", " and ", " < ", " <= ", " > ", " >= ", " == ", " != ", " in ", " not in ", " is ",
    " is not ", " | ", " ^ ", " & ", " << ", " >> ", " + ", " - ", " * ", " / ", " // ", " % ",
    " ** ", " <\n ",
];

/// Generates an expression: a simple one, or, while the depth lasts, an operator and its
/// operands, a conditional expression, or a constructor. Each operand is simple or in
/// parentheses, so that no `not` or conditional expression lands where the grammar forbids one;
/// the precedence property test in `tests/expression.rs` checks how operators group.
fn generate_expr(source: &mut Source, scope: Scope, out: &mut String) {
    let inner = Scope {
        depth: scope.depth.saturating_sub(1),
        ..scope
    };
    if scope.depth == 0 {
        generate_simple(source, scope, out);
        return;
    }
    match below(source, 14) {
        0..=6 => generate_simple(source, scope, out),
        7 => {
            generate_operand(source, inner, out);
            out.push_str(BINARY[below(source, BINARY.len())]);
            generate_operand(source, inner, out);
        }
        8 => {
            out.push_str(["-", "+", "~", "#", "not"][below(source, 5)]);
            out.push(' ');
            generate_operand(source, inner, out);
        }
        9 => {
            generate_operand(source, inner, out);
            out.push_str(" if ");
            generate_operand(source, inner, out);
            out.push_str(" else ");
            generate_operand(source, inner, out);
        }
        10 => generate_table(source, inner, out),
        11 => {
            out.push_str(if source.below(2) == 1 { "[ " } else { "(" });
            let close = if out.ends_with('(') { ")" } else { "]" };
            if source.below(3) != 0 {
                generate_exprs(source, inner, out);
                if close == ")" || source.below(2) == 1 {
                    out.push(',');
                }
            }
            out.push_str(close);
        }
        12 => generate_fstring(source, inner, out),
        _ => {
            out.push_str("function");
            generate_body(source, inner, out);
        }
    }
}

/// Generates an operand: a simple expression, or any expression in parentheses.
fn generate_operand(source: &mut Source, scope: Scope, out: &mut String) {
    if source.below(3) == 1 {
        out.push('(');
        generate_expr(source, scope, out);
        out.push(')');
    } else {
        generate_simple(source, scope, out);
    }
}

/// Generates a literal, `...`, or a prefix expression.
fn generate_simple(source: &mut Source, scope: Scope, out: &mut String) {
    match below(source, 8) {
        0 => out.push_str("nil"),
        1 => out.push_str(if source.below(2) == 1 {
            "true"
        } else {
            "false"
        }),
        2 => out.push_str(if source.below(2) == 1 { "12" } else { "0x1p4" }),
        3 => out.push_str(if source.below(2) == 1 {
            "'s'"
        } else {
            "[==[\nlong]==]"
        }),
        4 if scope.vararg => out.push_str("..."),
        _ => generate_prefix(source, scope, out),
    }
}

/// Generates a table constructor, with fields of each kind.
fn generate_table(source: &mut Source, scope: Scope, out: &mut String) {
    out.push('{');
    while source.below(3) != 0 {
        match below(source, 4) {
            0 => generate_expr(source, scope, out),
            1 => {
                out.push_str(name(source));
                out.push_str(" = ");
                generate_expr(source, scope, out);
            }
            2 => {
                out.push_str("[ ");
                generate_expr(source, scope, out);
                out.push_str("] = ");
                generate_expr(source, scope, out);
            }
            _ => {
                // A list, which a field that starts with `[` and has no `=` after its `]` is.
                out.push_str("[ ");
                generate_expr(source, scope, out);
                out.push_str("] + [ ]");
            }
        }
        out.push_str(if source.below(2) == 1 { ", " } else { "; " });
    }
    out.push('}');
}

/// Generates an f-string, with text, fields, conversions, and specs. Each field's expression
/// is in parentheses, so a `:` or `!` inside it is never the field's own.
fn generate_fstring(source: &mut Source, scope: Scope, out: &mut String) {
    out.push_str("f\"a");
    while source.below(3) != 0 {
        out.push_str("{(");
        generate_expr(source, scope, out);
        out.push(')');
        if source.below(3) == 1 {
            out.push('=');
        }
        if source.below(3) == 1 {
            out.push_str("!r");
        }
        if source.below(3) == 1 {
            out.push_str(":>{(");
            generate_expr(source, scope, out);
            out.push_str(")}.2");
        }
        out.push_str("}{{b}}");
    }
    out.push('"');
}

/// Generates a name or a parenthesized expression, then fields, indexes, and calls.
fn generate_prefix(source: &mut Source, scope: Scope, out: &mut String) {
    let inner = Scope {
        depth: scope.depth.saturating_sub(1),
        ..scope
    };
    if scope.depth > 0 && source.below(4) == 1 {
        out.push('(');
        generate_expr(source, inner, out);
        out.push(')');
    } else {
        out.push_str(name(source));
    }
    while source.below(3) == 1 {
        generate_suffix(source, inner, out);
    }
}

fn generate_suffix(source: &mut Source, scope: Scope, out: &mut String) {
    match below(source, 4) {
        0 => {
            out.push('.');
            out.push_str(name(source));
        }
        1 if scope.depth > 0 => {
            out.push_str("[ ");
            generate_expr(source, scope, out);
            out.push(']');
        }
        2 => {
            out.push(':');
            out.push_str(name(source));
            generate_args(source, scope, out);
        }
        _ => generate_args(source, scope, out),
    }
}

fn generate_args(source: &mut Source, scope: Scope, out: &mut String) {
    if source.below(3) == 1 {
        out.push_str(" 'arg'");
    } else {
        out.push('(');
        if scope.depth > 0 && source.below(2) == 1 {
            generate_exprs(source, scope, out);
        }
        out.push(')');
    }
}

/// Generates an assignment target: a name, or an expression ending in a field or an index.
fn generate_target(source: &mut Source, scope: Scope, out: &mut String) {
    if source.below(2) == 1 {
        out.push_str(name(source));
        return;
    }
    generate_prefix(source, scope, out);
    if source.below(2) == 1 {
        out.push('.');
        out.push_str(name(source));
    } else {
        out.push_str("[ ");
        generate_expr(source, scope, out);
        out.push(']');
    }
}

/// Generates a call: an expression ending in arguments.
fn generate_call(source: &mut Source, scope: Scope, out: &mut String) {
    generate_prefix(source, scope, out);
    if source.below(2) == 1 {
        out.push(':');
        out.push_str(name(source));
    }
    generate_args(source, scope, out);
}
