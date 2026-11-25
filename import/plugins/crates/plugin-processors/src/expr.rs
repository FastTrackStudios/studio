//! The mapping language: one expression per engine target, over an
//! algorithm's controls, compiled to a small bytecode that runs on the
//! audio thread.
//!
//! ```text
//! band.1.q   "lf_bell ? 0.6 + abs(lf_gain) * 0.06 : 0.707"
//! band.2.q   "curve(lmf_gain, lmf_q)"
//! band.1.shape "lf_bell ? bell : low_shelf"
//! ```
//!
//! - Numbers, the algorithm's control ids, the target's choice names
//!   (`bell`, `low_shelf`: the engine's labels, lowercased, spaces as `_`),
//!   and the algorithm's curves (breakpoint tables) by name. In a choice
//!   target's expression a choice name wins over a control of the same id
//!   (`band.1.shape low_cut` is the shape, beside a `low_cut` knob).
//! - `? :`, `|| &&`, `== != < <= > >=`, `+ -`, `* /`, `^`, unary `- !`.
//! - Functions: `abs min max clamp(x,lo,hi) pow exp log10 sqrt floor round
//!   db_to_gain gain_to_db curve(x, name) lerp(a,b,t)`.
//!
//! Safe by construction: no loops, no jumps (both arms of `?:` are
//! evaluated and one selected), no allocation, a fixed stack and a bounded
//! program, so a downloaded algorithm can't hang or grow the audio thread.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// The most a whole algorithm's program may hold.
pub const MAX_OPS: usize = 8192;
/// The deepest the evaluation stack goes.
pub const MAX_DEPTH: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    Const(f32),
    Control(u16),
    /// Piecewise-linear lookup in curve `n` of the top of the stack.
    Curve(u16),
    Neg,
    Not,
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    And,
    Or,
    /// `c ? a : b` with `c a b` on the stack.
    Select,
    Abs,
    Min,
    Max,
    Clamp,
    Exp,
    Log10,
    Sqrt,
    Floor,
    Round,
    DbToGain,
    GainToDb,
    Lerp,
}

/// Why an expression didn't compile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExprError {
    pub at: usize,
    pub why: String,
}

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "at {}: {}", self.at, self.why)
    }
}

/// What names mean while compiling one target's expression.
pub struct Scope<'a> {
    pub controls: &'a [&'a str],
    /// The target's choice names, if it is a choice.
    pub choices: &'a [&'a str],
    pub curves: &'a [&'a str],
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f32),
    Ident(String),
    Sym(&'static str),
}

fn lex(src: &str) -> Result<Vec<(usize, Tok)>, ExprError> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let err = |at: usize, why: &str| ExprError {
        at,
        why: why.into(),
    };
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        if c.is_ascii_digit() || (c == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            while i < b.len()
                && (b[i].is_ascii_digit()
                    || b[i] == b'.'
                    || b[i] == b'e'
                    || b[i] == b'E'
                    || ((b[i] == b'-' || b[i] == b'+')
                        && matches!(b.get(i.wrapping_sub(1)), Some(b'e' | b'E'))))
            {
                i += 1;
            }
            let text = src.get(start..i).ok_or_else(|| err(start, "bad number"))?;
            let v: f32 = text.parse().map_err(|_| err(start, "bad number"))?;
            out.push((start, Tok::Num(v)));
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'.') {
                i += 1;
            }
            let text = src.get(start..i).ok_or_else(|| err(start, "bad name"))?;
            out.push((start, Tok::Ident(text.into())));
            continue;
        }
        const SYMS: &[&str] = &[
            "&&", "||", "==", "!=", "<=", ">=", "<", ">", "+", "-", "*", "/", "^", "!", "?", ":",
            "(", ")", ",",
        ];
        let rest = src.get(i..).unwrap_or("");
        let Some(sym) = SYMS.iter().find(|s| rest.starts_with(**s)) else {
            return Err(err(i, "unexpected character"));
        };
        out.push((i, Tok::Sym(sym)));
        i += sym.len();
    }
    Ok(out)
}

