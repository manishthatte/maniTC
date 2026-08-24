//! Definitional interpreter for the ManiT core.
//!
//! © Manish Jagdish Thatte
//!
//! This implements docs/semantics.md and nothing else. Where the two differ,
//! this file is wrong. It is written for auditability rather than speed — the
//! recommendations ask for "deliberately slow and obviously correct" — so every
//! rule appears once, in the order the document states it, with the document's
//! section number on it.
//!
//! Independence rule: see lex.rs.

use super::ast::*;
use std::collections::HashMap;

/// §3. T3_MAX = (3^27 - 1) / 2.
pub const T3_MAX: i64 = 3_812_798_742_493;

/// Which version of the language this account is evaluating (R2).
///
/// The reference keeps its OWN copy of this distinction rather than importing
/// the compiler's `LangVersion`, for the reason the whole module exists: a
/// reference implementation that shares a definition with the thing it checks
/// cannot witness a mistake in that definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    /// `/` truncates toward zero, `%` takes the dividend's sign.
    #[default]
    V1,
    /// C4: `/` rounds to nearest, ties away from zero; `%` is the balanced
    /// remainder that pairs with it.
    V2,
}

/// C4, written from `docs/semantics.md` §6.1 rather than from the compiler.
///
/// Deliberately the OBVIOUS transcription of the rule — widen, take absolute
/// values, compare `2|r|` against `|b|` — and not the negative-magnitude form
/// the compiler and both backends use. If the two forms disagree anywhere, the
/// conformance suite is what says so, and it can only say so while they are
/// written differently.
fn div_nearest_ref(a: i64, b: i64) -> i64 {
    let (x, y) = (a as i128, b as i128);
    let q = x / y;
    let r = x - q * y;
    if r == 0 {
        return q as i64;
    }
    if 2 * r.abs() >= y.abs() {
        // Ties away from zero: away from the sign the quotient itself has.
        if (x < 0) == (y < 0) { (q + 1) as i64 } else { (q - 1) as i64 }
    } else {
        q as i64
    }
}
const LANES: usize = 27;

/// §3. Four value forms. `Trit` and `Bool3` share a carrier and differ only in
/// type; `Bool` does not share it — that is the hazard §6.4 documents.
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Int(i64),
    Trit(i8),
    Bool3(i8),
    Bool(bool),
    /// §6.8. A `Result<T, str>`.
    Res(Res),
    /// A string. In the core it exists only to be printed or carried as a
    /// `Result` message, so it has no operations of its own.
    Text(String),
}

/// §6.8. A `Result<T, str>` value.
///
/// One carrier for three outcomes, with the tag as a TRIT rather than a
/// boolean pair — which is the whole design claim: `Ok`, `Unknown` and `Err`
/// are three coordinate answers, not "success" and two flavours of failure.
/// `Unknown` carries a reason and is NOT a kind of `Err`.
#[derive(Debug, Clone, PartialEq)]
pub struct Res {
    /// +1 Ok, 0 Unknown, -1 Err.
    pub tag: i8,
    /// Present only when `tag == 1`.
    pub val: Option<Box<Val>>,
    /// The message carried by `Unknown` and `Err`; empty for `Ok`.
    pub msg: String,
}

impl Val {
    /// How `io::print_int` sees a value: the carrier, as an integer.
    fn as_print_int(&self) -> i64 {
        match self {
            Val::Int(n) => *n,
            Val::Trit(t) | Val::Bool3(t) => *t as i64,
            Val::Bool(b) => *b as i64,
            // §6.8: a Result's integer face is its TAG — the trit that says
            // which of the three outcomes it is. That is what `io::print_int`
            // shows and what `tif` dispatches on.
            Val::Res(r) => r.tag as i64,
            Val::Text(_) => 0,
        }
    }
}

/// §8. A trap ends the program; the trace produced so far is retained.
pub struct Trap(pub String);

