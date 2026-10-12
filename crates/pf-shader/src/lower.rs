//! Checks a shader's types and turns it into instructions (see [`crate::vm`]), every call
//! expanded in place.
//!
//! GLSL has no recursion, so a call can always be expanded where it stands; one that would
//! recurse is refused. That leaves a program with no calls at all: a tree of blocks, `if`s, and
//! loops over numbered slots. Expanding is also what bounds it: the instructions are counted as
//! they are written, and a shader that expands past [`Limits::instructions`] is refused.
//!
//! Slots are handed out like a stack: a variable's until its block ends, a working value's until
//! its statement does. Constants have slots of their own, numbered after the rest.

use crate::ast::{Base, BinOp, Declared, Expr, Function, Node, Passing, Stmt, Ty, UnOp, Unit};
use crate::isf::{Header, InputKind};
use crate::vm::{Block, Ins, Loop, Node as Code, Op, Slot, Uniforms};
use crate::{Error, Limits};
use std::collections::HashMap;

/// Constants are numbered from here while a program is being written, and moved to follow the
/// other slots once it's known how many those are.
const CONSTANTS: Slot = 1 << 30;
/// Deepest level of `if`s, loops, and calls inside one another.
const MAX_LEVEL: u16 = 200;
/// Deepest the walk over the shader goes: statements, expressions, and calls inside one another.
const MAX_WALK: u32 = 600;
/// Deepest calls go inside one another.
const MAX_CALLS: usize = 32;
/// The stack a shader is read on: room for the deepest walk the limits above allow.
const STACK_BYTES: usize = 32 * 1024 * 1024;

/// Where the numbers every shader is given are among its uniforms.
const TIME: usize = 0;
const SIZE: usize = 1;
const TIME_DELTA: usize = 3;
const FRAME_INDEX: usize = 4;
const BUILT_IN: usize = 10;

type Res<T> = Result<T, Error>;

fn fail<T>(line: u32, message: impl Into<String>) -> Res<T> {
    Err(Error::new(line, message))
}

/// A shader ready to run.
#[derive(Debug, Clone)]
pub struct Program {
    pub(crate) code: Block,
    pub(crate) slots: u32,
    pub(crate) levels: u16,
    pub(crate) constants: Vec<(Slot, u32)>,
    pub(crate) uniform_slots: Vec<Slot>,
    pub(crate) coord: [Slot; 2],
    pub(crate) color: [Slot; 4],
    pub(crate) limits: Limits,
    header: Header,
    /// Where each input's numbers start among the uniforms.
    input_at: Vec<usize>,
    defaults: Vec<u32>,
    instructions: usize,
    /// The least work any pixel does (see [`least`]).
    least_work: u64,
}

const TRUE: u32 = u32::MAX;

impl Program {
    /// Reads a shader file. Nothing in `source` is trusted: see [`Limits`].
    pub fn compile(source: &str, limits: &Limits) -> Result<Program, Error> {
        if source.len() > limits.source_bytes {
            return fail(0, "the file is too large to be a shader");
        }
        let limits = *limits;
        // On a thread of its own: with a stack sized for the deepest shader the limits allow,
        // and so that a fault in reading one is an error here, not the end of the program.
        std::thread::scope(|scope| {
            let reader = std::thread::Builder::new()
                .name("shader".into())
                .stack_size(STACK_BYTES)
                .spawn_scoped(scope, move || compile_here(source, &limits));
            match reader {
                Ok(reader) => reader
                    .join()
                    .unwrap_or_else(|_| fail(0, "the shader couldn't be read (a fault in PixelFlow)")),
                Err(_) => fail(0, "no thread to read the shader on"),
            }
        })
    }

    /// What the file says about itself, its inputs first of all.
    pub fn header(&self) -> &Header {
        &self.header
    }

    /// Instructions in the program, every call expanded.
    pub fn instructions(&self) -> usize {
        self.instructions
    }

    /// Numbers a pixel keeps while it runs.
    pub fn slots(&self) -> usize {
        self.slots as usize
    }

    /// The least work any pixel does, whatever the inputs and wherever it is: what is plain
    /// from how the shader is written (a lower bound; most pixels do more).
    pub fn least_work(&self) -> u64 {
        self.least_work
    }

    /// The most work each pixel may do in a frame of `pixels` pixels: [`Limits::work`], or less
    /// where that many pixels doing so much would take the frame past [`Limits::frame_work`].
    pub fn work_for(&self, pixels: usize) -> u32 {
        let each = self.limits.frame_work / (pixels.max(1) as u64);
        each.min(u64::from(self.limits.work)) as u32
    }

    /// The inputs at the values the file gives them, at time 0 on a picture one cell in size.
    pub fn uniforms(&self) -> Uniforms {
        Uniforms {
            bits: self.defaults.clone(),
        }
    }

    /// The time in seconds, the seconds since the frame before, and the frame's number.
    pub fn set_time(&self, uniforms: &mut Uniforms, seconds: f32, delta: f32, frame: i32) {
        uniforms.bits[TIME] = seconds.to_bits();
        uniforms.bits[TIME_DELTA] = delta.to_bits();
        uniforms.bits[FRAME_INDEX] = frame as u32;
    }

    /// The size of the picture in cells.
    pub fn set_size(&self, uniforms: &mut Uniforms, columns: f32, rows: f32) {
        uniforms.bits[SIZE] = columns.max(1.0).to_bits();
        uniforms.bits[SIZE + 1] = rows.max(1.0).to_bits();
    }

    /// Gives input `name` a value: one number for a float, a choice (the number the shader is
    /// given), or a switch (0 is off); two for a point; four for a color. Says whether there is
    /// such an input. A number that isn't one, or is outside the input's range, is brought
    /// inside it.
    pub fn set_input(&self, uniforms: &mut Uniforms, name: &str, value: &[f32]) -> bool {
        let Some(index) = self.header.inputs.iter().position(|i| i.name == name) else {
            return false;
        };
        let at = self.input_at[index];
        let number = |k: usize, lo: f32, hi: f32, fallback: f32| {
            let v = value
                .get(k)
                .copied()
                .filter(|v| v.is_finite())
                .unwrap_or(fallback);
            v.clamp(lo.min(hi), hi.max(lo))
        };
        match &self.header.inputs[index].kind {
            InputKind::Float { min, max, default } => {
                uniforms.bits[at] = number(0, *min, *max, *default).to_bits();
            }
            InputKind::Bool { default } => {
                let on = value.first().map_or(*default, |v| *v != 0.0);
                uniforms.bits[at] = if on { TRUE } else { 0 };
            }
            InputKind::Event => {
                uniforms.bits[at] = if value.first().is_some_and(|v| *v != 0.0) {
                    TRUE
                } else {
                    0
                }
            }
            InputKind::Long { values, default, .. } => {
                let wanted = number(0, i32::MIN as f32, i32::MAX as f32, *default as f32) as i32;
                let given = if values.is_empty() || values.contains(&wanted) {
                    wanted
                } else {
                    *default
                };
                uniforms.bits[at] = given as u32;
            }
            InputKind::Point { min, max, default } => {
                for k in 0..2 {
                    uniforms.bits[at + k] = number(k, min[k], max[k], default[k]).to_bits();
                }
            }
            InputKind::Color { default } => {
                for (k, fallback) in default.iter().enumerate() {
                    uniforms.bits[at + k] = number(k, 0.0, 1.0, *fallback).to_bits();
                }
            }
            InputKind::Image | InputKind::Audio | InputKind::AudioFft => return false,
        }
        true
    }
}

fn compile_here(source: &str, limits: &Limits) -> Res<Program> {
    let header = crate::isf::header(source)?;
    if let Some(need) = header.unsupported() {
        return fail(0, need);
    }
    let tokens = crate::lex::lex(source, limits)?;
    let tokens = crate::lex::preprocess(&tokens, limits)?;
    let unit = crate::parse::parse(&tokens, limits)?;
    Lower::new(&unit, limits).program(header)
}

/// A variable in scope.
#[derive(Debug, Clone)]
struct Var {
    ty: Ty,
    slot: Slot,
    /// The level it was declared at.
    lvl: u16,
    /// Its length, for an array.
    len: Option<u32>,
    /// Its value, for a whole number that is constant.
    constant: Option<i32>,
    /// Given from outside, or declared `const`: not to be assigned.
    fixed: bool,
}

/// A value while the program is written: its type and the slot of each of its numbers.
#[derive(Debug, Clone)]
struct Val {
    ty: Ty,
    slots: Vec<Slot>,
    /// For the numbers of a variable (not a working value): the level it was declared at.
    of: Option<u16>,
}

/// Somewhere a value can be stored.
#[derive(Debug, Clone)]
enum Place {
    Slots {
        ty: Ty,
        slots: Vec<Slot>,
        lvl: u16,
    },
    /// Numbers `offs` of the element `idx` picks, of `len` elements `stride` numbers each from
    /// `base`.
    Element {
        ty: Ty,
        base: Slot,
        stride: u32,
        len: u32,
        idx: Slot,
        offs: Vec<u32>,
        lvl: u16,
    },
}

