# Subsystem: `lunaferra-math`

`lunaferra-math` computes the floating-point functions that IEEE 754, the floating-point standard,
doesn't define exactly, such as sine, the exponential, the logarithm, and float powers. It uses
only the operations IEEE 754 does define exactly, so it gives the same bits on every target
(ADR-0027: six platforms, and the same results on each). The compiler's constant folding, the
VM's `**`, the JIT, and the `math` library all call it, so a value never depends on which of them
computed it. Its accuracy and its place as a crate of its own come from ADR-0028. It lands in
M2.19.

## Why a math library of Lunaferra's own

Rust's `f64::sin`, `f64::exp`, `f64::powf`, and their kin call each platform's math library:
glibc on Linux, Microsoft's on Windows, Apple's on macOS and iOS, Bionic on Android, and a
portable one on the web. IEEE 754 requires none of these functions to be correctly rounded, so the
libraries can differ in a result's last bit. A script that prints `math.sin(1)` with every digit,
or a game that replays a recorded run on another machine, would then see different results. The
basic operations, addition, subtraction, multiplication, division, and square root, are correctly
rounded on every target, so code built only from them gives the same bits everywhere. The cost is
a library to write and test, and results that can differ in the last bit from the C library that
PUC-Rio Lua uses.

## Functions

| Function | Lua names |
| :- | :- |
| `sin`, `cos`, `tan` | `math.sin`, `math.cos`, `math.tan` |
| `asin`, `acos`, `atan`, and `atan2` | `math.asin`, `math.acos`, `math.atan` with one or two arguments |
| `exp` | `math.exp` |
| `log`, `log2`, `log10`, and the logarithm in any base | `math.log` with one or two arguments |
| `pow` | `**` when either operand is a float |

`math.sqrt`, `math.floor`, `math.ceil`, `math.fmod`, and `math.abs` need no entry: IEEE 754 defines
each of them exactly, so the VM and the library compute them directly.

## Accuracy and method

- Each result is within one unit in the last place (ulp) of the exact value: the distance between
  the result and the true value is less than the gap between two adjacent floats there.
- The code uses only `+`, `-`, `*`, `/`, and `sqrt` on `f64`, and integer operations. It never
  calls a platform's library, never uses fused multiply-add, and never depends on the order a
  compiler might choose, because Rust doesn't reorder floating-point operations.
- Constants, such as the coefficients of each polynomial, are written as exact bit patterns, so
  no decimal conversion can round them differently.
- Trigonometric functions reduce large arguments exactly, so `math.sin(1e300)` is as accurate as
  `math.sin(1)`.
- Special values follow Annex F of the C standard, as the C library PUC-Rio uses does: for
  example, `pow(1, nan)` is 1, and `exp(-inf)` is 0. The VM decides which of these results raise an
  error instead, such as a power of zero with a negative exponent, as
  [`language.md`](language.md#meanings) says (M2.9).

## Invariants

1. For every input in the stored file, each function gives the bits stored for it, on every
   target.
2. For every stored input, each result is within one ulp of the exact value the file records.
3. Each function is pure: the same input gives the same bits every time, and no input panics.
4. Special values follow Annex F of the C standard.

## Tests

- Invariants 1 and 2: the stored file lists inputs, each input's exact value to 40 digits, and the
  bits of Lunaferra's result, including zeros, subnormal numbers, the largest floats, `inf`, `nan`,
  and the inputs near which each function's result changes fastest. A contributor computes the
  exact values with any high-precision tool; the project doesn't depend on one. The test runs on
  every target in CI from M3.18 on ([`03-platforms.md`](../03-platforms.md)).
- Invariant 3: a property test calls each function twice on generated inputs and compares the
  bits.
- Invariant 4: a unit test for each special case in Annex F.