/// Why an evaluation stopped early.
///
/// `?` is not a trap: it unwinds to the enclosing function and becomes that
/// function's return value (§6.9). Modelling both with one channel is what
/// keeps `?` out of the expression type — every expression would otherwise
/// have to return "a value or a propagation", and the rules in §5 would be
/// written twice.
pub enum Abort {
    Trap(Trap),
    /// The non-`Ok` `Result` being propagated out of the enclosing call.
    Propagate(Val),
}

type R<T> = Result<T, Abort>;

fn trap<T>(msg: impl Into<String>) -> R<T> {
    Err(Abort::Trap(Trap(msg.into())))
}

/// §7. `return` is not an expression, so it needs its own control-flow path.
enum Flow { Normal, Return(Option<Val>) }

pub struct Interp<'a> {
    fns: HashMap<String, &'a Fn>,
    /// §4. The output trace.
    pub out: String,
    /// A step budget. Not part of the semantics — a non-terminating program has
    /// no observable behaviour to compare, and the conformance harness needs to
    /// stop rather than hang. Exceeding it is reported distinctly from a trap.
    budget: u64,
    /// R2: the language version being evaluated. Read in exactly one place —
    /// the `/` and `%` rule of §6.1.
    lang: Lang,
}

pub struct Observation {
    pub out: String,
    pub trap: Option<String>,
}

pub fn run(program: &[Fn]) -> Result<Observation, String> {
    run_with(program, Lang::default())
}

pub fn run_with(program: &[Fn], lang: Lang) -> Result<Observation, String> {
    let mut fns = HashMap::new();
    for f in program {
        if fns.insert(f.name.clone(), f).is_some() {
            return Err(format!("duplicate function `{}`", f.name));
        }
    }
    if !fns.contains_key("main") {
        return Err("no `main`".into());
    }
    let mut it = Interp { fns, out: String::new(), budget: 200_000_000, lang };
    let main = it.fns["main"];
    match it.call_body(main, Vec::new()) {
        Ok(_) => Ok(Observation { out: it.out, trap: None }),
        Err(Abort::Trap(Trap(why))) => Ok(Observation { out: it.out, trap: Some(why) }),
        Err(Abort::Propagate(_)) => Ok(Observation {
            out: it.out,
            trap: Some("`?` propagated out of main".into()),
        }),
    }
}

// ---------------------------------------------------------------------------
// §6.6 lane decomposition
// ---------------------------------------------------------------------------

fn lanes(w: i64) -> [i8; LANES] {
    // Repeated balanced division. `w` is always in range here: §8 traps before
    // an out-of-range value can reach a lane operation.
    let mut out = [0i8; LANES];
    let mut n = w;
    for slot in out.iter_mut() {
        let mut r = n % 3;
        n /= 3;
        if r == 2 { r = -1; n += 1; }
        if r == -2 { r = 1; n -= 1; }
        *slot = r as i8;
    }
    out
}

fn from_lanes(l: &[i8; LANES]) -> i64 {
    let mut v = 0i64;
    let mut p = 1i64;
    for d in l.iter() {
        v += *d as i64 * p;
        p *= 3;
    }
    v
}

// ---------------------------------------------------------------------------
// §6.4, §6.5 per-trit connectives
// ---------------------------------------------------------------------------