impl Place {
    fn ty(&self) -> Ty {
        match self {
            Place::Slots { ty, .. } | Place::Element { ty, .. } => *ty,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Frame {
    lvl: u16,
    ret: Ty,
    /// Where the call's answer goes.
    slot: Slot,
}

#[derive(Debug, Clone, Copy)]
struct InLoop {
    live: u16,
    body: u16,
}

struct Lower<'a> {
    unit: &'a Unit,
    limits: &'a Limits,
    /// The block being written.
    cur: Vec<Code>,
    scopes: Vec<HashMap<String, Var>>,
    next: Slot,
    most: Slot,
    constants: Vec<u32>,
    constant_at: HashMap<u32, Slot>,
    lvl: u16,
    deepest: u16,
    written: usize,
    walk: u32,
    calls: Vec<usize>,
    frames: Vec<Frame>,
    loops: Vec<InLoop>,
}

fn vector(base: Base, n: usize) -> Ty {
    if n == 1 {
        Ty::Scalar(base)
    } else {
        Ty::Vector(base, n as u8)
    }
}

/// Which numbers of a vector `.field` names (`xyzw`, `rgba`, or `stpq`, not mixed).
fn swizzle(field: &str, of: usize) -> Option<Vec<usize>> {
    if field.is_empty() || field.len() > 4 {
        return None;
    }
    for set in ["xyzw", "rgba", "stpq"] {
        let picked: Option<Vec<usize>> = field.chars().map(|c| set.find(c)).collect();
        if let Some(picked) = picked {
            return picked.iter().all(|&k| k < of).then_some(picked);
        }
    }
    None
}

/// Whether working `e` out changes anything (so it can't be worked out and then not used).
fn has_effects(e: &Node, unit: &Unit) -> bool {
    match &e.e {
        Expr::Int(_) | Expr::Float(_) | Expr::Bool(_) | Expr::Var(_) => false,
        Expr::Assign(..) | Expr::Step { .. } => true,
        Expr::Call(name, args) => {
            unit.functions.iter().any(|f| &f.name == name) || args.iter().any(|a| has_effects(a, unit))
        }
        Expr::Field(a, _) | Expr::Unary(_, a) => has_effects(a, unit),
        Expr::Index(a, b) | Expr::Binary(_, a, b) | Expr::Comma(a, b) => {
            has_effects(a, unit) || has_effects(b, unit)
        }
        Expr::Ternary(a, b, c) => has_effects(a, unit) || has_effects(b, unit) || has_effects(c, unit),
    }
}

impl<'a> Lower<'a> {
    fn new(unit: &'a Unit, limits: &'a Limits) -> Self {
        Self {
            unit,
            limits,
            cur: Vec::new(),
            scopes: vec![HashMap::new()],
            next: 0,
            most: 0,
            constants: Vec::new(),
            constant_at: HashMap::new(),
            lvl: 0,
            deepest: 0,
            written: 0,
            walk: 0,
            calls: Vec::new(),
            frames: Vec::new(),
            loops: Vec::new(),
        }
    }

    // ---- slots and instructions ------------------------------------------------------------

    fn alloc(&mut self, n: usize) -> Res<Slot> {
        let first = self.next;
        let end = first as usize + n;
        if end > self.limits.slots {
            return fail(0, "the shader keeps too many values at once");
        }
        self.next = end as Slot;
        self.most = self.most.max(self.next);
        Ok(first)
    }

    fn constant(&mut self, bits: u32) -> Slot {
        if let Some(&slot) = self.constant_at.get(&bits) {
            return slot;
        }
        let slot = CONSTANTS + self.constants.len() as Slot;
        self.constants.push(bits);
        self.constant_at.insert(bits, slot);
        slot
    }

    fn float(&mut self, v: f32) -> Slot {
        self.constant(v.to_bits())
    }

    fn zero(&mut self, base: Base) -> Slot {
        match base {
            Base::Float => self.float(0.0),
            Base::Int | Base::Bool => self.constant(0),
        }
    }

    fn one(&mut self, base: Base) -> Slot {
        match base {
            Base::Float => self.float(1.0),
            Base::Int => self.constant(1),
            Base::Bool => self.constant(TRUE),
        }
    }

    fn push(&mut self, code: Code) -> Res<()> {
        self.written += 1;
        if self.written > self.limits.instructions {
            return fail(
                0,
                "the shader is too large once its calls and loops are written out",
            );
        }
        self.cur.push(code);
        Ok(())
    }

    fn op3(&mut self, op: Op, a: Slot, b: Slot, c: Slot) -> Res<Slot> {
        let d = self.alloc(1)?;
        self.push(Code::Ins(Ins { op, d, a, b, c }))?;
        Ok(d)
    }

    fn op2(&mut self, op: Op, a: Slot, b: Slot) -> Res<Slot> {
        self.op3(op, a, b, a)
    }

    fn op1(&mut self, op: Op, a: Slot) -> Res<Slot> {
        self.op3(op, a, a, a)
    }

    fn store(&mut self, dst: Slot, src: Slot, lvl: u16) -> Res<()> {
        self.push(Code::Store { dst, src, lvl })
    }

    /// The block written so far, leaving an empty one to write.
    fn take_block(&mut self) -> Block {
        let nodes = std::mem::take(&mut self.cur);
        let cost = nodes
            .iter()
            .fold(0u32, |cost, node| cost.saturating_add(node.weight()));
        Block { nodes, cost }
    }

    fn deeper(&mut self, by: u16, line: u32) -> Res<()> {
        if self.lvl + by > MAX_LEVEL {
            return fail(line, "ifs, loops, and calls nest too deeply");
        }
        self.deepest = self.deepest.max(self.lvl + by);
        Ok(())
    }

    fn enter_walk(&mut self, line: u32) -> Res<()> {
        self.walk += 1;
        if self.walk > MAX_WALK {
            return fail(line, "the shader nests too deeply");
        }
        Ok(())
    }

    // ---- names -----------------------------------------------------------------------------

    fn find(&self, name: &str) -> Option<&Var> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    fn declare(&mut self, name: &str, var: Var) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), var);
        }
    }

    /// The value of a whole-number expression that is constant, if `e` is one.
    fn const_int(&self, e: &Node) -> Option<i32> {
        match &e.e {
            Expr::Int(n) => Some(*n),
            Expr::Var(name) => self.find(name)?.constant,
            Expr::Unary(UnOp::Neg, a) => Some(self.const_int(a)?.wrapping_neg()),
            Expr::Binary(op, a, b) => {
                let (a, b) = (self.const_int(a)?, self.const_int(b)?);
                match op {
                    BinOp::Add => Some(a.wrapping_add(b)),
                    BinOp::Sub => Some(a.wrapping_sub(b)),
                    BinOp::Mul => Some(a.wrapping_mul(b)),
                    BinOp::Div if b != 0 => Some(a.wrapping_div(b)),
                    BinOp::Rem if b != 0 => Some(a.wrapping_rem(b)),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    // ---- conversions -----------------------------------------------------------------------

    fn convert_slot(&mut self, slot: Slot, from: Base, to: Base) -> Res<Slot> {
        let op = match (from, to) {
            (Base::Int, Base::Float) => Op::IntToFloat,
            (Base::Float, Base::Int) => Op::FloatToInt,
            (Base::Bool, Base::Float) => Op::BoolToFloat,
            (Base::Bool, Base::Int) => Op::BoolToInt,
            (Base::Float, Base::Bool) => Op::FloatToBool,
            (Base::Int, Base::Bool) => Op::IntToBool,
            _ => return Ok(slot),
        };
        self.op1(op, slot)
    }

    /// `v` with every number turned into a `to`.
    fn convert(&mut self, v: &Val, to: Base) -> Res<Val> {
        let from = v.ty.base();
        if from == to {
            return Ok(v.clone());
        }
        let mut slots = Vec::with_capacity(v.slots.len());
        for &s in &v.slots {
            slots.push(self.convert_slot(s, from, to)?);
        }
        let ty = match v.ty {
            Ty::Scalar(_) => Ty::Scalar(to),
            Ty::Vector(_, n) => Ty::Vector(to, n),
            other => other,
        };
        Ok(Val { ty, slots, of: None })
    }

    /// `v` as a `want`, by the conversions GLSL makes unasked (a whole number to a float).
    fn fit(&mut self, v: Val, want: Ty, line: u32) -> Res<Val> {
        if v.ty == want {
            return Ok(v);
        }
        let same_shape = match (v.ty, want) {
            (Ty::Scalar(_), Ty::Scalar(_)) => true,
            (Ty::Vector(_, a), Ty::Vector(_, b)) => a == b,
            _ => false,
        };
        if same_shape && v.ty.base() == Base::Int && want.base() == Base::Float {
            return self.convert(&v, Base::Float);
        }
        fail(
            line,
            format!("a {} is needed here, not a {}", want.name(), v.ty.name()),
        )
    }

    /// A working copy of `v`, if its numbers are a variable's.
    fn settled(&mut self, v: Val) -> Res<Val> {
        if v.of.is_none() {
            return Ok(v);
        }
        let first = self.alloc(v.slots.len())?;
        for (k, &s) in v.slots.iter().enumerate() {
            self.push(Code::Ins(Ins {
                op: Op::Copy,
                d: first + k as Slot,
                a: s,
                b: s,
                c: s,
            }))?;
        }
        Ok(Val {
            ty: v.ty,
            slots: (first..first + v.slots.len() as Slot).collect(),
            of: None,
        })
    }

    // ---- places ----------------------------------------------------------------------------

    fn place(&mut self, e: &Node) -> Res<Place> {
        self.enter_walk(e.line)?;
        let place = match &e.e {
            Expr::Var(name) => {
                let Some(var) = self.find(name).cloned() else {
                    return fail(e.line, format!("there is no `{name}`"));
                };
                if var.fixed {
                    return fail(e.line, format!("`{name}` can't be changed"));
                }
                if var.len.is_some() {
                    return fail(e.line, format!("`{name}` is an array: one element at a time"));
                }
                let n = var.ty.components() as Slot;
                Place::Slots {
                    ty: var.ty,
                    slots: (var.slot..var.slot + n).collect(),
                    lvl: var.lvl,
                }
            }
            Expr::Field(inner, field) => {
                let inner = self.place(inner)?;
                let Ty::Vector(base, n) = inner.ty() else {
                    return fail(e.line, format!("`.{field}` of something that isn't a vector"));
                };
                let Some(picked) = swizzle(field, usize::from(n)) else {
                    return fail(e.line, format!("a {} has no `.{field}`", inner.ty().name()));
                };
                if (1..picked.len()).any(|k| picked[..k].contains(&picked[k])) {
                    return fail(e.line, format!("`.{field}` names a component twice"));
                }
                let ty = vector(base, picked.len());
                match inner {
                    Place::Slots { slots, lvl, .. } => Place::Slots {
                        ty,
                        slots: picked.iter().map(|&k| slots[k]).collect(),
                        lvl,
                    },
                    Place::Element {
                        base,
                        stride,
                        len,
                        idx,
                        offs,
                        lvl,
                        ..
                    } => Place::Element {
                        ty,
                        base,
                        stride,
                        len,
                        idx,
                        offs: picked.iter().map(|&k| offs[k]).collect(),
                        lvl,
                    },
                }
            }
            Expr::Index(inner, index) => {
                // An array's element.
                if let Expr::Var(name) = &inner.e
                    && let Some(var) = self.find(name).cloned()
                    && let Some(len) = var.len
                {
                    if var.fixed {
                        return fail(e.line, format!("`{name}` can't be changed"));
                    }
                    let stride = var.ty.components() as u32;
                    match self.const_int(index) {
                        Some(k) if k >= 0 && (k as u32) < len => Place::Slots {
                            ty: var.ty,
                            slots: (0..stride).map(|j| var.slot + k as u32 * stride + j).collect(),
                            lvl: var.lvl,
                        },
                        Some(_) => return fail(e.line, format!("`{name}` has no such element")),
                        None => {
                            let idx = self.index(index)?;
                            Place::Element {
                                ty: var.ty,
                                base: var.slot,
                                stride,
                                len,
                                idx,
                                offs: (0..stride).collect(),
                                lvl: var.lvl,
                            }
                        }
                    }
                } else {
                    let inner_place = self.place(inner)?;
                    // A vector's number, or a matrix's column.
                    let (each, of, ty) = match inner_place.ty() {
                        Ty::Vector(base, n) => (1usize, usize::from(n), Ty::Scalar(base)),
                        Ty::Matrix(n) => (usize::from(n), usize::from(n), Ty::Vector(Base::Float, n)),
                        other => return fail(e.line, format!("a {} has no elements", other.name())),
                    };
                    match (self.const_int(index), inner_place) {
                        (Some(k), _) if k < 0 || k as usize >= of => {
                            return fail(e.line, "no such element");
                        }
                        (Some(k), Place::Slots { slots, lvl, .. }) => Place::Slots {
                            ty,
                            slots: slots[k as usize * each..(k as usize + 1) * each].to_vec(),
                            lvl,
                        },
                        (
                            Some(k),
                            Place::Element {
                                base,
                                stride,
                                len,
                                idx,
                                offs,
                                lvl,
                                ..
                            },
                        ) => Place::Element {
                            ty,
                            base,
                            stride,
                            len,
                            idx,
                            offs: offs[k as usize * each..(k as usize + 1) * each].to_vec(),
                            lvl,
                        },
                        (None, Place::Slots { slots, lvl, .. }) => {
                            let in_a_row = slots.windows(2).all(|w| w[1] == w[0] + 1);
                            if !in_a_row {
                                return fail(e.line, "an element picked this way can't be stored to");
                            }
                            let idx = self.index(index)?;
                            Place::Element {
                                ty,
                                base: slots[0],
                                stride: each as u32,
                                len: of as u32,
                                idx,
                                offs: (0..each as u32).collect(),
                                lvl,
                            }
                        }
                        (None, Place::Element { .. }) => {
                            return fail(
                                e.line,
                                "an element of an element picked this way can't be stored to",
                            );
                        }
                    }
                }
            }
            _ => return fail(e.line, "this can't be stored to"),
        };
        self.walk -= 1;
        Ok(place)
    }

    /// An index, as a whole number.
    fn index(&mut self, e: &Node) -> Res<Slot> {
        let v = self.expr(e)?;
        match v.ty {
            Ty::Scalar(Base::Int) => Ok(v.slots[0]),
            Ty::Scalar(Base::Float) => self.convert_slot(v.slots[0], Base::Float, Base::Int),
            other => fail(
                e.line,
                format!("an index is a whole number, not a {}", other.name()),
            ),
        }
    }

    fn read(&mut self, place: &Place) -> Res<Val> {
        match place {
            Place::Slots { ty, slots, lvl } => Ok(Val {
                ty: *ty,
                slots: slots.clone(),
                of: Some(*lvl),
            }),
            Place::Element {
                ty,
                base,
                stride,
                len,
                idx,
                offs,
                ..
            } => {
                let first = self.alloc(offs.len())?;
                for (k, &off) in offs.iter().enumerate() {
                    self.push(Code::Load {
                        d: first + k as Slot,
                        base: *base,
                        stride: *stride,
                        len: *len,
                        idx: *idx,
                        off,
                        count: 1,
                    })?;
                }
                Ok(Val {
                    ty: *ty,
                    slots: (first..first + offs.len() as Slot).collect(),
                    of: None,
                })
            }
        }
    }

    /// Stores `value` (already of the place's type) in `place`.
    fn write(&mut self, place: &Place, value: &Val) -> Res<()> {
        match place {
            Place::Slots { slots, lvl, .. } => {
                // A variable's numbers stored over one another (`v.xy = v.yx`) are taken first.
                let value = if value.of.is_some() && slots.len() > 1 {
                    self.settled(value.clone())?
                } else {
                    value.clone()
                };
                for (&dst, &src) in slots.iter().zip(&value.slots) {
                    self.store(dst, src, *lvl)?;
                }
            }
            Place::Element {
                base,
                stride,
                len,
                idx,
                offs,
                lvl,
                ..
            } => {
                let value = self.settled(value.clone())?;
                for (&off, &src) in offs.iter().zip(&value.slots) {
                    self.push(Code::StoreAt {
                        base: *base,
                        stride: *stride,
                        len: *len,
                        idx: *idx,
                        off,
                        src,
                        lvl: *lvl,
                    })?;
                }
            }
        }
        Ok(())
    }

    // ---- expressions -----------------------------------------------------------------------

    fn expr(&mut self, e: &Node) -> Res<Val> {
        self.enter_walk(e.line)?;
        let line = e.line;
        let value = match &e.e {
            Expr::Int(n) => Val {
                ty: Ty::INT,
                slots: vec![self.constant(*n as u32)],
                of: None,
            },
            Expr::Float(f) => Val {
                ty: Ty::FLOAT,
                slots: vec![self.float(*f)],
                of: None,
            },
            Expr::Bool(b) => Val {
                ty: Ty::BOOL,
                slots: vec![self.constant(if *b { TRUE } else { 0 })],
                of: None,
            },
            Expr::Var(name) => {
                let Some(var) = self.find(name).cloned() else {
                    return fail(line, format!("there is no `{name}`"));
                };
                if var.len.is_some() {
                    return fail(line, format!("`{name}` is an array: one element at a time"));
                }
                let n = var.ty.components() as Slot;
                Val {
                    ty: var.ty,
                    slots: (var.slot..var.slot + n).collect(),
                    of: Some(var.lvl),
                }
            }
            Expr::Field(inner, field) => {
                let inner = self.expr(inner)?;
                let picked = match inner.ty {
                    Ty::Vector(_, n) => swizzle(field, usize::from(n)),
                    // `x.x` and `x.xxx` of a single number, as desktop GLSL allows.
                    Ty::Scalar(_) => swizzle(field, 1),
                    _ => None,
                };
                let Some(picked) = picked else {
                    return fail(line, format!("a {} has no `.{field}`", inner.ty.name()));
                };
                Val {
                    ty: vector(inner.ty.base(), picked.len()),
                    slots: picked.iter().map(|&k| inner.slots[k]).collect(),
                    of: inner.of,
                }
            }
            Expr::Index(inner, index) => self.indexed(inner, index, line)?,
            Expr::Unary(op, inner) => {
                let v = self.expr(inner)?;
                let op = match (op, v.ty.base(), v.ty) {
                    (UnOp::Neg, Base::Float, _) => Op::FNeg,
                    (UnOp::Neg, Base::Int, _) => Op::INeg,
                    (UnOp::Not, Base::Bool, Ty::Scalar(_)) | (UnOp::BitNot, Base::Int, _) => Op::INot,
                    _ => return fail(line, format!("this can't be done to a {}", v.ty.name())),
                };
                let mut slots = Vec::with_capacity(v.slots.len());
                for &s in &v.slots {
                    slots.push(self.op1(op, s)?);
                }
                Val {
                    ty: v.ty,
                    slots,
                    of: None,
                }
            }
            Expr::Step { target, up, post } => {
                let place = self.place(target)?;
                let now = self.read(&place)?;
                let base = now.ty.base();
                let op = match (base, up) {
                    (Base::Float, true) => Op::FAdd,
                    (Base::Float, false) => Op::FSub,
                    (Base::Int, true) => Op::IAdd,
                    (Base::Int, false) => Op::ISub,
                    (Base::Bool, _) => return fail(line, "a truth can't be counted up or down"),
                };
                let before = if *post {
                    Some(self.settled(now.clone())?)
                } else {
                    None
                };
                let one = self.one(base);
                let mut slots = Vec::with_capacity(now.slots.len());
                for &s in &now.slots {
                    slots.push(self.op2(op, s, one)?);
                }
                let after = Val {
                    ty: now.ty,
                    slots,
                    of: None,
                };
                self.write(&place, &after)?;
                before.unwrap_or(after)
            }
            Expr::Binary(op @ (BinOp::And | BinOp::Or), left, right) if has_effects(right, self.unit) => {
                // The right side runs only when the left doesn't settle it.
                let l = self.expr(left)?;
                if l.ty != Ty::BOOL {
                    return fail(line, "`&&` and `||` take truths");
                }
                let result = self.alloc(1)?;
                let lvl = self.lvl;
                self.store(result, l.slots[0], lvl)?;
                let cond = match op {
                    BinOp::And => result,
                    _ => self.op1(Op::INot, result)?,
                };
                self.deeper(1, line)?;
                let outer = std::mem::take(&mut self.cur);
                self.lvl += 1;
                let r = self.expr(right)?;
                if r.ty != Ty::BOOL {
                    return fail(line, "`&&` and `||` take truths");
                }
                self.store(result, r.slots[0], lvl)?;
                self.lvl -= 1;
                let then = self.take_block();
                self.cur = outer;
                self.push(Code::If {
                    cond,
                    then,
                    otherwise: Block::default(),
                })?;
                Val {
                    ty: Ty::BOOL,
                    slots: vec![result],
                    of: None,
                }
            }
            Expr::Binary(op, left, right) => {
                let l = self.expr(left)?;
                let r = self.expr(right)?;
                self.binary(*op, l, r, line)?
            }
            Expr::Assign(op, target, value) => {
                let place = self.place(target)?;
                let value = self.expr(value)?;
                let value = match op {
                    None => self.fit(value, place.ty(), line)?,
                    Some(op) => {
                        let now = self.read(&place)?;
                        let combined = self.binary(*op, now, value, line)?;
                        self.fit(combined, place.ty(), line)?
                    }
                };
                self.write(&place, &value)?;
                value
            }
            Expr::Ternary(cond, yes, no) => self.ternary(cond, yes, no, line)?,
            Expr::Comma(first, second) => {
                self.expr(first)?;
                self.expr(second)?
            }
            Expr::Call(name, args) => self.call(name, args, line)?,
        };
        self.walk -= 1;
        Ok(value)
    }

    fn indexed(&mut self, inner: &Node, index: &Node, line: u32) -> Res<Val> {
        // An array's element.
        if let Expr::Var(name) = &inner.e
            && let Some(var) = self.find(name).cloned()
            && let Some(len) = var.len
        {
            let stride = var.ty.components() as u32;
            return match self.const_int(index) {
                Some(k) if k >= 0 && (k as u32) < len => Ok(Val {
                    ty: var.ty,
                    slots: (0..stride).map(|j| var.slot + k as u32 * stride + j).collect(),
                    of: Some(var.lvl),
                }),
                Some(_) => fail(line, format!("`{name}` has no such element")),
                None => {
                    let idx = self.index(index)?;
                    let first = self.alloc(stride as usize)?;
                    self.push(Code::Load {
                        d: first,
                        base: var.slot,
                        stride,
                        len,
                        idx,
                        off: 0,
                        count: stride,
                    })?;
                    Ok(Val {
                        ty: var.ty,
                        slots: (first..first + stride).collect(),
                        of: None,
                    })
                }
            };
        }
        let v = self.expr(inner)?;
        let (each, of, ty) = match v.ty {
            Ty::Vector(base, n) => (1usize, usize::from(n), Ty::Scalar(base)),
            Ty::Matrix(n) => (usize::from(n), usize::from(n), Ty::Vector(Base::Float, n)),
            other => return fail(line, format!("a {} has no elements", other.name())),
        };
        if let Some(k) = self.const_int(index) {
            if k < 0 || k as usize >= of {
                return fail(line, "no such element");
            }
            let k = k as usize;
            return Ok(Val {
                ty,
                slots: v.slots[k * each..(k + 1) * each].to_vec(),
                of: v.of,
            });
        }
        // Picked as the program runs: from numbers in a row.
        let in_a_row = v.slots.windows(2).all(|w| w[1] == w[0] + 1) && v.slots[0] < CONSTANTS;
        let v = if in_a_row {
            v
        } else {
            self.settled(Val { of: Some(0), ..v })?
        };
        let idx = self.index(index)?;
        let first = self.alloc(each)?;
        self.push(Code::Load {
            d: first,
            base: v.slots[0],
            stride: each as u32,
            len: of as u32,
            idx,
            off: 0,
            count: each as u32,
        })?;
        Ok(Val {
            ty,
            slots: (first..first + each as Slot).collect(),
            of: None,
        })
    }

    fn ternary(&mut self, cond: &Node, yes: &Node, no: &Node, line: u32) -> Res<Val> {
        let c = self.expr(cond)?;
        if c.ty != Ty::BOOL {
            return fail(line, "`?` asks a truth");
        }
        if !has_effects(yes, self.unit) && !has_effects(no, self.unit) {
            // Both sides worked out, and one picked.
            let y = self.expr(yes)?;
            let n = self.expr(no)?;
            let (y, n) = self.alike(y, n, line)?;
            let mut slots = Vec::with_capacity(y.slots.len());
            for (&a, &b) in y.slots.iter().zip(&n.slots) {
                slots.push(self.op3(Op::Select, c.slots[0], a, b)?);
            }
            return Ok(Val {
                ty: y.ty,
                slots,
                of: None,
            });
        }
        // Only the side picked runs.
        let cond = self.settled(c)?.slots[0];
        let lvl = self.lvl;
        self.deeper(1, line)?;
        let outer = std::mem::take(&mut self.cur);
        self.lvl += 1;
        let y = self.expr(yes)?;
        let result = self.alloc(y.ty.components())?;
        let slots: Vec<Slot> = (result..result + y.ty.components() as Slot).collect();
        for (&dst, &src) in slots.iter().zip(&y.slots) {
            self.store(dst, src, lvl)?;
        }
        let then = self.take_block();
        let n = self.expr(no)?;
        let n = self.fit(n, y.ty, line)?;
        for (&dst, &src) in slots.iter().zip(&n.slots) {
            self.store(dst, src, lvl)?;
        }
        let otherwise = self.take_block();
        self.lvl -= 1;
        self.cur = outer;
        self.push(Code::If {
            cond,
            then,
            otherwise,
        })?;
        Ok(Val {
            ty: y.ty,
            slots,
            of: None,
        })
    }

    /// Two values brought to one type (a whole number beside a float becomes a float).
    fn alike(&mut self, a: Val, b: Val, line: u32) -> Res<(Val, Val)> {
        if a.ty == b.ty {
            return Ok((a, b));
        }
        if a.ty.base() == Base::Float {
            let b = self.fit(b, a.ty, line)?;
            Ok((a, b))
        } else {
            let a = self.fit(a, b.ty, line)?;
            Ok((a, b))
        }
    }

    /// `op` on each pair of numbers, a single number going with every number of a vector.
    fn each2(&mut self, op: Op, a: &Val, b: &Val, line: u32) -> Res<Vec<Slot>> {
        let n = a.slots.len().max(b.slots.len());
        if (a.slots.len() != n && a.slots.len() != 1) || (b.slots.len() != n && b.slots.len() != 1) {
            return fail(
                line,
                format!("a {} and a {} don't go together", a.ty.name(), b.ty.name()),
            );
        }
        let mut slots = Vec::with_capacity(n);
        for k in 0..n {
            let x = a.slots[if a.slots.len() == 1 { 0 } else { k }];
            let y = b.slots[if b.slots.len() == 1 { 0 } else { k }];
            slots.push(self.op2(op, x, y)?);
        }
        Ok(slots)
    }

    /// The sum of `terms` (at least one), added in order.
    fn sum(&mut self, terms: &[Slot]) -> Res<Slot> {
        let mut total = terms[0];
        for &t in &terms[1..] {
            total = self.op2(Op::FAdd, total, t)?;
        }
        Ok(total)
    }

    fn dot(&mut self, a: &[Slot], b: &[Slot]) -> Res<Slot> {
        let mut terms = Vec::with_capacity(a.len());
        for (&x, &y) in a.iter().zip(b) {
            terms.push(self.op2(Op::FMul, x, y)?);
        }
        self.sum(&terms)
    }

    fn binary(&mut self, op: BinOp, l: Val, r: Val, line: u32) -> Res<Val> {
        let mismatch = |l: &Val, r: &Val| -> Error {
            Error::new(
                line,
                format!("a {} and a {} don't go together here", l.ty.name(), r.ty.name()),
            )
        };
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                // Matrices multiply as matrices.
                if op == BinOp::Mul {
                    match (l.ty, r.ty) {
                        (Ty::Matrix(n), Ty::Matrix(m)) if n == m => {
                            let n = usize::from(n);
                            let mut slots = Vec::with_capacity(n * n);
                            for c in 0..n {
                                for row in 0..n {
                                    let left: Vec<Slot> = (0..n).map(|k| l.slots[k * n + row]).collect();
                                    slots.push(self.dot(&left, &r.slots[c * n..(c + 1) * n])?);
                                }
                            }
                            return Ok(Val {
                                ty: l.ty,
                                slots,
                                of: None,
                            });
                        }
                        (Ty::Matrix(n), Ty::Vector(Base::Float, m)) if n == m => {
                            let n = usize::from(n);
                            let mut slots = Vec::with_capacity(n);
                            for row in 0..n {
                                let left: Vec<Slot> = (0..n).map(|c| l.slots[c * n + row]).collect();
                                slots.push(self.dot(&left, &r.slots)?);
                            }
                            return Ok(Val {
                                ty: r.ty,
                                slots,
                                of: None,
                            });
                        }
                        (Ty::Vector(Base::Float, m), Ty::Matrix(n)) if n == m => {
                            let n = usize::from(n);
                            let mut slots = Vec::with_capacity(n);
                            for c in 0..n {
                                slots.push(self.dot(&l.slots, &r.slots[c * n..(c + 1) * n])?);
                            }
                            return Ok(Val {
                                ty: l.ty,
                                slots,
                                of: None,
                            });
                        }
                        _ => {}
                    }
                }
                if l.ty.base() == Base::Bool || r.ty.base() == Base::Bool {
                    return Err(mismatch(&l, &r));
                }
                let float = l.ty.base() == Base::Float || r.ty.base() == Base::Float;
                let base = if float { Base::Float } else { Base::Int };
                let (l, r) = (self.convert(&l, base)?, self.convert(&r, base)?);
                let ty = match (l.ty, r.ty) {
                    (a, b) if a == b => a,
                    (a, Ty::Scalar(_)) => a,
                    (Ty::Scalar(_), b) => b,
                    _ => return Err(mismatch(&l, &r)),
                };
                let op = match (op, base) {
                    (BinOp::Add, Base::Float) => Op::FAdd,
                    (BinOp::Sub, Base::Float) => Op::FSub,
                    (BinOp::Mul, Base::Float) => Op::FMul,
                    (BinOp::Div, Base::Float) => Op::FDiv,
                    (BinOp::Add, _) => Op::IAdd,
                    (BinOp::Sub, _) => Op::ISub,
                    (BinOp::Mul, _) => Op::IMul,
                    _ => Op::IDiv,
                };
                Ok(Val {
                    ty,
                    slots: self.each2(op, &l, &r, line)?,
                    of: None,
                })
            }
            BinOp::Rem | BinOp::Shl | BinOp::Shr | BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => {
                if l.ty.base() != Base::Int || r.ty.base() != Base::Int {
                    return Err(mismatch(&l, &r));
                }
                let ty = match (l.ty, r.ty) {
                    (a, b) if a == b => a,
                    (a, Ty::Scalar(_)) => a,
                    (Ty::Scalar(_), b) => b,
                    _ => return Err(mismatch(&l, &r)),
                };
                let op = match op {
                    BinOp::Rem => Op::IMod,
                    BinOp::Shl => Op::IShl,
                    BinOp::Shr => Op::IShr,
                    BinOp::BitAnd => Op::IAnd,
                    BinOp::BitOr => Op::IOr,
                    _ => Op::IXor,
                };
                Ok(Val {
                    ty,
                    slots: self.each2(op, &l, &r, line)?,
                    of: None,
                })
            }
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                let scalars = matches!((l.ty, r.ty), (Ty::Scalar(a), Ty::Scalar(b)) if a != Base::Bool && b != Base::Bool);
                if !scalars {
                    return Err(mismatch(&l, &r));
                }
                let float = l.ty.base() == Base::Float || r.ty.base() == Base::Float;
                let base = if float { Base::Float } else { Base::Int };
                let (l, r) = (self.convert(&l, base)?, self.convert(&r, base)?);
                let op = match (op, float) {
                    (BinOp::Lt, true) => Op::FLt,
                    (BinOp::Le, true) => Op::FLe,
                    (BinOp::Gt, true) => Op::FGt,
                    (BinOp::Ge, true) => Op::FGe,
                    (BinOp::Lt, false) => Op::ILt,
                    (BinOp::Le, false) => Op::ILe,
                    (BinOp::Gt, false) => Op::IGt,
                    _ => Op::IGe,
                };
                Ok(Val {
                    ty: Ty::BOOL,
                    slots: vec![self.op2(op, l.slots[0], r.slots[0])?],
                    of: None,
                })
            }
            BinOp::Eq | BinOp::Ne => {
                let (l, r) = self.alike(l, r, line)?;
                let float = l.ty.base() == Base::Float;
                let each = match (op, float) {
                    (BinOp::Eq, true) => Op::FEq,
                    (BinOp::Eq, false) => Op::IEq,
                    (_, true) => Op::FNe,
                    _ => Op::INe,
                };
                // Equal when every number is; unequal when any isn't.
                let join = if op == BinOp::Eq { Op::IAnd } else { Op::IOr };
                let mut all = self.op2(each, l.slots[0], r.slots[0])?;
                for (&a, &b) in l.slots.iter().zip(&r.slots).skip(1) {
                    let one = self.op2(each, a, b)?;
                    all = self.op2(join, all, one)?;
                }
                Ok(Val {
                    ty: Ty::BOOL,
                    slots: vec![all],
                    of: None,
                })
            }
            BinOp::And | BinOp::Or | BinOp::Xor => {
                if l.ty != Ty::BOOL || r.ty != Ty::BOOL {
                    return fail(line, "`&&`, `||`, and `^^` take truths");
                }
                let op = match op {
                    BinOp::And => Op::IAnd,
                    BinOp::Or => Op::IOr,
                    _ => Op::IXor,
                };
                Ok(Val {
                    ty: Ty::BOOL,
                    slots: vec![self.op2(op, l.slots[0], r.slots[0])?],
                    of: None,
                })
            }
        }
    }

    // ---- calls -----------------------------------------------------------------------------

    fn call(&mut self, name: &str, args: &[Node], line: u32) -> Res<Val> {
        if let Some(ty) = Ty::named(name) {
            let mut values = Vec::with_capacity(args.len());
            for arg in args {
                values.push(self.expr(arg)?);
            }
            return self.construct(ty, &values, line);
        }
        let unit = self.unit;
        let named: Vec<usize> = (0..unit.functions.len())
            .filter(|&k| unit.functions[k].name == name && unit.functions[k].params.len() == args.len())
            .collect();
        if !named.is_empty() {
            return self.call_user(&named, args, line);
        }
        if unit.functions.iter().any(|f| f.name == name) {
            return fail(line, format!("`{name}` doesn't take {} arguments", args.len()));
        }
        match name {
            "texture2D"
            | "texture"
            | "texture2DRect"
            | "IMG_PIXEL"
            | "IMG_NORM_PIXEL"
            | "IMG_THIS_PIXEL"
            | "IMG_THIS_NORM_PIXEL"
            | "IMG_SIZE" => {
                return fail(line, format!("`{name}`: pictures aren't supported yet"));
            }
            "dFdx" | "dFdy" | "fwidth" => {
                return fail(
                    line,
                    format!(
                        "`{name}` asks how a value changes from one pixel to the next, which isn't supported yet"
                    ),
                );
            }
            _ => {}
        }
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            values.push(self.expr(arg)?);
        }
        self.built_in(name, values, line)
    }

    fn construct(&mut self, ty: Ty, args: &[Val], line: u32) -> Res<Val> {
        if ty == Ty::Void || args.is_empty() {
            return fail(line, format!("{}() needs something to be made from", ty.name()));
        }
        let base = ty.base();
        let want = ty.components();
        // A matrix from one number is that number down its diagonal; from a matrix, its corner.
        if let Ty::Matrix(n) = ty {
            let n = usize::from(n);
            if args.len() == 1 {
                let zero = self.float(0.0);
                let one = self.float(1.0);
                match args[0].ty {
                    Ty::Scalar(_) => {
                        let v = self.convert(&args[0], Base::Float)?.slots[0];
                        let slots = (0..n * n)
                            .map(|k| if k % (n + 1) == 0 { v } else { zero })
                            .collect();
                        return Ok(Val { ty, slots, of: None });
                    }
                    Ty::Matrix(m) => {
                        let m = usize::from(m);
                        let slots = (0..n * n)
                            .map(|k| {
                                let (c, r) = (k / n, k % n);
                                if c < m && r < m {
                                    args[0].slots[c * m + r]
                                } else if c == r {
                                    one
                                } else {
                                    zero
                                }
                            })
                            .collect();
                        return Ok(Val { ty, slots, of: None });
                    }
                    _ => {}
                }
            }
        }
        let mut slots = Vec::with_capacity(want);
        for arg in args {
            if slots.len() >= want {
                return fail(line, format!("too much to make a {} from", ty.name()));
            }
            let arg = self.convert(arg, base)?;
            slots.extend(arg.slots.iter().copied());
        }
        // One number fills a vector.
        if slots.len() == 1 && args[0].ty.components() == 1 {
            slots = vec![slots[0]; want];
        }
        if slots.len() < want {
            return fail(line, format!("too little to make a {} from", ty.name()));
        }
        slots.truncate(want);
        Ok(Val { ty, slots, of: None })
    }

    /// Expands a call to one of the functions `named` (all of that name and argument count).
    fn call_user(&mut self, named: &[usize], args: &[Node], line: u32) -> Res<Val> {
        let unit = self.unit;
        // What the arguments are, to tell functions of one name apart. Those passed out are
        // places, not values.
        let mut given: Vec<Option<Val>> = Vec::with_capacity(args.len());
        let any_out = |k: usize| {
            named
                .iter()
                .any(|&f| unit.functions[f].params[k].passing != Passing::In)
        };
        for (k, arg) in args.iter().enumerate() {
            given.push(if any_out(k) { None } else { Some(self.expr(arg)?) });
        }
        let fits = |f: &Function, exact: bool| {
            f.params.iter().zip(&given).all(|(p, g)| match g {
                None => true,
                Some(v) if v.ty == p.ty => true,
                Some(v) => {
                    !exact
                        && v.ty.base() == Base::Int
                        && p.ty.base() == Base::Float
                        && v.ty.components() == p.ty.components()
                        && !matches!(p.ty, Ty::Matrix(_))
                }
            })
        };
        let chosen = named
            .iter()
            .copied()
            .find(|&f| fits(&unit.functions[f], true))
            .or_else(|| named.iter().copied().find(|&f| fits(&unit.functions[f], false)));
        let Some(index) = chosen else {
            return fail(
                line,
                format!(
                    "`{}` doesn't take arguments of these types",
                    unit.functions[named[0]].name
                ),
            );
        };
        let f: &Function = &unit.functions[index];
        if self.calls.contains(&index) {
            return fail(
                line,
                format!("`{}` calls itself, which GLSL doesn't allow", f.name),
            );
        }
        if self.calls.len() >= MAX_CALLS {
            return fail(line, "calls nest too deeply");
        }
        self.enter_walk(line)?;
        self.deeper(1, line)?;
        let lvl = self.lvl + 1;
        // Arguments in, and the places arguments come back out to.
        let mut ins: Vec<Option<Val>> = Vec::with_capacity(args.len());
        let mut outs: Vec<(usize, Place)> = Vec::new();
        for (k, (param, arg)) in f.params.iter().zip(args).enumerate() {
            match param.passing {
                Passing::In => {
                    let v = match given[k].take() {
                        Some(v) => v,
                        None => self.expr(arg)?,
                    };
                    ins.push(Some(self.fit(v, param.ty, line)?));
                }
                Passing::Out | Passing::InOut => {
                    let place = self.place(arg)?;
                    if place.ty() != param.ty {
                        return fail(line, format!("argument {} must be a {}", k + 1, param.ty.name()));
                    }
                    ins.push(if param.passing == Passing::InOut {
                        Some(self.read(&place)?)
                    } else {
                        None
                    });
                    outs.push((k, place));
                }
            }
        }
        let result = self.alloc(f.ret.components())?;
        let mut params = Vec::with_capacity(f.params.len());
        for param in &f.params {
            params.push(self.alloc(param.ty.components())?);
        }
        // The function sees the globals and its own names, not the caller's.
        let outer_code = std::mem::take(&mut self.cur);
        let outer_loops = std::mem::take(&mut self.loops);
        let outer_scopes = self.scopes.split_off(1);
        self.scopes.push(HashMap::new());
        self.lvl = lvl;
        self.frames.push(Frame {
            lvl,
            ret: f.ret,
            slot: result,
        });
        self.calls.push(index);
        let written = (|| -> Res<()> {
            // The answer, and what is passed back out, are read by the caller once the call is
            // over: by every pixel that made the call, those that returned early among them. So
            // they count as the caller's (see `Machine::store`). What is only passed in is the
            // call's own.
            let zero = self.zero(f.ret.base());
            for k in 0..f.ret.components() as Slot {
                self.store(result + k, zero, lvl - 1)?;
            }
            for ((param, &slot), value) in f.params.iter().zip(&params).zip(&ins) {
                let seen_at = if param.passing == Passing::In {
                    lvl
                } else {
                    lvl - 1
                };
                let zero = self.zero(param.ty.base());
                for k in 0..param.ty.components() {
                    let src = value.as_ref().map_or(zero, |v| v.slots[k]);
                    self.store(slot + k as Slot, src, seen_at)?;
                }
                if !param.name.is_empty() {
                    self.declare(
                        &param.name,
                        Var {
                            ty: param.ty,
                            slot,
                            lvl: seen_at,
                            len: None,
                            constant: None,
                            fixed: false,
                        },
                    );
                }
            }
            for stmt in &f.body {
                self.stmt(stmt)?;
            }
            Ok(())
        })();
        self.calls.pop();
        self.frames.pop();
        self.lvl = lvl - 1;
        self.scopes.truncate(1);
        self.scopes.extend(outer_scopes);
        self.loops = outer_loops;
        let body = self.take_block();
        self.cur = outer_code;
        written?;
        self.push(Code::Call(body))?;
        for (k, place) in &outs {
            let n = f.params[*k].ty.components() as Slot;
            let value = Val {
                ty: f.params[*k].ty,
                slots: (params[*k]..params[*k] + n).collect(),
                of: None,
            };
            self.write(place, &value)?;
        }
        self.walk -= 1;
        Ok(Val {
            ty: f.ret,
            slots: (result..result + f.ret.components() as Slot).collect(),
            of: None,
        })
    }

    /// `op` on each number of `a`, as floats.
    fn map1(&mut self, op: Op, a: &Val) -> Res<Val> {
        let a = self.convert(a, Base::Float)?;
        let mut slots = Vec::with_capacity(a.slots.len());
        for &s in &a.slots {
            slots.push(self.op1(op, s)?);
        }
        Ok(Val {
            ty: a.ty,
            slots,
            of: None,
        })
    }

    /// `op` on each pair of numbers, as floats; a single number goes with every one of a vector.
    fn map2(&mut self, op: Op, a: &Val, b: &Val, line: u32) -> Res<Val> {
        let (a, b) = (self.convert(a, Base::Float)?, self.convert(b, Base::Float)?);
        let ty = if a.slots.len() >= b.slots.len() {
            a.ty
        } else {
            b.ty
        };
        Ok(Val {
            ty,
            slots: self.each2(op, &a, &b, line)?,
            of: None,
        })
    }

    fn map3(&mut self, op: Op, a: &Val, b: &Val, c: &Val, line: u32) -> Res<Val> {
        let vals = [
            self.convert(a, Base::Float)?,
            self.convert(b, Base::Float)?,
            self.convert(c, Base::Float)?,
        ];
        let n = vals.iter().map(|v| v.slots.len()).max().unwrap_or(1);
        if vals.iter().any(|v| v.slots.len() != n && v.slots.len() != 1) {
            return fail(line, "these don't go together");
        }
        let ty = vals
            .iter()
            .find(|v| v.slots.len() == n)
            .map_or(Ty::FLOAT, |v| v.ty);
        let at = |v: &Val, k: usize| v.slots[if v.slots.len() == 1 { 0 } else { k }];
        let mut slots = Vec::with_capacity(n);
        for k in 0..n {
            slots.push(self.op3(op, at(&vals[0], k), at(&vals[1], k), at(&vals[2], k))?);
        }
        Ok(Val { ty, slots, of: None })
    }

    fn length(&mut self, v: &Val) -> Res<Slot> {
        let v = self.convert(v, Base::Float)?;
        if v.slots.len() == 1 {
            return self.op1(Op::FAbs, v.slots[0]);
        }
        let squared = self.dot(&v.slots, &v.slots)?;
        self.op1(Op::FSqrt, squared)
    }

    fn scalar(slot: Slot) -> Val {
        Val {
            ty: Ty::FLOAT,
            slots: vec![slot],
            of: None,
        }
    }

    fn built_in(&mut self, name: &str, args: Vec<Val>, line: u32) -> Res<Val> {
        let wrong = || -> Error { Error::new(line, format!("`{name}` isn't given what it takes")) };
        let floats = args
            .iter()
            .all(|a| a.ty.base() != Base::Bool && !matches!(a.ty, Ty::Void));
        let ints = args.iter().all(|a| a.ty.base() == Base::Int);
        let one = |op: Op| match (args.len(), floats) {
            (1, true) if !matches!(args[0].ty, Ty::Matrix(_)) => Ok(op),
            _ => Err(wrong()),
        };
        match (name, args.len()) {
            ("sin", _) => self.map1(one(Op::FSin)?, &args[0]),
            ("cos", _) => self.map1(one(Op::FCos)?, &args[0]),
            ("tan", _) => self.map1(one(Op::FTan)?, &args[0]),
            ("asin", _) => self.map1(one(Op::FAsin)?, &args[0]),
            ("acos", _) => self.map1(one(Op::FAcos)?, &args[0]),
            ("sinh", _) => self.map1(one(Op::FSinh)?, &args[0]),
            ("cosh", _) => self.map1(one(Op::FCosh)?, &args[0]),
            ("tanh", _) => self.map1(one(Op::FTanh)?, &args[0]),
            ("exp", _) => self.map1(one(Op::FExp)?, &args[0]),
            ("log", _) => self.map1(one(Op::FLog)?, &args[0]),
            ("exp2", _) => self.map1(one(Op::FExp2)?, &args[0]),
            ("log2", _) => self.map1(one(Op::FLog2)?, &args[0]),
            ("sqrt", _) => self.map1(one(Op::FSqrt)?, &args[0]),
            ("inversesqrt", _) => self.map1(one(Op::FInvSqrt)?, &args[0]),
            ("floor", _) => self.map1(one(Op::FFloor)?, &args[0]),
            ("ceil", _) => self.map1(one(Op::FCeil)?, &args[0]),
            ("fract", _) => self.map1(one(Op::FFract)?, &args[0]),
            ("trunc", _) => self.map1(one(Op::FTrunc)?, &args[0]),
            ("round", _) => self.map1(one(Op::FRound)?, &args[0]),
            ("atan", 1) => self.map1(one(Op::FAtan)?, &args[0]),
            ("abs" | "sign", 1) if ints => {
                let op = if name == "abs" { Op::IAbs } else { Op::ISign };
                let mut slots = Vec::with_capacity(args[0].slots.len());
                for &s in &args[0].slots {
                    slots.push(self.op1(op, s)?);
                }
                Ok(Val {
                    ty: args[0].ty,
                    slots,
                    of: None,
                })
            }
            ("abs", _) => self.map1(one(Op::FAbs)?, &args[0]),
            ("sign", _) => self.map1(one(Op::FSign)?, &args[0]),
            ("radians" | "degrees", 1) if floats => {
                let by = if name == "radians" {
                    std::f32::consts::PI / 180.0
                } else {
                    180.0 / std::f32::consts::PI
                };
                let by = Self::scalar(self.float(by));
                self.map2(Op::FMul, &args[0], &by, line)
            }
            ("atan", 2) if floats => self.map2(Op::FAtan2, &args[0], &args[1], line),
            ("pow", 2) if floats => self.map2(Op::FPow, &args[0], &args[1], line),
            ("mod", 2) if floats => self.map2(Op::FMod, &args[0], &args[1], line),
            ("step", 2) if floats => self.map2(Op::FStep, &args[0], &args[1], line),
            ("min" | "max", 2) if ints => {
                let op = if name == "min" { Op::IMin } else { Op::IMax };
                let ty = if args[0].slots.len() >= args[1].slots.len() {
                    args[0].ty
                } else {
                    args[1].ty
                };
                Ok(Val {
                    ty,
                    slots: self.each2(op, &args[0], &args[1], line)?,
                    of: None,
                })
            }
            ("min", 2) if floats => self.map2(Op::FMin, &args[0], &args[1], line),
            ("max", 2) if floats => self.map2(Op::FMax, &args[0], &args[1], line),
            ("clamp", 3) if ints => {
                let low = self.each2(Op::IMax, &args[0], &args[1], line)?;
                let low = Val {
                    ty: args[0].ty,
                    slots: low,
                    of: None,
                };
                Ok(Val {
                    ty: args[0].ty,
                    slots: self.each2(Op::IMin, &low, &args[2], line)?,
                    of: None,
                })
            }
            ("clamp", 3) if floats => self.map3(Op::FClamp, &args[0], &args[1], &args[2], line),
            ("mix", 3) if args[2].ty.base() == Base::Bool => {
                let (x, y) = self.alike(args[0].clone(), args[1].clone(), line)?;
                if args[2].slots.len() != x.slots.len() {
                    return Err(wrong());
                }
                let mut slots = Vec::with_capacity(x.slots.len());
                for k in 0..x.slots.len() {
                    slots.push(self.op3(Op::Select, args[2].slots[k], y.slots[k], x.slots[k])?);
                }
                Ok(Val {
                    ty: x.ty,
                    slots,
                    of: None,
                })
            }
            ("mix", 3) if floats => self.map3(Op::FMix, &args[0], &args[1], &args[2], line),
            ("smoothstep", 3) if floats => self.map3(Op::FSmooth, &args[0], &args[1], &args[2], line),
            ("length", 1) if floats => Ok(Self::scalar(self.length(&args[0])?)),
            ("distance", 2) if floats => {
                let between = self.map2(Op::FSub, &args[0], &args[1], line)?;
                Ok(Self::scalar(self.length(&between)?))
            }
            ("dot", 2) if floats && args[0].slots.len() == args[1].slots.len() => {
                let (a, b) = (
                    self.convert(&args[0], Base::Float)?,
                    self.convert(&args[1], Base::Float)?,
                );
                Ok(Self::scalar(self.dot(&a.slots, &b.slots)?))
            }
            ("cross", 2) if args.iter().all(|a| a.ty == Ty::Vector(Base::Float, 3)) => {
                let (a, b) = (&args[0].slots, &args[1].slots);
                let mut slots = Vec::with_capacity(3);
                for (i, j) in [(1, 2), (2, 0), (0, 1)] {
                    let left = self.op2(Op::FMul, a[i], b[j])?;
                    let right = self.op2(Op::FMul, a[j], b[i])?;
                    slots.push(self.op2(Op::FSub, left, right)?);
                }
                Ok(Val {
                    ty: args[0].ty,
                    slots,
                    of: None,
                })
            }
            ("normalize", 1) if floats => {
                let length = Self::scalar(self.length(&args[0])?);
                self.map2(Op::FDiv, &args[0], &length, line)
            }
            ("reflect", 2) if floats && args[0].ty == args[1].ty => {
                // I − 2 (N · I) N
                let (i, n) = (&args[0], &args[1]);
                let d = self.dot(&n.slots, &i.slots)?;
                let two = self.float(2.0);
                let twice = Self::scalar(self.op2(Op::FMul, two, d)?);
                let along = self.map2(Op::FMul, n, &twice, line)?;
                self.map2(Op::FSub, i, &along, line)
            }
            ("refract", 3) if floats && args[0].ty == args[1].ty && args[2].ty == Ty::FLOAT => {
                // k = 1 − η² (1 − (N · I)²); nothing when k < 0, else η I − (η (N · I) + √k) N
                let (i, n, eta) = (&args[0], &args[1], args[2].slots[0]);
                let d = self.dot(&n.slots, &i.slots)?;
                let one = self.float(1.0);
                let zero = self.float(0.0);
                let dd = self.op2(Op::FMul, d, d)?;
                let rest = self.op2(Op::FSub, one, dd)?;
                let ee = self.op2(Op::FMul, eta, eta)?;
                let scaled = self.op2(Op::FMul, ee, rest)?;
                let k = self.op2(Op::FSub, one, scaled)?;
                let root = self.op1(Op::FSqrt, k)?;
                let eta_d = self.op2(Op::FMul, eta, d)?;
                let along = Self::scalar(self.op2(Op::FAdd, eta_d, root)?);
                let bent_i = self.map2(Op::FMul, i, &Self::scalar(eta), line)?;
                let bent_n = self.map2(Op::FMul, n, &along, line)?;
                let bent = self.map2(Op::FSub, &bent_i, &bent_n, line)?;
                let inside = self.op2(Op::FLt, k, zero)?;
                let mut slots = Vec::with_capacity(bent.slots.len());
                for &s in &bent.slots {
                    slots.push(self.op3(Op::Select, inside, zero, s)?);
                }
                Ok(Val {
                    ty: bent.ty,
                    slots,
                    of: None,
                })
            }
            ("faceforward", 3) if floats && args[0].ty == args[1].ty && args[1].ty == args[2].ty => {
                let d = self.dot(&args[2].slots, &args[1].slots)?;
                let zero = self.float(0.0);
                let facing = self.op2(Op::FLt, d, zero)?;
                let mut slots = Vec::with_capacity(args[0].slots.len());
                for &s in &args[0].slots {
                    let away = self.op1(Op::FNeg, s)?;
                    slots.push(self.op3(Op::Select, facing, s, away)?);
                }
                Ok(Val {
                    ty: args[0].ty,
                    slots,
                    of: None,
                })
            }
            ("matrixCompMult", 2) if args[0].ty == args[1].ty && matches!(args[0].ty, Ty::Matrix(_)) => {
                Ok(Val {
                    ty: args[0].ty,
                    slots: self.each2(Op::FMul, &args[0], &args[1], line)?,
                    of: None,
                })
            }
            ("transpose", 1) if matches!(args[0].ty, Ty::Matrix(_)) => {
                let Ty::Matrix(n) = args[0].ty else {
                    return Err(wrong());
                };
                let n = usize::from(n);
                Ok(Val {
                    ty: args[0].ty,
                    slots: (0..n * n).map(|k| args[0].slots[(k % n) * n + k / n]).collect(),
                    of: None,
                })
            }
            ("lessThan" | "lessThanEqual" | "greaterThan" | "greaterThanEqual" | "equal" | "notEqual", 2)
                if args[0].ty == args[1].ty && matches!(args[0].ty, Ty::Vector(..)) =>
            {
                let float = args[0].ty.base() == Base::Float;
                let op = match (name, float) {
                    ("lessThan", true) => Op::FLt,
                    ("lessThanEqual", true) => Op::FLe,
                    ("greaterThan", true) => Op::FGt,
                    ("greaterThanEqual", true) => Op::FGe,
                    ("equal", true) => Op::FEq,
                    ("notEqual", true) => Op::FNe,
                    ("lessThan", false) => Op::ILt,
                    ("lessThanEqual", false) => Op::ILe,
                    ("greaterThan", false) => Op::IGt,
                    ("greaterThanEqual", false) => Op::IGe,
                    ("equal", false) => Op::IEq,
                    _ => Op::INe,
                };
                Ok(Val {
                    ty: vector(Base::Bool, args[0].slots.len()),
                    slots: self.each2(op, &args[0], &args[1], line)?,
                    of: None,
                })
            }
            ("any" | "all", 1) if args[0].ty.base() == Base::Bool => {
                let op = if name == "any" { Op::IOr } else { Op::IAnd };
                let mut joined = args[0].slots[0];
                for &s in &args[0].slots[1..] {
                    joined = self.op2(op, joined, s)?;
                }
                Ok(Val {
                    ty: Ty::BOOL,
                    slots: vec![joined],
                    of: None,
                })
            }
            ("not", 1) if args[0].ty.base() == Base::Bool => {
                let mut slots = Vec::with_capacity(args[0].slots.len());
                for &s in &args[0].slots {
                    slots.push(self.op1(Op::INot, s)?);
                }
                Ok(Val {
                    ty: args[0].ty,
                    slots,
                    of: None,
                })
            }
            (
                "radians" | "degrees" | "atan" | "pow" | "mod" | "step" | "min" | "max" | "clamp" | "mix"
                | "smoothstep" | "length" | "distance" | "dot" | "cross" | "normalize" | "reflect"
                | "refract" | "faceforward",
                _,
            ) => Err(wrong()),
            _ => fail(line, format!("there is no function `{name}`")),
        }
    }

    // ---- statements ------------------------------------------------------------------------

    /// `stmt` in a scope of its own, its slots given back after.
    fn scoped(&mut self, stmt: &Stmt) -> Res<()> {
        let mark = self.next;
        self.scopes.push(HashMap::new());
        let done = self.stmt(stmt);
        self.scopes.pop();
        self.next = mark;
        done
    }

    fn truth(&mut self, cond: &Node, what: &str) -> Res<Val> {
        let v = self.expr(cond)?;
        if v.ty != Ty::BOOL {
            return fail(cond.line, format!("{what} asks a truth, not a {}", v.ty.name()));
        }
        Ok(v)
    }

    fn declare_vars(&mut self, ty: Ty, constant: bool, vars: &[Declared], fixed: bool) -> Res<()> {
        for var in vars {
            let len = match &var.array {
                None => None,
                Some(length) => match self.const_int(length) {
                    Some(n) if n >= 1 && n as u32 <= self.limits.array_len => Some(n as u32),
                    Some(_) => {
                        return fail(
                            var.line,
                            format!("an array holds from 1 to {} elements", self.limits.array_len),
                        );
                    }
                    None => return fail(var.line, "an array's length must be a constant whole number"),
                },
            };
            let each = ty.components();
            let total = each * len.unwrap_or(1) as usize;
            let slot = self.alloc(total)?;
            let mark = self.next;
            let lvl = self.lvl;
            let mut value = None;
            match (&var.init, len) {
                (Some(_), Some(_)) => {
                    return fail(
                        var.line,
                        "an array given its values where it's declared isn't supported yet",
                    );
                }
                (Some(init), None) => {
                    if constant && ty == Ty::INT {
                        value = self.const_int(init);
                    }
                    let v = self.expr(init)?;
                    let v = self.fit(v, ty, var.line)?;
                    for (k, &src) in v.slots.iter().enumerate() {
                        self.store(slot + k as Slot, src, lvl)?;
                    }
                }
                (None, _) => {
                    let zero = self.zero(ty.base());
                    for k in 0..total {
                        self.store(slot + k as Slot, zero, lvl)?;
                    }
                }
            }
            self.next = mark;
            self.declare(
                &var.name,
                Var {
                    ty,
                    slot,
                    lvl,
                    len,
                    constant: value,
                    fixed: fixed || constant,
                },
            );
        }
        Ok(())
    }

    fn stmt(&mut self, stmt: &Stmt) -> Res<()> {
        self.enter_walk(0)?;
        match stmt {
            Stmt::Empty => {}
            Stmt::Decl { ty, constant, vars } => self.declare_vars(*ty, *constant, vars, false)?,
            Stmt::Expr(e) => {
                let mark = self.next;
                self.expr(e)?;
                self.next = mark;
            }
            Stmt::Block(stmts) => {
                let mark = self.next;
                self.scopes.push(HashMap::new());
                let mut done = Ok(());
                for s in stmts {
                    done = self.stmt(s);
                    if done.is_err() {
                        break;
                    }
                }
                self.scopes.pop();
                self.next = mark;
                done?;
            }
            Stmt::If(cond, then, otherwise) => {
                let mark = self.next;
                let c = self.truth(cond, "`if`")?;
                // A copy, in case a branch changes what was asked.
                let cond_slot = self.settled(c)?.slots[0];
                self.deeper(1, cond.line)?;
                let outer = std::mem::take(&mut self.cur);
                self.lvl += 1;
                let mut done = self.scoped(then);
                let then_block = self.take_block();
                if done.is_ok()
                    && let Some(otherwise) = otherwise
                {
                    done = self.scoped(otherwise);
                }
                let otherwise_block = self.take_block();
                self.lvl -= 1;
                self.cur = outer;
                done?;
                self.push(Code::If {
                    cond: cond_slot,
                    then: then_block,
                    otherwise: otherwise_block,
                })?;
                self.next = mark;
            }
            Stmt::For {
                init,
                cond,
                step,
                body,
            } => self.lower_loop(init.as_deref(), cond.as_ref(), step.as_ref(), body, false)?,
            Stmt::While(cond, body) => self.lower_loop(None, Some(cond), None, body, false)?,
            Stmt::DoWhile(body, cond) => self.lower_loop(None, Some(cond), None, body, true)?,
            Stmt::Return(value, line) => {
                let Some(frame) = self.frames.last().copied() else {
                    return fail(*line, "`return` outside a function");
                };
                let mark = self.next;
                match (value, frame.ret) {
                    (None, Ty::Void) => {}
                    (Some(e), ret) if ret != Ty::Void => {
                        let v = self.expr(e)?;
                        let v = self.fit(v, ret, *line)?;
                        for (k, &src) in v.slots.iter().enumerate() {
                            self.store(frame.slot + k as Slot, src, frame.lvl - 1)?;
                        }
                    }
                    (None, ret) => return fail(*line, format!("this function returns a {}", ret.name())),
                    (Some(_), _) => return fail(*line, "this function returns nothing"),
                }
                self.next = mark;
                self.push(Code::Return { lvl: frame.lvl })?;
            }
            Stmt::Break(line) => {
                let Some(l) = self.loops.last().copied() else {
                    return fail(*line, "`break` outside a loop");
                };
                self.push(Code::Break { lvl: l.live })?;
            }
            Stmt::Continue(line) => {
                let Some(l) = self.loops.last().copied() else {
                    return fail(*line, "`continue` outside a loop");
                };
                self.push(Code::Continue { lvl: l.body })?;
            }
            Stmt::Discard => self.push(Code::Discard)?,
        }
        self.walk -= 1;
        Ok(())
    }

    fn lower_loop(
        &mut self,
        init: Option<&Stmt>,
        cond: Option<&Node>,
        step: Option<&Node>,
        body: &Stmt,
        body_first: bool,
    ) -> Res<()> {
        let mark = self.next;
        let line = cond
            .map(|c| c.line)
            .or_else(|| init.and_then(first_line))
            .or_else(|| step.map(|s| s.line))
            .or_else(|| first_line(body))
            .unwrap_or(0);
        self.deeper(2, line)?;
        let turns = match body_first {
            false => self.counted_turns(init, cond, step, body),
            true => None,
        };
        // `while (true)` asks nothing each turn.
        let cond = cond.filter(|c| c.e != Expr::Bool(true));
        let (live, body_lvl) = (self.lvl + 1, self.lvl + 2);
        let outer = std::mem::take(&mut self.cur);
        self.scopes.push(HashMap::new());
        self.lvl = live;
        let parts = (|| -> Res<Loop> {
            if let Some(init) = init {
                self.stmt(init)?;
            }
            let init = self.take_block();
            let cond = match cond {
                Some(cond) => Some(self.truth(cond, "a loop")?.slots[0]),
                None => None,
            };
            let test = self.take_block();
            self.lvl = body_lvl;
            self.loops.push(InLoop { live, body: body_lvl });
            let done = self.scoped(body);
            self.loops.pop();
            self.lvl = live;
            done?;
            let body = self.take_block();
            if let Some(step) = step {
                let mark = self.next;
                self.expr(step)?;
                self.next = mark;
            }
            let step = self.take_block();
            Ok(Loop {
                init,
                test,
                cond,
                body,
                step,
                body_first,
                turns,
                line,
            })
        })();
        self.lvl = live - 1;
        self.scopes.pop();
        self.cur = outer;
        self.next = mark;
        self.push(Code::Loop(Box::new(parts?)))
    }

    /// How many turns a `for` loop takes, when that is plain from how it's written: a whole
    /// number counted from one constant to another by a constant step, which nothing in the
    /// body changes. A loop that would count the wrong way for ever takes as many as can be
    /// told of.
    fn counted_turns(
        &self,
        init: Option<&Stmt>,
        cond: Option<&Node>,
        step: Option<&Node>,
        body: &Stmt,
    ) -> Option<u32> {
        let Stmt::Decl {
            ty: Ty::INT, vars, ..
        } = init?
        else {
            return None;
        };
        let [counter] = vars.as_slice() else {
            return None;
        };
        if counter.array.is_some() {
            return None;
        }
        let name = counter.name.as_str();
        let is_counter = |e: &Node| matches!(&e.e, Expr::Var(v) if v == name);
        let from = i64::from(self.const_int(counter.init.as_ref()?)?);
        let Expr::Binary(test, left, right) = &cond?.e else {
            return None;
        };
        if !is_counter(left) || mentions(right, name) {
            return None;
        }
        let to = i64::from(self.const_int(right)?);
        let by = match &step?.e {
            Expr::Step { target, up, .. } if is_counter(target) => {
                if *up {
                    1
                } else {
                    -1
                }
            }
            Expr::Assign(Some(op @ (BinOp::Add | BinOp::Sub)), target, amount)
                if is_counter(target) && !mentions(amount, name) =>
            {
                let amount = i64::from(self.const_int(amount)?);
                if *op == BinOp::Add { amount } else { -amount }
            }
            _ => return None,
        };
        if stmt_changes(body, name, self.unit) {
            return None;
        }
        // Turns of a counter that gets there; one that never does goes round until it wraps.
        let never = i64::from(u32::MAX);
        let turns = match (test, by.signum()) {
            (BinOp::Lt, 1) => (to - from + by - 1).div_euclid(by).max(0),
            (BinOp::Le, 1) if from <= to => (to - from) / by + 1,
            (BinOp::Gt, -1) => (from - to - by - 1).div_euclid(-by).max(0),
            (BinOp::Ge, -1) if from >= to => (from - to) / -by + 1,
            (BinOp::Le, 1) | (BinOp::Ge, -1) => 0,
            (BinOp::Lt, _) if from < to => never,
            (BinOp::Le, _) if from <= to => never,
            (BinOp::Gt, _) if from > to => never,
            (BinOp::Ge, _) if from >= to => never,
            (BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge, _) => 0,
            (BinOp::Ne, _) if from == to => 0,
            (BinOp::Ne, sign) if sign != 0 && (to - from) % by == 0 && (to - from) / by > 0 => {
                (to - from) / by
            }
            (BinOp::Ne, _) => never,
            _ => return None,
        };
        Some(turns.min(never) as u32)
    }

    // ---- the whole program -----------------------------------------------------------------

    /// A value every shader is given from outside: its slots, and its name in scope.
    fn given(&mut self, name: &str, ty: Ty, uniform_slots: &mut Vec<Slot>) -> Res<Slot> {
        let slot = self.alloc(ty.components())?;
        uniform_slots.extend(slot..slot + ty.components() as Slot);
        self.declare(
            name,
            Var {
                ty,
                slot,
                lvl: 0,
                len: None,
                constant: None,
                fixed: true,
            },
        );
        Ok(slot)
    }

    fn program(mut self, header: Header) -> Res<Program> {
        let vec2 = Ty::Vector(Base::Float, 2);
        let vec4 = Ty::Vector(Base::Float, 4);
        let mut uniform_slots = Vec::new();
        // In the order of `TIME`, `SIZE`, and the rest above.
        self.given("TIME", Ty::FLOAT, &mut uniform_slots)?;
        let size = self.given("RENDERSIZE", vec2, &mut uniform_slots)?;
        self.given("TIMEDELTA", Ty::FLOAT, &mut uniform_slots)?;
        self.given("FRAMEINDEX", Ty::INT, &mut uniform_slots)?;
        self.given("PASSINDEX", Ty::INT, &mut uniform_slots)?;
        // The date is always nothing: a frame mustn't depend on the day it's drawn.
        self.given("DATE", vec4, &mut uniform_slots)?;
        debug_assert_eq!(uniform_slots.len(), BUILT_IN);
        let mut defaults = vec![0u32; BUILT_IN];
        defaults[SIZE] = 1.0f32.to_bits();
        defaults[SIZE + 1] = 1.0f32.to_bits();
        let mut input_at = Vec::with_capacity(header.inputs.len());
        for input in &header.inputs {
            input_at.push(uniform_slots.len());
            let (ty, bits): (Ty, Vec<u32>) = match &input.kind {
                InputKind::Float { default, .. } => (Ty::FLOAT, vec![default.to_bits()]),
                InputKind::Bool { default } => (Ty::BOOL, vec![if *default { TRUE } else { 0 }]),
                InputKind::Event => (Ty::BOOL, vec![0]),
                InputKind::Long { default, .. } => (Ty::INT, vec![*default as u32]),
                InputKind::Point { default, .. } => (vec2, default.iter().map(|v| v.to_bits()).collect()),
                InputKind::Color { default } => (vec4, default.iter().map(|v| v.to_bits()).collect()),
                InputKind::Image | InputKind::Audio | InputKind::AudioFft => {
                    return fail(
                        0,
                        format!(
                            "input \"{}\" is a picture or the music, which isn't supported yet",
                            input.name
                        ),
                    );
                }
            };
            if self.find(&input.name).is_some() {
                return fail(
                    0,
                    format!("input \"{}\" has a name the shader is already given", input.name),
                );
            }
            self.given(&input.name, ty, &mut uniform_slots)?;
            defaults.extend(bits);
        }
        // The pixel's place and its color.
        let coord = self.alloc(4)?;
        let color = self.alloc(4)?;
        let normal = self.alloc(2)?;
        for (name, ty, slot, fixed) in [
            ("gl_FragCoord", vec4, coord, true),
            ("gl_FragColor", vec4, color, false),
            ("isf_FragNormCoord", vec2, normal, true),
            ("vv_FragNormCoord", vec2, normal, true),
        ] {
            self.declare(
                name,
                Var {
                    ty,
                    slot,
                    lvl: 0,
                    len: None,
                    constant: None,
                    fixed,
                },
            );
        }
        let (zero, one) = (self.float(0.0), self.float(1.0));
        self.store(coord + 2, zero, 0)?;
        self.store(coord + 3, one, 0)?;
        for k in 0..4 {
            self.store(color + k, zero, 0)?;
        }
        for k in 0..2 {
            let across = self.op2(Op::FDiv, coord + k, size + k)?;
            self.store(normal + k, across, 0)?;
        }
        // The shader's own globals, in the order it declares them.
        let unit = self.unit;
        for global in &unit.globals {
            if global.uniform {
                // One this isn't told about is nothing; one it is told about is already there.
                if self.find(&global.var.name).is_none() {
                    if global.var.array.is_some() {
                        return fail(global.var.line, "arrays given from outside aren't supported yet");
                    }
                    let slot = self.alloc(global.ty.components())?;
                    self.declare(
                        &global.var.name,
                        Var {
                            ty: global.ty,
                            slot,
                            lvl: 0,
                            len: None,
                            constant: None,
                            fixed: true,
                        },
                    );
                }
                continue;
            }
            self.declare_vars(
                global.ty,
                global.constant,
                std::slice::from_ref(&global.var),
                false,
            )?;
        }
        let Some(main) = unit
            .functions
            .iter()
            .position(|f| f.name == "main" && f.params.is_empty())
        else {
            return fail(0, "the shader has no `main`");
        };
        if unit.functions[main].ret != Ty::Void {
            return fail(unit.functions[main].line, "`main` returns nothing");
        }
        self.call_user(&[main], &[], unit.functions[main].line)?;
        let mut code = self.take_block();
        // Constants follow the other slots.
        let slots = self.most;
        let total = slots as usize + self.constants.len();
        if total > self.limits.slots {
            return fail(0, "the shader keeps too many values at once");
        }
        renumber(&mut code, slots);
        let constants = self
            .constants
            .iter()
            .enumerate()
            .map(|(k, &bits)| (slots + k as Slot, bits))
            .collect();
        let program = Program {
            code,
            slots: total as u32,
            levels: self.deepest + 2,
            constants,
            uniform_slots,
            coord: [coord, coord + 1],
            color: [color, color + 1, color + 2, color + 3],
            limits: *self.limits,
            header,
            input_at,
            defaults,
            instructions: self.written,
            least_work: 0,
        };
        check(&program.code, 0, &program, slots)?;
        // What can't finish is refused here, not stopped a frame at a time.
        let least_work = match least(&program.code, 0) {
            Least {
                endless: Some(line), ..
            } => return fail(line, "a loop that never ends"),
            found => found.least(),
        };
        if least_work > u64::from(program.limits.work) {
            return fail(
                0,
                format!(
                    "the shader does more work for every pixel than a shader may ({least_work}, and the most is {})",
                    program.limits.work
                ),
            );
        }
        Ok(Program {
            least_work,
            ..program
        })
    }
}