struct Parser<'a> {
    toks: Vec<(usize, Tok)>,
    pos: usize,
    scope: &'a Scope<'a>,
    ops: Vec<Op>,
    depth: usize,
    max_depth: usize,
    end: usize,
}

fn norm_name(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c == ' ' || c == '-' {
                '_'
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect()
}

impl Parser<'_> {
    fn at(&self) -> usize {
        self.toks.get(self.pos).map_or(self.end, |t| t.0)
    }

    fn err<T>(&self, why: impl Into<String>) -> Result<T, ExprError> {
        Err(ExprError {
            at: self.at(),
            why: why.into(),
        })
    }

    fn peek_sym(&self, s: &str) -> bool {
        matches!(self.toks.get(self.pos), Some((_, Tok::Sym(x))) if *x == s)
    }

    fn eat(&mut self, s: &str) -> bool {
        if self.peek_sym(s) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, s: &str) -> Result<(), ExprError> {
        if self.eat(s) {
            Ok(())
        } else {
            self.err(alloc::format!("expected `{s}`"))
        }
    }

    fn emit(&mut self, op: Op, pushes: isize) -> Result<(), ExprError> {
        if self.ops.len() >= MAX_OPS {
            return self.err("expression too long");
        }
        self.ops.push(op);
        self.depth = self.depth.saturating_add_signed(pushes);
        self.max_depth = self.max_depth.max(self.depth);
        if self.max_depth > MAX_DEPTH {
            return self.err("expression nests too deeply");
        }
        Ok(())
    }

    fn ternary(&mut self) -> Result<(), ExprError> {
        self.or()?;
        if self.eat("?") {
            self.ternary()?;
            self.expect(":")?;
            self.ternary()?;
            self.emit(Op::Select, -2)?;
        }
        Ok(())
    }

    fn binary(
        &mut self,
        next: fn(&mut Self) -> Result<(), ExprError>,
        ops: &[(&str, Op)],
    ) -> Result<(), ExprError> {
        next(self)?;
        'outer: loop {
            for (s, op) in ops {
                if self.eat(s) {
                    next(self)?;
                    self.emit(*op, -1)?;
                    continue 'outer;
                }
            }
            return Ok(());
        }
    }

    fn or(&mut self) -> Result<(), ExprError> {
        self.binary(Self::and, &[("||", Op::Or)])
    }
    fn and(&mut self) -> Result<(), ExprError> {
        self.binary(Self::cmp, &[("&&", Op::And)])
    }
    fn cmp(&mut self) -> Result<(), ExprError> {
        self.binary(
            Self::add,
            &[
                ("==", Op::Eq),
                ("!=", Op::Ne),
                ("<=", Op::Le),
                (">=", Op::Ge),
                ("<", Op::Lt),
                (">", Op::Gt),
            ],
        )
    }
    fn add(&mut self) -> Result<(), ExprError> {
        self.binary(Self::mul, &[("+", Op::Add), ("-", Op::Sub)])
    }
    fn mul(&mut self) -> Result<(), ExprError> {
        self.binary(Self::pow, &[("*", Op::Mul), ("/", Op::Div)])
    }
    fn pow(&mut self) -> Result<(), ExprError> {
        self.unary()?;
        if self.eat("^") {
            self.pow()?; // right-associative
            self.emit(Op::Pow, -1)?;
        }
        Ok(())
    }
    fn unary(&mut self) -> Result<(), ExprError> {
        if self.eat("-") {
            self.unary()?;
            return self.emit(Op::Neg, 0);
        }
        if self.eat("!") {
            self.unary()?;
            return self.emit(Op::Not, 0);
        }
        self.atom()
    }

    fn args(&mut self, n: usize) -> Result<(), ExprError> {
        self.expect("(")?;
        for i in 0..n {
            if i > 0 {
                self.expect(",")?;
            }
            self.ternary()?;
        }
        self.expect(")")
    }

    fn atom(&mut self) -> Result<(), ExprError> {
        let Some((_, tok)) = self.toks.get(self.pos).cloned() else {
            return self.err("expected a value");
        };
        self.pos += 1;
        match tok {
            Tok::Num(v) => self.emit(Op::Const(v), 1),
            Tok::Sym("(") => {
                self.ternary()?;
                self.expect(")")
            }
            Tok::Sym(s) => self.err(alloc::format!("unexpected `{s}`")),
            Tok::Ident(name) => {
                let call = self.peek_sym("(");
                if call {
                    let (n, op) = match name.as_str() {
                        "abs" => (1, Op::Abs),
                        "exp" => (1, Op::Exp),
                        "log10" => (1, Op::Log10),
                        "sqrt" => (1, Op::Sqrt),
                        "floor" => (1, Op::Floor),
                        "round" => (1, Op::Round),
                        "db_to_gain" => (1, Op::DbToGain),
                        "gain_to_db" => (1, Op::GainToDb),
                        "min" => (2, Op::Min),
                        "max" => (2, Op::Max),
                        "pow" => (2, Op::Pow),
                        "clamp" => (3, Op::Clamp),
                        "lerp" => (3, Op::Lerp),
                        "curve" => return self.curve(),
                        other => return self.err(alloc::format!("no function `{other}`")),
                    };
                    self.args(n)?;
                    let pops = isize::try_from(n).unwrap_or(1) - 1;
                    return self.emit(op, -pops);
                }
                // A choice target's own choices first: a control named like
                // one (a channel strip's `low_cut` knob) must not shadow the
                // shape `low_cut`, which would set the shape to the knob's Hz.
                let n = norm_name(&name);
                if let Some(i) = self.scope.choices.iter().position(|c| norm_name(c) == n) {
                    return self.emit(Op::Const(crate::util_f32(i)), 1);
                }
                if let Some(i) = self.scope.controls.iter().position(|c| *c == name) {
                    return self.emit(Op::Control(u16::try_from(i).unwrap_or(u16::MAX)), 1);
                }
                match name.as_str() {
                    "true" => self.emit(Op::Const(1.0), 1),
                    "false" => self.emit(Op::Const(0.0), 1),
                    "pi" => self.emit(Op::Const(core::f32::consts::PI), 1),
                    _ => self.err(alloc::format!(
                        "`{name}` is not a control, a choice of this target, or a constant"
                    )),
                }
            }
        }
    }

    fn curve(&mut self) -> Result<(), ExprError> {
        self.expect("(")?;
        self.ternary()?;
        self.expect(",")?;
        let Some((_, Tok::Ident(name))) = self.toks.get(self.pos).cloned() else {
            return self.err("expected a curve name");
        };
        self.pos += 1;
        self.expect(")")?;
        let Some(i) = self.scope.curves.iter().position(|c| *c == name) else {
            return self.err(alloc::format!("no curve `{name}`"));
        };
        self.emit(Op::Curve(u16::try_from(i).unwrap_or(u16::MAX)), 0)
    }
}