fn t_and(a: i8, b: i8) -> i8 { a.min(b) }
fn t_or(a: i8, b: i8) -> i8 { a.max(b) }
/// §6.5. Written as the table the document prints, not as modular arithmetic.
fn t_xor(a: i8, b: i8) -> i8 {
    match (a, b) {
        (-1, -1) => 1, (-1, 0) => -1, (-1, 1) => 0,
        (0, -1) => -1, (0, 0) => 0, (0, 1) => 1,
        (1, -1) => 0, (1, 0) => 1, (1, 1) => -1,
        _ => unreachable!("not a trit: {} {}", a, b),
    }
}
/// §6.4. Łukasiewicz: the (0,0) cell is +1, which is what makes `a timp a` a
/// tautology. Kleene's `max(-a, b)` gives 0 there.
fn t_imp(a: i8, b: i8) -> i8 { (1 - a + b).min(1) }
fn t_con(a: i8, b: i8) -> i8 {
    if a == 1 && b == 1 { 1 } else if a == -1 && b == -1 { -1 } else { 0 }
}
fn t_any(a: i8, b: i8) -> i8 {
    if a == 1 || b == 1 { 1 } else if a == -1 || b == -1 { -1 } else { 0 }
}
fn t_cmp(a: i8, b: i8) -> i8 { if a > b { 1 } else if a < b { -1 } else { 0 } }

impl<'a> Interp<'a> {
    fn tick(&mut self) -> R<()> {
        if self.budget == 0 {
            return trap("step budget exhausted (non-terminating?)");
        }
        self.budget -= 1;
        Ok(())
    }

    // ---- §8 range check ---------------------------------------------------

    fn checked(&self, v: i128, what: &str) -> R<Val> {
        if v > T3_MAX as i128 || v < -(T3_MAX as i128) {
            return trap(format!(
                "{} overflow: result {} is outside the 27-trit range [{}, {}]",
                what, v, -T3_MAX, T3_MAX
            ));
        }
        Ok(Val::Int(v as i64))
    }

    // ---- §6.4 operand coercion -------------------------------------------

    /// §6.4. A `Bool` operand of a three-valued operator converts by
    /// `b -> 2b - 1`, so `false` becomes -1. An `Int` operand is NOT accepted:
    /// applying a three-valued connective to a ternary NUMBER is undefined
    /// (report.txt P1).
    fn trit_operand(&self, v: Val, op: &str) -> R<i8> {
        Ok(match v {
            Val::Trit(t) | Val::Bool3(t) => t,
            Val::Bool(b) => if b { 1 } else { -1 },
            // A `Result`'s tag IS a trit (§6.8), so `tif r.tag()` and
            // `tif r` dispatch the same way. That is the design claim made
            // operational rather than a convenience.
            Val::Res(ref r) => r.tag,
            Val::Int(_) => return trap(format!(
                "`{}` applied to an int: a three-valued operator takes trit or bool3 (semantics.md §6.4)", op)),
            Val::Text(_) => return trap(format!(
                "`{}` applied to a str", op)),
        })
    }

    /// §6.6. A lane-wise operand is a word.
    fn word_operand(&self, v: Val, op: &str) -> R<i64> {
        match v {
            Val::Int(n) => Ok(n),
            _ => trap(format!(
                "`{}` applied to a non-word: lane-wise operators take int (semantics.md §6.6)", op)),
        }
    }

    // ---- calls ------------------------------------------------------------

    fn call_body(&mut self, f: &'a Fn, args: Vec<Val>) -> R<Option<Val>> {
        let mut env: Vec<HashMap<String, (Val, bool)>> = vec![HashMap::new()];
        for ((name, ty), v) in f.params.iter().zip(args) {
            let v = coerce_decl(v, ty.clone());
            env[0].insert(name.clone(), (v, false));
        }
        // §6.9. A `?` inside this body unwinds to HERE and becomes the call's
        // return value. That is the whole of what `?` does: it is a return, not
        // an error, which is why `Unknown` survives it intact.
        match self.stmts(&f.body, &mut env) {
            Ok(Flow::Return(v)) => Ok(v),
            Ok(Flow::Normal) => Ok(None),
            Err(Abort::Propagate(v)) => Ok(Some(v)),
            Err(other) => Err(other),
        }
    }

    fn stmts(
        &mut self,
        body: &'a [Stmt],
        env: &mut Vec<HashMap<String, (Val, bool)>>,
    ) -> R<Flow> {
        env.push(HashMap::new());
        let r = self.stmts_inner(body, env);
        env.pop();
        r
    }