/// The first line of `stmt` that is known (a statement doesn't keep its own).
fn first_line(stmt: &Stmt) -> Option<u32> {
    match stmt {
        Stmt::Empty | Stmt::Discard => None,
        Stmt::Decl { vars, .. } => vars.first().map(|v| v.line),
        Stmt::Expr(e) | Stmt::If(e, ..) | Stmt::While(e, _) => Some(e.line),
        Stmt::Return(_, line) | Stmt::Break(line) | Stmt::Continue(line) => Some(*line),
        Stmt::For {
            init,
            cond,
            step,
            body,
        } => init
            .as_deref()
            .and_then(first_line)
            .or(cond.as_ref().map(|c| c.line))
            .or(step.as_ref().map(|s| s.line))
            .or_else(|| first_line(body)),
        Stmt::DoWhile(body, cond) => first_line(body).or(Some(cond.line)),
        Stmt::Block(stmts) => stmts.iter().find_map(first_line),
    }
}

/// Whether `e` names the variable `name` anywhere.
fn mentions(e: &Node, name: &str) -> bool {
    match &e.e {
        Expr::Int(_) | Expr::Float(_) | Expr::Bool(_) => false,
        Expr::Var(v) => v == name,
        Expr::Call(_, args) => args.iter().any(|a| mentions(a, name)),
        Expr::Field(a, _) | Expr::Unary(_, a) => mentions(a, name),
        Expr::Step { target, .. } => mentions(target, name),
        Expr::Index(a, b) | Expr::Binary(_, a, b) | Expr::Comma(a, b) | Expr::Assign(_, a, b) => {
            mentions(a, name) || mentions(b, name)
        }
        Expr::Ternary(a, b, c) => mentions(a, name) || mentions(b, name) || mentions(c, name),
    }
}