/// Compile one expression.
///
/// # Errors
///
/// Where and why it doesn't parse, names something unknown, or is too
/// large.
pub fn compile(src: &str, scope: &Scope<'_>) -> Result<Vec<Op>, ExprError> {
    let toks = lex(src)?;
    let mut p = Parser {
        toks,
        pos: 0,
        scope,
        ops: Vec::new(),
        depth: 0,
        max_depth: 0,
        end: src.len(),
    };
    p.ternary()?;
    if p.pos != p.toks.len() {
        return p.err("unexpected text after the expression");
    }
    Ok(p.ops)
}

/// A breakpoint curve: `(x0 y0 x1 y1 …)`, x ascending.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    pub points: Vec<(f32, f32)>,
}

impl Curve {
    /// # Errors
    ///
    /// When the list is odd, empty, or its x don't ascend.
    pub fn from_flat(flat: &[f32]) -> Result<Self, String> {
        if flat.is_empty() || flat.len() % 2 != 0 {
            return Err("a curve is pairs of x y".into());
        }
        let points: Vec<(f32, f32)> = flat.chunks_exact(2).map(|p| (p[0], p[1])).collect();
        if points.windows(2).any(|w| w[1].0 <= w[0].0) {
            return Err("a curve's x must ascend".into());
        }
        Ok(Self { points })
    }