    fn stmts_inner(
        &mut self,
        body: &'a [Stmt],
        env: &mut Vec<HashMap<String, (Val, bool)>>,
    ) -> R<Flow> {
        for s in body {
            self.tick()?;
            match s {
                Stmt::Let { name, mutable, ty, init } => {
                    let v = self.expr(init, env)?;
                    let v = match ty { Some(t) => coerce_decl(v, t.clone()), None => v };
                    env.last_mut().unwrap().insert(name.clone(), (v, *mutable));
                }
                Stmt::Assign { name, val } => {
                    let v = self.expr(val, env)?;
                    let mut done = false;
                    for scope in env.iter_mut().rev() {
                        if let Some((slot, mutable)) = scope.get_mut(name) {
                            if !*mutable {
                                return trap(format!(
                                    "cannot assign to immutable binding `{}`", name));
                            }
                            // Assignment preserves the binding's value FORM, so
                            // `let mut t: trit = +; t = 5;` clamps as §6.7 says.
                            let form = slot.clone();
                            *slot = reshape(v, form);
                            done = true;
                            break;
                        }
                    }
                    if !done {
                        return trap(format!("assignment to unbound `{}`", name));
                    }
                }
                Stmt::If { arms, els } => {
                    let mut taken = false;
                    for (c, b) in arms {
                        let cv = self.expr(c, env)?;
                        if truthy(cv) {
                            if let Flow::Return(v) = self.stmts(b, env)? {
                                return Ok(Flow::Return(v));
                            }
                            taken = true;
                            break;
                        }
                    }
                    if !taken {
                        if let Some(b) = els {
                            if let Flow::Return(v) = self.stmts(b, env)? {
                                return Ok(Flow::Return(v));
                            }
                        }
                    }
                }
                Stmt::Tif { scrutinee, pos, zero, neg } => {
                    // §7. Three arms, dispatched on the sign of the carrier.
                    let v = self.expr(scrutinee, env)?;
                    let t = self.trit_operand(v, "tif")?;
                    let arm = if t > 0 { pos } else if t == 0 { zero } else { neg };
                    if let Flow::Return(v) = self.stmts(arm, env)? {
                        return Ok(Flow::Return(v));
                    }
                }
                Stmt::While { cond, body } => loop {
                    self.tick()?;
                    let c = self.expr(cond, env)?;
                    if !truthy(c) { break; }
                    if let Flow::Return(v) = self.stmts(body, env)? {
                        return Ok(Flow::Return(v));
                    }
                },
                Stmt::Return(e) => {
                    let v = match e { Some(e) => Some(self.expr(e, env)?), None => None };
                    return Ok(Flow::Return(v));
                }
                Stmt::Expr(e) => { self.expr(e, env)?; }
            }
        }
        Ok(Flow::Normal)
    }

    // ---- expressions ------------------------------------------------------