/// Whether working `e` out could change the variable `name`: it is stored to, counted up or
/// down, or handed to one of the shader's own functions (which may pass it back out).
fn expr_changes(e: &Node, name: &str, unit: &Unit) -> bool {
    match &e.e {
        Expr::Int(_) | Expr::Float(_) | Expr::Bool(_) | Expr::Var(_) => false,
        Expr::Assign(_, target, value) => mentions(target, name) || expr_changes(value, name, unit),
        Expr::Step { target, .. } => mentions(target, name),
        Expr::Call(called, args) => {
            let own = unit.functions.iter().any(|f| &f.name == called);
            args.iter()
                .any(|a| (own && mentions(a, name)) || expr_changes(a, name, unit))
        }
        Expr::Field(a, _) | Expr::Unary(_, a) => expr_changes(a, name, unit),
        Expr::Index(a, b) | Expr::Binary(_, a, b) | Expr::Comma(a, b) => {
            expr_changes(a, name, unit) || expr_changes(b, name, unit)
        }
        Expr::Ternary(a, b, c) => {
            expr_changes(a, name, unit) || expr_changes(b, name, unit) || expr_changes(c, name, unit)
        }
    }
}

/// Whether `stmt` could change the variable `name`, or declares another of that name (which
/// would make it hard to tell which one a later line means).
fn stmt_changes(stmt: &Stmt, name: &str, unit: &Unit) -> bool {
    let expr = |e: &Node| expr_changes(e, name, unit);
    let inner = |s: &Stmt| stmt_changes(s, name, unit);
    match stmt {
        Stmt::Empty | Stmt::Break(_) | Stmt::Continue(_) | Stmt::Discard | Stmt::Return(None, _) => false,
        Stmt::Decl { vars, .. } => vars.iter().any(|v| {
            v.name == name || v.init.as_ref().is_some_and(expr) || v.array.as_ref().is_some_and(expr)
        }),
        Stmt::Expr(e) | Stmt::Return(Some(e), _) => expr(e),
        Stmt::If(cond, then, otherwise) => {
            expr(cond) || inner(then) || otherwise.as_deref().is_some_and(inner)
        }
        Stmt::For {
            init,
            cond,
            step,
            body,
        } => {
            init.as_deref().is_some_and(inner)
                || cond.as_ref().is_some_and(expr)
                || step.as_ref().is_some_and(expr)
                || inner(body)
        }
        Stmt::While(cond, body) | Stmt::DoWhile(body, cond) => expr(cond) || inner(body),
        Stmt::Block(stmts) => stmts.iter().any(inner),
    }
}