    #[must_use]
    pub fn at(&self, x: f32) -> f32 {
        let Some(&(x0, y0)) = self.points.first() else {
            return 0.0;
        };
        if x <= x0 {
            return y0;
        }
        for w in self.points.windows(2) {
            let ((xa, ya), (xb, yb)) = (w[0], w[1]);
            if x <= xb {
                let t = (x - xa) / (xb - xa);
                return ya + (yb - ya) * t;
            }
        }
        self.points.last().map_or(0.0, |p| p.1)
    }
}

/// Run `ops` over `controls` (plain values). No allocation.
#[must_use]
pub fn eval(ops: &[Op], controls: &[f32], curves: &[Curve]) -> f32 {
    let mut stack = [0.0_f32; MAX_DEPTH];
    let mut sp = 0_usize;
    macro_rules! pop {
        () => {{
            sp = sp.saturating_sub(1);
            stack.get(sp).copied().unwrap_or(0.0)
        }};
    }
    macro_rules! push {
        ($v:expr) => {{
            let v = $v;
            if let Some(slot) = stack.get_mut(sp) {
                *slot = v;
            }
            sp = (sp + 1).min(MAX_DEPTH);
        }};
    }
    let b = |c: bool| if c { 1.0 } else { 0.0 };
    for op in ops {
        match *op {
            Op::Const(v) => push!(v),
            Op::Control(i) => push!(controls.get(usize::from(i)).copied().unwrap_or(0.0)),
            Op::Curve(i) => {
                let x = pop!();
                push!(curves.get(usize::from(i)).map_or(0.0, |c| c.at(x)));
            }
            Op::Neg => {
                let a = pop!();
                push!(-a);
            }
            Op::Not => {
                let a = pop!();
                push!(b(a < 0.5));
            }
            Op::Abs => {
                let a = pop!();
                push!(libm::fabsf(a));
            }
            Op::Exp => {
                let a = pop!();
                push!(libm::expf(a));
            }
            Op::Log10 => {
                let a = pop!();
                push!(libm::log10f(a.max(1e-30)));
            }
            Op::Sqrt => {
                let a = pop!();
                push!(libm::sqrtf(a.max(0.0)));
            }
            Op::Floor => {
                let a = pop!();
                push!(libm::floorf(a));
            }
            Op::Round => {
                let a = pop!();
                push!(libm::roundf(a));
            }
            Op::DbToGain => {
                let a = pop!();
                push!(crate::db_to_gain(a));
            }
            Op::GainToDb => {
                let a = pop!();
                push!(20.0 * libm::log10f(a.max(1e-30)));
            }
            Op::Select => {
                let (f, t, c) = (pop!(), pop!(), pop!());
                push!(if c >= 0.5 { t } else { f });
            }
            Op::Clamp => {
                let (hi, lo, x) = (pop!(), pop!(), pop!());
                push!(x.max(lo).min(hi));
            }
            Op::Lerp => {
                let (t, bb, a) = (pop!(), pop!(), pop!());
                push!(a + (bb - a) * t);
            }
            op => {
                let (y, x) = (pop!(), pop!());
                push!(match op {
                    Op::Add => x + y,
                    Op::Sub => x - y,
                    Op::Mul => x * y,
                    Op::Div => {
                        if y == 0.0 { 0.0 } else { x / y }
                    }
                    Op::Pow => libm::powf(x, y),
                    Op::Lt => b(x < y),
                    Op::Le => b(x <= y),
                    Op::Gt => b(x > y),
                    Op::Ge => b(x >= y),
                    Op::Eq => b((x - y).abs() < 1e-6),
                    Op::Ne => b((x - y).abs() >= 1e-6),
                    Op::And => b(x >= 0.5 && y >= 0.5),
                    Op::Or => b(x >= 0.5 || y >= 0.5),
                    Op::Min => x.min(y),
                    Op::Max => x.max(y),
                    _ => 0.0,
                });
            }
        }
    }
    let v = if sp > 0 {
        stack.get(sp - 1).copied().unwrap_or(0.0)
    } else {
        0.0
    };
    if v.is_finite() { v } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str, controls: &[(&str, f32)]) -> f32 {
        let names: Vec<&str> = controls.iter().map(|c| c.0).collect();
        let values: Vec<f32> = controls.iter().map(|c| c.1).collect();
        let scope = Scope {
            controls: &names,
            choices: &["Bell", "Low Shelf"],
            curves: &["q"],
        };
        let ops = compile(src, &scope).unwrap_or_else(|e| panic!("{src}: {e}"));
        let curve = Curve::from_flat(&[0.0, 0.6, 15.0, 1.5]).unwrap_or_else(|e| panic!("{e}"));
        eval(&ops, &values, &[curve])
    }

    #[test]
    fn expressions_evaluate_as_written() {
        assert_eq!(run("1 + 2 * 3", &[]), 7.0);
        assert_eq!(run("(1 + 2) * 3", &[]), 9.0);
        assert_eq!(run("2 ^ 3 ^ 2", &[]), 512.0);
        assert_eq!(run("-gain", &[("gain", 4.0)]), -4.0);
        assert!(
            (run(
                "lf_bell ? 0.6 + abs(lf_gain) * 0.06 : 0.707",
                &[("lf_bell", 1.0), ("lf_gain", -10.0)]
            ) - 1.2)
                .abs()
                < 1e-6
        );
        assert!(
            (run(
                "lf_bell ? 0.6 + abs(lf_gain) * 0.06 : 0.707",
                &[("lf_bell", 0.0), ("lf_gain", -10.0)]
            ) - 0.707)
                .abs()
                < 1e-6
        );
        assert_eq!(run("lf_bell ? bell : low_shelf", &[("lf_bell", 0.0)]), 1.0);
        assert_eq!(
            run("low_shelf", &[("low_shelf", 500.0)]),
            1.0,
            "a choice name wins over a control of the same id"
        );
        assert!((run("curve(g, q)", &[("g", 7.5)]) - 1.05).abs() < 1e-6);
        assert_eq!(run("clamp(x, 0, 1)", &[("x", 3.0)]), 1.0);
        assert_eq!(run("x > 2 && x < 4", &[("x", 3.0)]), 1.0);
        assert_eq!(run("1 / 0", &[]), 0.0, "division by zero is 0, not inf");
        assert!((run("db_to_gain(-6)", &[]) - 0.501_187).abs() < 1e-5);
    }

    #[test]
    fn bad_expressions_say_where_and_why() {
        let scope = Scope {
            controls: &["a"],
            choices: &[],
            curves: &[],
        };
        for (src, why) in [
            ("a +", "expected a value"),
            ("b", "is not a control"),
            ("nope(a)", "no function"),
            ("a a", "unexpected text"),
            ("curve(a, z)", "no curve"),
            ("(a", "expected `)`"),
        ] {
            match compile(src, &scope) {
                Err(e) => assert!(e.why.contains(why), "{src}: {}", e.why),
                Ok(_) => panic!("{src} compiled"),
            }
        }
        // Bounded: a deep nest is refused, not run.
        let deep = "(".repeat(40) + "a" + &")".repeat(40);
        assert!(
            compile(&deep, &scope).is_ok(),
            "parentheses alone add no depth"
        );
        let wide = (0..40).map(|_| "a").collect::<Vec<_>>().join(" + (") + &")".repeat(39);
        assert!(
            compile(&wide, &scope).is_err(),
            "40 nested operands exceed the stack"
        );
    }
}