    fn expr(
        &mut self,
        e: &'a Expr,
        env: &mut Vec<HashMap<String, (Val, bool)>>,
    ) -> R<Val> {
        self.tick()?;
        match e {
            Expr::Int(n) => self.checked(*n as i128, "literal"),
            Expr::TritLit(t) => Ok(Val::Trit(*t)),
            Expr::BoolLit(b) => Ok(Val::Bool(*b)),
            Expr::Bool3Lit(t) => Ok(Val::Bool3(*t)),
            Expr::Str(t) => Ok(Val::Text(t.clone())),
            Expr::Var(name) => {
                for scope in env.iter().rev() {
                    if let Some((v, _)) = scope.get(name) { return Ok(v.clone()); }
                }
                trap(format!("unbound identifier `{}`", name))
            }
            Expr::Cast(inner, ty) => {
                let v = self.expr(inner, env)?;
                Ok(cast(v, ty.clone()))
            }
            Expr::Un(op, inner) => {
                let v = self.expr(inner, env)?;
                match op {
                    Un::Neg => {
                        let n = v.as_print_int() as i128;
                        self.checked(-n, "negation")
                    }
                    // §6.4
                    Un::Tnot => {
                        let t = self.trit_operand(v.clone(), "tnot")?;
                        Ok(match v { Val::Bool3(_) => Val::Bool3(-t), _ => Val::Trit(-t) })
                    }
                    Un::Tposs => Ok(Val::Bool(self.trit_operand(v, "tposs")? >= 0)),
                    Un::Tnec => Ok(Val::Bool(self.trit_operand(v, "tnec")? == 1)),
                    // §6.6. tnotw a = -a, because negating a balanced-ternary
                    // number negates every trit.
                    Un::Tnotw => {
                        let w = self.word_operand(v, "tnotw")?;
                        self.checked(-(w as i128), "tnotw")
                    }
                }
            }
            Expr::Bin(op, l, r) => self.binop(*op, l, r, env),
            Expr::Call(name, args) => self.call(name, args, env),

            // §6.8. The three constructors. `Ok` carries a value; `Unknown`
            // and `Err` carry a message. `Unknown` is NOT a kind of `Err` and
            // the two are never merged here.
            Expr::Method(recv, name, args) => {
                let r = self.expr(recv, env)?;
                self.result_method(r, name, args, env)
            }

            // §6.9. `?` — propagate the whole non-Ok Result out of the
            // enclosing function; evaluate to the payload on Ok.
            //
            // The propagated value is the ORIGINAL Result, message intact, so
            // `Unknown("why")` arrives at the caller still saying why and
            // still saying Unknown. Collapsing it to Err here would be the
            // exact mistake the type exists to prevent.
            Expr::Try(inner) => {
                let v = self.expr(inner, env)?;
                match v {
                    Val::Res(r) if r.tag == 1 => {
                        Ok(*r.val.clone().unwrap_or(Box::new(Val::Int(0))))
                    }
                    Val::Res(r) => Err(Abort::Propagate(Val::Res(r))),
                    other => trap(format!("`?` applied to a non-Result: {:?}", other)),
                }
            }

            // §6.10. `match` on a Result. Exhaustiveness is enforced by the
            // parser, so by here one arm always applies.
            Expr::Match(scrut, arms) => {
                let v = self.expr(scrut, env)?;
                let r = match v {
                    Val::Res(r) => r,
                    other => return trap(format!(
                        "the core only matches on a Result, got {:?}", other)),
                };
                let want = match r.tag { 1 => "Ok", 0 => "Unknown", _ => "Err" };
                let arm = arms.iter().find(|a| a.variant == want)
                    .or_else(|| arms.iter().find(|a| a.variant == "_"))
                    .ok_or_else(|| Abort::Trap(Trap(format!(
                        "no arm for `{}` — the parser should have refused this", want))))?;
                let bound = match r.tag {
                    1 => *r.val.clone().unwrap_or(Box::new(Val::Int(0))),
                    _ => Val::Text(r.msg.clone()),
                };
                env.push(HashMap::new());
                if let Some(b) = &arm.binding {
                    env.last_mut().unwrap().insert(b.clone(), (bound, false));
                }
                let flow = self.stmts_inner(&arm.body, env);
                env.pop();
                match flow? {
                    Flow::Return(v) => Err(Abort::Propagate(v.unwrap_or(Val::Int(0)))),
                    Flow::Normal => Ok(Val::Int(0)),
                }
            }
        }
    }