/// The least work a pixel can do in a block: on its way through to the end, and on its way out
/// by a `break`, `continue`, `return`, or `discard`.
#[derive(Debug, Default)]
struct Least {
    /// To the end of the block (`None`: nothing gets there).
    through: Option<u64>,
    /// To each way out, with the level it leaves (0 for `discard`: every level).
    out: Vec<(u16, u64)>,
    /// The line of a loop no pixel can leave, if there is one on the way.
    endless: Option<u32>,
}

impl Least {
    /// The least of all, whichever way a pixel goes.
    fn least(&self) -> u64 {
        let out = self.out.iter().map(|&(_, work)| work);
        self.through.into_iter().chain(out).min().unwrap_or(0)
    }

    /// Takes in a part that starts once `done` work is done: its ways out are carried up, and
    /// the least work to its end is returned (`None`: nothing gets through it).
    fn part(&mut self, done: u64, part: Least) -> Option<u64> {
        self.endless = self.endless.or(part.endless);
        for (to, work) in part.out {
            self.out.push((to, done.saturating_add(work)));
        }
        part.through
    }
}

/// The least work any pixel does running `block` at level `lvl`, from how the program is
/// written alone: every `if` takes its cheaper side, a loop whose turns can be counted
/// ([`Loop::turns`]) and that nothing leaves early takes them all, and any other loop takes
/// none. A lower bound, so that what it refuses could never have finished.
fn least(block: &Block, lvl: u16) -> Least {
    let mut done = u64::from(block.cost);
    let mut found = Least::default();
    for node in &block.nodes {
        match node {
            Code::If { then, otherwise, .. } => {
                let then = found.part(done, least(then, lvl + 1));
                let otherwise = found.part(done, least(otherwise, lvl + 1));
                match then.into_iter().chain(otherwise).min() {
                    Some(least) => done = done.saturating_add(least),
                    None => return found,
                }
            }
            Code::Call(body) => {
                let mut call = least(body, lvl + 1);
                // A `return` leaves the call and carries on here.
                let returns = call.out.iter().filter(|o| o.0 == lvl + 1).map(|o| o.1).min();
                call.out.retain(|o| o.0 != lvl + 1);
                let through = found.part(done, call);
                match through.into_iter().chain(returns).min() {
                    Some(least) => done = done.saturating_add(least),
                    None => return found,
                }
            }
            Code::Loop(l) => {
                let (live, body) = (lvl + 1, lvl + 2);
                let Some(init) = found.part(done, least(&l.init, live)) else {
                    return found;
                };
                done = done.saturating_add(init);
                let Some(test) = found.part(done, least(&l.test, live)) else {
                    return found;
                };
                let mut turn = least(&l.body, body);
                let leaves = turn.out.iter().any(|o| o.0 == live);
                let goes_on = turn.out.iter().filter(|o| o.0 == body).map(|o| o.1).min();
                turn.out.retain(|o| o.0 < live);
                let escapes = !turn.out.is_empty();
                // A pixel that leaves does so, at the soonest, in the first turn.
                let first = match l.body_first {
                    true => done,
                    false => done.saturating_add(test),
                };
                let body_through = found.part(first.saturating_add(1), turn);
                let step = found.part(first, least(&l.step, live));
                let each = body_through
                    .into_iter()
                    .chain(goes_on)
                    .min()
                    .zip(step)
                    .map(|(body, step)| {
                        1u64.saturating_add(body)
                            .saturating_add(step)
                            .saturating_add(test)
                    });
                match (l.cond.is_some(), leaves, l.turns, each) {
                    // Nothing asks it to stop and nothing leaves it.
                    (false, false, _, _) => {
                        if !escapes {
                            found.endless = found.endless.or(Some(l.line));
                        }
                        return found;
                    }
                    (true, false, Some(turns), Some(each)) if !l.body_first => {
                        done = first.saturating_add(each.saturating_mul(u64::from(turns)));
                    }
                    _ => done = first,
                }
            }
            Code::Break { lvl: to } | Code::Continue { lvl: to } | Code::Return { lvl: to } => {
                found.out.push((*to, done));
                return found;
            }
            Code::Discard => {
                found.out.push((0, done));
                return found;
            }
            Code::Ins(_) | Code::Store { .. } | Code::Load { .. } | Code::StoreAt { .. } => {}
        }
    }
    found.through = Some(done);
    found
}