    fn binop(
        &mut self,
        op: Bin,
        l: &'a Expr,
        r: &'a Expr,
        env: &mut Vec<HashMap<String, (Val, bool)>>,
    ) -> R<Val> {
        // §6.3. The two exceptions to left-to-right-then-apply: the right
        // operand is not evaluated at all, so its output is not produced.
        if matches!(op, Bin::AndAnd | Bin::OrOr) {
            let lv = truthy(self.expr(l, env)?);
            return match op {
                Bin::AndAnd => if !lv { Ok(Val::Bool(false)) }
                               else { Ok(Val::Bool(truthy(self.expr(r, env)?))) },
                _ => if lv { Ok(Val::Bool(true)) }
                     else { Ok(Val::Bool(truthy(self.expr(r, env)?))) },
            };
        }

        // §5. Left to right, fully.
        let a = self.expr(l, env)?;
        let b = self.expr(r, env)?;

        match op {
            // §6.1
            Bin::Add => self.checked(a.as_print_int() as i128 + b.as_print_int() as i128, "add"),
            Bin::Sub => self.checked(a.as_print_int() as i128 - b.as_print_int() as i128, "sub"),
            Bin::Mul => self.checked(a.as_print_int() as i128 * b.as_print_int() as i128, "mul"),
            Bin::Div | Bin::Rem => {
                let (x, y) = (a.as_print_int(), b.as_print_int());
                if y == 0 {
                    return trap(format!(
                        "division by zero: {} {} 0", x, if op == Bin::Div { "/" } else { "%" }));
                }
                match self.lang {
                    // Truncating toward zero, remainder taking the dividend's
                    // sign. Rust's `/` and `%` are already defined that way,
                    // which is why this is not spelled out further.
                    Lang::V1 => Ok(Val::Int(if op == Bin::Div { x / y } else { x % y })),
                    // C4. `%` is DEFINED from `/` here, not given a rule of its
                    // own — that is what makes `(a / b) * b + (a % b) == a`
                    // hold, and stating the remainder separately would be
                    // stating the identity twice and inviting the two
                    // statements to disagree.
                    Lang::V2 => {
                        let q = div_nearest_ref(x, y);
                        if op == Bin::Div {
                            // The quotient can leave the word: `T3_MIN / -1` is
                            // in range, but nothing else is, and the check
                            // belongs here rather than being assumed.
                            self.checked(q as i128, "div")
                        } else {
                            self.checked(x as i128 - (q as i128) * (y as i128), "rem")
                        }
                    }
                }
            }
            // §6.2
            Bin::Eq | Bin::Ne | Bin::Lt | Bin::Gt | Bin::Le | Bin::Ge => {
                let (x, y) = (a.as_print_int(), b.as_print_int());
                Ok(Val::Bool(match op {
                    Bin::Eq => x == y, Bin::Ne => x != y,
                    Bin::Lt => x < y, Bin::Gt => x > y,
                    Bin::Le => x <= y, _ => x >= y,
                }))
            }
            // §6.4
            Bin::Tand | Bin::Tor | Bin::Txor | Bin::Tcon | Bin::Tany
            | Bin::Timp | Bin::Teq => {
                let name = tlogic_name(op);
                let (x, y) = (self.trit_operand(a.clone(), name)?, self.trit_operand(b.clone(), name)?);
                let z = match op {
                    Bin::Tand => t_and(x, y),
                    Bin::Tor => t_or(x, y),
                    Bin::Txor => t_xor(x, y),
                    Bin::Tcon => t_con(x, y),
                    Bin::Tany => t_any(x, y),
                    Bin::Timp => t_imp(x, y),
                    _ => t_and(t_imp(x, y), t_imp(y, x)),
                };
                // §6.4 result typing. tand/tor/tany/timp/teq are closed on
                // {-1,+1}, so two Bools give a Bool; txor and tcon are not.
                let closed = matches!(op,
                    Bin::Tand | Bin::Tor | Bin::Tany | Bin::Timp | Bin::Teq);
                Ok(match (a, b) {
                    (Val::Bool(_), Val::Bool(_)) if closed => Val::Bool(z > 0),
                    (Val::Bool(_), Val::Bool(_))
                    | (Val::Bool3(_), Val::Bool3(_))
                    | (Val::Bool(_), Val::Bool3(_))
                    | (Val::Bool3(_), Val::Bool(_)) => Val::Bool3(z),
                    _ => Val::Trit(z),
                })
            }
            // §6.6
            Bin::Tandw | Bin::Torw | Bin::Txorw | Bin::Timpw | Bin::Tcmpw => {
                let name = tlogic_name(op);
                let (x, y) = (self.word_operand(a, name)?, self.word_operand(b, name)?);
                let (lx, ly) = (lanes(x), lanes(y));
                let mut out = [0i8; LANES];
                for i in 0..LANES {
                    out[i] = match op {
                        Bin::Tandw => t_and(lx[i], ly[i]),
                        Bin::Torw => t_or(lx[i], ly[i]),
                        Bin::Txorw => t_xor(lx[i], ly[i]),
                        Bin::Timpw => t_imp(lx[i], ly[i]),
                        _ => t_cmp(lx[i], ly[i]),
                    };
                }
                Ok(Val::Int(from_lanes(&out)))
            }
            Bin::AndAnd | Bin::OrOr => unreachable!("handled above"),
        }
    }

    fn call(
        &mut self,
        name: &str,
        args: &'a [Expr],
        env: &mut Vec<HashMap<String, (Val, bool)>>,
    ) -> R<Val> {
        // §6.8. The three `Result` constructors.
        match name {
            "Ok" | "Unknown" | "Err" => {
                let payload = match args.first() {
                    Some(a) => self.expr(a, env)?,
                    None => Val::Int(0),
                };
                return Ok(Val::Res(match name {
                    "Ok" => Res { tag: 1, val: Some(Box::new(payload)), msg: String::new() },
                    "Unknown" => Res {
                        tag: 0, val: None,
                        msg: match payload { Val::Text(t) => t, o => o.as_print_int().to_string() },
                    },
                    _ => Res {
                        tag: -1, val: None,
                        msg: match payload { Val::Text(t) => t, o => o.as_print_int().to_string() },
                    },
                }));
            }
            _ => {}
        }

        // §1. The core's only library surface: the four printers.
        match name {
            "io::print" | "io::println" => {
                for a in args {
                    if let Expr::Str(s) = a {
                        self.out.push_str(s);
                    } else {
                        match self.expr(a, env)? {
                            Val::Text(t) => self.out.push_str(&t),
                            v => self.out.push_str(&v.as_print_int().to_string()),
                        }
                    }
                }
                if name == "io::println" { self.out.push('\n'); }
                return Ok(Val::Int(0));
            }
            "io::print_int" | "io::println_int" => {
                for a in args {
                    match self.expr(a, env)? {
                        Val::Text(t) => self.out.push_str(&t),
                        v => self.out.push_str(&v.as_print_int().to_string()),
                    }
                }
                if name == "io::println_int" { self.out.push('\n'); }
                return Ok(Val::Int(0));
            }
            _ => {}
        }

        let f = *self.fns.get(name).ok_or_else(|| Abort::Trap(Trap(format!(
                "call to `{}`, which is not in the core and not defined here", name))))?;
        // §5. Arguments left to right, before the call.
        let mut vals = Vec::new();
        for a in args {
            vals.push(self.expr(a, env)?);
        }
        if vals.len() != f.params.len() {
            return trap(format!(
                "`{}` takes {} argument(s), given {}", name, f.params.len(), vals.len()));
        }
        let ret = self.call_body(f, vals)?;
        Ok(match (ret, f.ret.clone()) {
            (Some(v), t) => coerce_decl(v, t),
            (None, _) => Val::Int(0),
        })
    }
}