/// Moves the constants' slots to follow the `slots` others.
fn renumber(block: &mut Block, slots: Slot) {
    let at = |s: &mut Slot| {
        if *s >= CONSTANTS {
            *s = slots + (*s - CONSTANTS);
        }
    };
    for node in &mut block.nodes {
        match node {
            Code::Ins(i) => {
                at(&mut i.a);
                at(&mut i.b);
                at(&mut i.c);
            }
            Code::Store { src, .. } => at(src),
            Code::Load { idx, .. } => at(idx),
            Code::StoreAt { idx, src, .. } => {
                at(idx);
                at(src);
            }
            Code::If {
                cond,
                then,
                otherwise,
            } => {
                at(cond);
                renumber(then, slots);
                renumber(otherwise, slots);
            }
            Code::Loop(l) => {
                if let Some(cond) = &mut l.cond {
                    at(cond);
                }
                for part in [&mut l.init, &mut l.test, &mut l.body, &mut l.step] {
                    renumber(part, slots);
                }
            }
            Code::Call(body) => renumber(body, slots),
            Code::Break { .. } | Code::Continue { .. } | Code::Return { .. } | Code::Discard => {}
        }
    }
}

/// Makes sure nothing in a program can send the machine outside its memory: every slot and
/// level is one the program has, nothing writes a constant, and no instruction writes a slot it
/// reads. The machine relies on it.
fn check(block: &Block, lvl: u16, program: &Program, writable: Slot) -> Res<()> {
    let broken = || -> Error { Error::new(0, "the shader couldn't be read (a fault in PixelFlow)") };
    let slot = |s: Slot| s < program.slots;
    let level = |l: u16| l < program.levels;
    if !level(lvl) {
        return Err(broken());
    }
    for node in &block.nodes {
        let fine = match node {
            Code::Ins(i) => {
                let reads = [i.a, i.b, i.c];
                let reads = &reads[..i.op.reads()];
                i.d < writable && reads.iter().all(|&s| slot(s) && s != i.d)
            }
            Code::Store {
                dst,
                src,
                lvl: declared,
            } => *dst < writable && slot(*src) && *declared <= lvl,
            Code::Load {
                d,
                base,
                stride,
                len,
                idx,
                off,
                count,
            } => {
                let end = u64::from(*base) + u64::from(*stride) * u64::from(*len);
                let out_end = u64::from(*d) + u64::from(*count);
                *stride >= 1
                    && *len >= 1
                    && *count >= 1
                    && end <= u64::from(writable)
                    && out_end <= u64::from(writable)
                    && u64::from(*off) + u64::from(*count) <= u64::from(*stride)
                    && (out_end <= u64::from(*base) || u64::from(*d) >= end)
                    && slot(*idx)
                    && !(*d..*d + *count).contains(idx)
            }
            Code::StoreAt {
                base,
                stride,
                len,
                idx,
                off,
                src,
                lvl: declared,
            } => {
                let end = u64::from(*base) + u64::from(*stride) * u64::from(*len);
                let inside = |s: Slot| u64::from(s) >= u64::from(*base) && u64::from(s) < end;
                *stride >= 1
                    && *len >= 1
                    && end <= u64::from(writable)
                    && off < stride
                    && slot(*idx)
                    && slot(*src)
                    && !inside(*idx)
                    && !inside(*src)
                    && *declared <= lvl
            }
            Code::If {
                cond,
                then,
                otherwise,
            } => {
                check(then, lvl + 1, program, writable)?;
                check(otherwise, lvl + 1, program, writable)?;
                slot(*cond)
            }
            Code::Loop(l) => {
                check(&l.init, lvl + 1, program, writable)?;
                check(&l.test, lvl + 1, program, writable)?;
                check(&l.step, lvl + 1, program, writable)?;
                check(&l.body, lvl + 2, program, writable)?;
                l.cond.is_none_or(slot)
            }
            Code::Call(body) => {
                check(body, lvl + 1, program, writable)?;
                true
            }
            Code::Break { lvl: from } | Code::Continue { lvl: from } | Code::Return { lvl: from } => {
                *from <= lvl
            }
            Code::Discard => true,
        };
        if !fine {
            return Err(broken());
        }
    }
    Ok(())
}