impl<'a> Interp<'a> {
    /// §6.8. The six `Result` accessors the reference documents.
    ///
    /// `tag()` is the primitive one — it hands back the trit, which is what
    /// makes `tif r.tag()` a single three-way dispatch. `is_ok`/`is_unknown`/
    /// `is_err` are that same question asked one yes-or-no at a time.
    fn result_method(
        &mut self,
        recv: Val,
        name: &str,
        args: &'a [Expr],
        env: &mut Vec<HashMap<String, (Val, bool)>>,
    ) -> R<Val> {
        let r = match recv {
            Val::Res(r) => r,
            other => return trap(format!("`.{}()` on a non-Result: {:?}", name, other)),
        };
        match name {
            "tag" => Ok(Val::Trit(r.tag)),
            "is_ok" => Ok(Val::Bool(r.tag == 1)),
            "is_unknown" => Ok(Val::Bool(r.tag == 0)),
            "is_err" => Ok(Val::Bool(r.tag == -1)),
            // §8: `unwrap` names ONE of three outcomes, so the other two trap.
            // The two messages differ, because "it failed" and "we do not know"
            // are different facts and a shared message would hide which.
            "unwrap" => match r.tag {
                1 => Ok(*r.val.clone().unwrap_or(Box::new(Val::Int(0)))),
                0 => trap("unwrap on a Result that is Unknown"),
                _ => trap("unwrap on a Result that is Err"),
            },
            // The default is evaluated either way — the reference says so, and
            // it is observable when the argument prints.
            "unwrap_or" => {
                let d = match args.first() {
                    Some(a) => self.expr(a, env)?,
                    None => return trap("unwrap_or takes one argument"),
                };
                Ok(if r.tag == 1 {
                    *r.val.clone().unwrap_or(Box::new(Val::Int(0)))
                } else {
                    d
                })
            }
            other => trap(format!("`.{}()` is not a core Result method", other)),
        }
    }
}

fn tlogic_name(op: Bin) -> &'static str {
    match op {
        Bin::Tand => "tand", Bin::Tor => "tor", Bin::Txor => "txor",
        Bin::Tcon => "tcon", Bin::Tany => "tany", Bin::Timp => "timp",
        Bin::Teq => "teq", Bin::Tandw => "tandw", Bin::Torw => "torw",
        Bin::Txorw => "txorw", Bin::Timpw => "timpw", Bin::Tcmpw => "tcmpw",
        _ => "?",
    }
}

/// §7. `if` and `while` take a `Bool`. A `Bool` is 0/1, and a nonzero carrier
/// of any other form is true — which is what the compiler's `Int -> Bool` cast
/// does (§6.7).
fn truthy(v: Val) -> bool {
    match v {
        Val::Bool(b) => b,
        other => other.as_print_int() != 0,
    }
}

/// §6.7 casts.
fn cast(v: Val, to: Ty) -> Val {
    let carrier = v.as_print_int();
    match to {
        Ty::Int => Val::Int(carrier),
        // Int -> Trit CLAMPS. Not a truncation: `5 as trit` is +1.
        Ty::Trit => Val::Trit(carrier.clamp(-1, 1) as i8),
        Ty::Bool3 => Val::Bool3(carrier.clamp(-1, 1) as i8),
        Ty::Bool => Val::Bool(carrier != 0),
        Ty::Void => Val::Int(0),
        // A `Result` and a `str` are not scalars and the core defines no cast
        // to either: a declared type of that shape leaves the value alone.
        Ty::Str | Ty::Result(_) => v,
    }
}

/// A declared type on a `let`, a parameter or a return applies the §6.7 cast.
fn coerce_decl(v: Val, ty: Ty) -> Val {
    match ty { Ty::Void => v, t => cast(v, t) }
}

/// Assignment keeps the binding's existing value form (§7): a `trit` binding
/// stays a trit, so the assigned value is cast to it.
fn reshape(v: Val, like: Val) -> Val {
    match like {
        Val::Int(_) => cast(v, Ty::Int),
        Val::Trit(_) => cast(v, Ty::Trit),
        Val::Bool3(_) => cast(v, Ty::Bool3),
        Val::Bool(_) => cast(v, Ty::Bool),
        // Assigning into a Result- or str-shaped binding replaces it wholesale;
        // there is no narrowing to do.
        Val::Res(_) | Val::Text(_) => v,
    }
}
