//! Runs a compiled shader for a batch of pixels at a time.
//!
//! A program's values live in numbered slots, one number each. An instruction works on whole
//! slots: for every pixel of the batch at once (one tight loop along the batch), or just once
//! when what it reads is the same for every pixel. Whether a slot is the same for all (`uniform`)
//! or one number a pixel (`varying`) is found out as the program runs, so everything that
//! follows from the inputs and the time alone is worked out once a batch, never once a pixel:
//! a loop's counter, a rotation's sine and cosine, a shape that moves the same everywhere.
//!
//! Pixels that take different ways through an `if` or leave a loop at different turns are kept
//! track of with masks, a level for each `if`, loop, and call the program is inside (see
//! [`Machine::block`]). A value stored where only some pixels are running is blended in for
//! those pixels alone.
//!
//! A pixel's answer never depends on the pixels it shares a batch with: the same instructions
//! run for it, on the same numbers, whichever they are. The work a pixel has done is counted for
//! that pixel alone (each instruction it runs at that instruction's [`Op::weight`], whether or
//! not the batch let it be worked out once for all), and it is stopped by its own count.

use crate::lower::Program;
use crate::math;

pub(crate) type Slot = u32;

/// Most numbers one machine holds (slots times the width of a batch): 4 MB of them.
const MEMORY_NUMBERS: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Op {
    FAdd,
    FSub,
    FMul,
    FDiv,
    FMin,
    FMax,
    FMod,
    FPow,
    FAtan2,
    FStep,
    FLt,
    FLe,
    FGt,
    FGe,
    FEq,
    FNe,
    FNeg,
    FAbs,
    FSign,
    FFloor,
    FCeil,
    FFract,
    FTrunc,
    FRound,
    FSqrt,
    FInvSqrt,
    FSin,
    FCos,
    FTan,
    FAsin,
    FAcos,
    FAtan,
    FSinh,
    FCosh,
    FTanh,
    FExp,
    FLog,
    FExp2,
    FLog2,
    /// `a` to `b` by `c`.
    FMix,
    /// `a` kept from `b` to `c`.
    FClamp,
    /// From 0 at `a` to 1 at `b`, for `c`.
    FSmooth,
    IAdd,
    ISub,
    IMul,
    IDiv,
    IMod,
    IMin,
    IMax,
    IAnd,
    IOr,
    IXor,
    IShl,
    IShr,
    ILt,
    ILe,
    IGt,
    IGe,
    IEq,
    INe,
    INeg,
    IAbs,
    ISign,
    /// Every bit turned over: `~` for whole numbers and `!` for truths.
    INot,
    IntToFloat,
    FloatToInt,
    BoolToFloat,
    BoolToInt,
    FloatToBool,
    IntToBool,
    /// `b` where `a` is true, else `c`.
    Select,
    Copy,
}

impl Op {
    /// What running it counts for against a pixel's work ([`crate::Limits::work`]): about how
    /// long it takes beside an addition, so that the work a shader may do bounds the time it may
    /// take. The functions worked out by series ([`math`]) are the dear ones.
    pub(crate) fn weight(self) -> u32 {
        use Op::*;
        match self {
            FPow | FAsin | FAcos | FSinh | FCosh | FTanh => 24,
            FTan | FAtan | FAtan2 | FExp | FLog | FExp2 | FLog2 => 16,
            FSin | FCos => 12,
            FMod | FFloor | FCeil | FFract | FTrunc | FRound | FSqrt | FInvSqrt | FDiv | IDiv | IMod
            | FSmooth => 2,
            _ => 1,
        }
    }

    /// How many of `a`, `b`, `c` it reads.
    pub(crate) fn reads(self) -> usize {
        use Op::*;
        match self {
            FMix | FClamp | FSmooth | Select => 3,
            FAdd | FSub | FMul | FDiv | FMin | FMax | FMod | FPow | FAtan2 | FStep | FLt | FLe | FGt
            | FGe | FEq | FNe | IAdd | ISub | IMul | IDiv | IMod | IMin | IMax | IAnd | IOr | IXor | IShl
            | IShr | ILt | ILe | IGt | IGe | IEq | INe => 2,
            _ => 1,
        }
    }
}

/// One instruction: `d` from `a`, `b`, and `c`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ins {
    pub op: Op,
    pub d: Slot,
    pub a: Slot,
    pub b: Slot,
    pub c: Slot,
}

#[derive(Debug, Clone)]
pub(crate) struct Loop {
    /// Run once, before the first turn.
    pub init: Block,
    /// Works out `cond` before each turn (after the first, for `do … while`).
    pub test: Block,
    pub cond: Option<Slot>,
    pub body: Block,
    /// Run after each turn's body.
    pub step: Block,
    pub body_first: bool,
    /// How many turns it takes, when that is plain from how it's written (a counter from one
    /// constant to another that nothing else changes). The machine doesn't read it: it is what
    /// lets a shader that can't finish be refused before it's run.
    pub turns: Option<u32>,
    /// The line it's written on, for saying which loop can't end.
    pub line: u32,
}

#[derive(Debug, Clone)]
pub(crate) enum Node {
    Ins(Ins),
    /// A variable takes a value. `lvl` is the level the variable was declared at.
    Store {
        dst: Slot,
        src: Slot,
        lvl: u16,
    },
    /// `count` numbers read from element `idx` of `len` elements, each `stride` numbers, that
    /// start at `base`: the ones from `off` on, into `d` and the slots after it.
    Load {
        d: Slot,
        base: Slot,
        stride: u32,
        len: u32,
        idx: Slot,
        off: u32,
        count: u32,
    },
    /// One number written into element `idx` the same way.
    StoreAt {
        base: Slot,
        stride: u32,
        len: u32,
        idx: Slot,
        off: u32,
        src: Slot,
        lvl: u16,
    },
    If {
        cond: Slot,
        then: Block,
        otherwise: Block,
    },
    Loop(Box<Loop>),
    /// A call, expanded in place.
    Call(Block),
    /// Leave the loop whose level is `lvl`.
    Break {
        lvl: u16,
    },
    /// Leave this turn of the loop whose body's level is `lvl`.
    Continue {
        lvl: u16,
    },
    /// Leave the call whose level is `lvl`.
    Return {
        lvl: u16,
    },
    Discard,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Block {
    pub nodes: Vec<Node>,
    /// The work of the instructions directly in it: what running it once costs a pixel.
    pub cost: u32,
}

impl Node {
    /// What running it once counts for against a pixel's work, not counting the blocks inside it.
    pub(crate) fn weight(&self) -> u32 {
        match self {
            Node::Ins(ins) => ins.op.weight(),
            Node::Store { .. } => 1,
            // Each number is fetched from wherever that pixel's index points.
            Node::Load { count, .. } => count.saturating_mul(4),
            // A store by an index that varies looks at every element (one that was the same for
            // all is written out first), then writes each pixel's.
            Node::StoreAt { len, .. } => 4 + len / 64,
            _ => 0,
        }
    }
}

/// A value as the 32 bits a slot holds.
trait Bits: Copy {
    fn from_slot(bits: u32) -> Self;
    fn to_slot(self) -> u32;
}

impl Bits for f32 {
    #[inline(always)]
    fn from_slot(bits: u32) -> Self {
        f32::from_bits(bits)
    }
    #[inline(always)]
    fn to_slot(self) -> u32 {
        self.to_bits()
    }
}

impl Bits for i32 {
    #[inline(always)]
    fn from_slot(bits: u32) -> Self {
        bits as i32
    }
    #[inline(always)]
    fn to_slot(self) -> u32 {
        self as u32
    }
}

/// A truth is every bit set or none, so it masks as it is.
impl Bits for bool {
    #[inline(always)]
    fn from_slot(bits: u32) -> Self {
        bits != 0
    }
    #[inline(always)]
    fn to_slot(self) -> u32 {
        if self { u32::MAX } else { 0 }
    }
}

impl Bits for u32 {
    #[inline(always)]
    fn from_slot(bits: u32) -> Self {
        bits
    }
    #[inline(always)]
    fn to_slot(self) -> u32 {
        self
    }
}

/// How many of the numbers a shader is given are kept in place. A shader with more inputs than
/// that keeps them on the heap, so setting a frame up for it allocates.
const UNIFORMS_IN_PLACE: usize = 64;

/// The inputs a shader is run with: a number for each slot the program takes from outside. Made
/// by [`Program::uniforms`] and set through the program.
#[derive(Debug, Clone, PartialEq)]
pub struct Uniforms {
    len: usize,
    in_place: [u32; UNIFORMS_IN_PLACE],
    spilled: Vec<u32>,
}

impl Uniforms {
    pub(crate) fn new(bits: &[u32]) -> Self {
        let mut in_place = [0; UNIFORMS_IN_PLACE];
        let spilled = match in_place.get_mut(..bits.len()) {
            Some(room) => {
                room.copy_from_slice(bits);
                Vec::new()
            }
            None => bits.to_vec(),
        };
        Self {
            len: bits.len(),
            in_place,
            spilled,
        }
    }

    pub(crate) fn bits(&self) -> &[u32] {
        match self.in_place.get(..self.len) {
            Some(bits) => bits,
            None => &self.spilled,
        }
    }

    /// Sets number `at` (nothing, for one the program that made these doesn't take).
    pub(crate) fn set(&mut self, at: usize, bits: u32) {
        let all = match self.len <= UNIFORMS_IN_PLACE {
            true => &mut self.in_place[..self.len],
            false => &mut self.spilled[..],
        };
        if let Some(slot) = all.get_mut(at) {
            *slot = bits;
        }
    }
}

/// What a run cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RunStats {
    /// Pixels stopped for doing more work than the run allowed (they come out clear).
    pub stopped: u64,
    /// Work done, added up over the pixels (see [`crate::Limits::work`]).
    pub work: u64,
}

impl std::ops::AddAssign for RunStats {
    fn add_assign(&mut self, other: Self) {
        self.stopped += other.stopped;
        self.work += other.work;
    }
}

/// The working memory one thread runs a program in.
#[derive(Debug, Clone)]
pub struct Machine {
    /// Most pixels in a batch.
    width: usize,
    /// Pixels in the batch being run.
    n: usize,
    /// Each slot's number for each pixel, used when the slot is varying.
    vals: Vec<Vec<u32>>,
    /// Each slot's one number, used when it is uniform.
    uni: Vec<u32>,
    varying: Vec<bool>,
    /// The pixels running at each level, and how many they are.
    masks: Vec<Vec<u32>>,
    counts: Vec<u32>,
    scratch: [Vec<u32>; 3],
    /// Work every pixel of the batch has done, and what some have done besides.
    work_all: u64,
    work_own: Vec<u32>,
    /// No pixel's own work is more than this.
    work_own_most: u64,
    limit: u64,
    /// Pixels stopped by the limit, and pixels the shader discarded.
    stopped: Vec<u32>,
    discarded: Vec<u32>,
    any_stopped: bool,
    any_discarded: bool,
}

fn count(mask: &[u32]) -> u32 {
    mask.iter().map(|&m| m & 1).sum()
}

impl Machine {
    /// Working memory for `program`, running at most `width` pixels at a time.
    pub fn new(program: &Program, width: usize) -> Self {
        let slots = program.slots as usize;
        // A batch as wide as asked, within the memory one machine may take.
        let width = width.clamp(1, 4096).min((MEMORY_NUMBERS / slots.max(1)).max(1));
        let levels = program.levels as usize + 1;
        Self {
            width,
            n: 0,
            vals: vec![vec![0; width]; slots],
            uni: vec![0; slots],
            varying: vec![false; slots],
            masks: vec![vec![0; width]; levels],
            counts: vec![0; levels],
            scratch: [vec![0; width], vec![0; width], vec![0; width]],
            work_all: 0,
            work_own: vec![0; width],
            work_own_most: 0,
            limit: 0,
            stopped: vec![0; width],
            discarded: vec![0; width],
            any_stopped: false,
            any_discarded: false,
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    /// The bytes of working memory it holds.
    pub fn bytes(&self) -> usize {
        (self.vals.len() + self.masks.len() + 6) * self.width * std::mem::size_of::<u32>()
    }

    /// Runs `program` for the pixels whose centers are at (`xs`, `ys`), in cells from the bottom
    /// left of the picture, and gives each one's red, green, blue, and alpha as the shader left
    /// them (not clamped; a pixel stopped or discarded is all zeros). No pixel does more than
    /// `work` (see [`Program::work_for`]); one that would is stopped.
    ///
    /// Nothing is allocated unless the machine was made for another program.
    pub fn run(
        &mut self,
        program: &Program,
        uniforms: &Uniforms,
        work: u32,
        xs: &[f32],
        ys: &[f32],
        out: &mut [[f32; 4]],
    ) -> RunStats {
        let mut stats = RunStats::default();
        let total = xs.len().min(ys.len()).min(out.len());
        // Memory made for another program doesn't fit this one.
        if self.uni.len() != program.slots as usize || self.masks.len() != program.levels as usize + 1 {
            *self = Self::new(program, self.width);
        }
        self.limit = u64::from(work.min(program.limits.work));
        let mut at = 0;
        while at < total {
            let n = (total - at).min(self.width);
            self.n = n;
            // Nothing is left from the batch before.
            self.varying.fill(false);
            self.uni.fill(0);
            for &(slot, bits) in &program.constants {
                self.uni[slot as usize] = bits;
            }
            for (&slot, &bits) in program.uniform_slots.iter().zip(uniforms.bits()) {
                self.uni[slot as usize] = bits;
            }
            for (slot, from) in [(program.coord[0], xs), (program.coord[1], ys)] {
                let slot = slot as usize;
                for (to, &v) in self.vals[slot][..n].iter_mut().zip(&from[at..at + n]) {
                    *to = v.to_bits();
                }
                self.varying[slot] = true;
            }
            self.masks[0][..n].fill(u32::MAX);
            self.counts[0] = n as u32;
            self.work_all = 0;
            self.work_own[..n].fill(0);
            self.work_own_most = 0;
            self.stopped[..n].fill(0);
            self.discarded[..n].fill(0);
            self.any_stopped = false;
            self.any_discarded = false;
            self.block(&program.code, 0);
            for (k, px) in out[at..at + n].iter_mut().enumerate() {
                let gone = (self.any_stopped && self.stopped[k] != 0)
                    || (self.any_discarded && self.discarded[k] != 0);
                for (channel, &slot) in px.iter_mut().zip(&program.color) {
                    let slot = slot as usize;
                    let bits = if self.varying[slot] {
                        self.vals[slot][k]
                    } else {
                        self.uni[slot]
                    };
                    *channel = if gone { 0.0 } else { f32::from_bits(bits) };
                }
                stats.work += self.work_all + u64::from(self.work_own[k]);
                stats.stopped += u64::from(self.any_stopped && self.stopped[k] != 0);
            }
            at += n;
        }
        stats
    }

    /// Counts `cost` instructions against the pixels running at `lvl`.
    #[inline]
    fn charge(&mut self, cost: u32, lvl: usize) {
        if self.counts[lvl] as usize == self.n {
            self.work_all += u64::from(cost);
        } else {
            for (own, &m) in self.work_own[..self.n].iter_mut().zip(&self.masks[lvl][..self.n]) {
                *own = own.saturating_add(cost & m);
            }
            self.work_own_most += u64::from(cost);
        }
    }

    /// Stops the pixels at `lvl` that have done more than they may.
    fn stop_overworked(&mut self, lvl: usize) {
        if self.work_all + self.work_own_most <= self.limit {
            return;
        }
        let n = self.n;
        let mut stopped = 0;
        for k in 0..n {
            let over = self.masks[lvl][k] != 0 && self.work_all + u64::from(self.work_own[k]) > self.limit;
            self.scratch[0][k] = if over { u32::MAX } else { 0 };
            self.stopped[k] |= self.scratch[0][k];
            stopped += u32::from(over);
        }
        if stopped == 0 {
            return;
        }
        self.any_stopped = true;
        let gone = std::mem::take(&mut self.scratch[0]);
        for l in 0..=lvl {
            for (m, &g) in self.masks[l][..n].iter_mut().zip(&gone[..n]) {
                *m &= !g;
            }
            self.counts[l] -= stopped;
        }
        self.scratch[0] = gone;
    }

    /// Level `to` starts with the pixels running at `from` (the level above it).
    #[inline]
    fn enter(&mut self, from: usize, to: usize) {
        let (above, below) = self.masks.split_at_mut(to);
        below[0][..self.n].copy_from_slice(&above[from][..self.n]);
        self.counts[to] = self.counts[from];
    }

    /// Level `to` starts with the pixels running at `from` for which `cond` is true (or false,
    /// when `want` is).
    fn enter_where(&mut self, from: usize, to: usize, cond: Slot, want: bool) {
        let n = self.n;
        let flip = if want { 0 } else { u32::MAX };
        let (above, below) = self.masks.split_at_mut(to);
        let conds = &self.vals[cond as usize][..n];
        for ((m, &a), &c) in below[0][..n].iter_mut().zip(&above[from][..n]).zip(conds) {
            *m = a & (c ^ flip);
        }
        self.counts[to] = count(&below[0][..n]);
    }

    /// The pixels running at `lvl` stop running at every level from `from` to `lvl`.
    fn leave(&mut self, lvl: usize, from: usize) {
        let n = self.n;
        let leaving = self.counts[lvl];
        let mut gone = std::mem::take(&mut self.scratch[0]);
        gone[..n].copy_from_slice(&self.masks[lvl][..n]);
        for l in from.min(lvl)..=lvl {
            for (m, &g) in self.masks[l][..n].iter_mut().zip(&gone[..n]) {
                *m &= !g;
            }
            self.counts[l] -= leaving;
        }
        self.scratch[0] = gone;
    }

    /// Runs `block` for the pixels running at level `lvl`.
    ///
    /// Each `if`, loop, and call the program enters is a level deeper, with a mask of the pixels
    /// running there: a copy of the level above, less the pixels whose condition says otherwise.
    /// `break`, `continue`, and `return` take the pixels running where they stand out of every
    /// level up to the loop or call they leave. Those pixels are still in the mask of the level
    /// outside it, and carry on there once the others are done.
    fn block(&mut self, block: &Block, lvl: usize) {
        if self.counts[lvl] == 0 {
            return;
        }
        // A pixel this block would take past what it may do is stopped before the block is run,
        // so no pixel ever does more.
        self.charge(block.cost, lvl);
        self.stop_overworked(lvl);
        for node in &block.nodes {
            if self.counts[lvl] == 0 {
                return;
            }
            match node {
                Node::Ins(ins) => self.ins(ins),
                Node::Store {
                    dst,
                    src,
                    lvl: declared,
                } => self.store(*dst, *src, *declared as usize, lvl),
                Node::Load {
                    d,
                    base,
                    stride,
                    len,
                    idx,
                    off,
                    count,
                } => self.load(*d, *base, *stride, *len, *idx, *off, *count),
                Node::StoreAt {
                    base,
                    stride,
                    len,
                    idx,
                    off,
                    src,
                    lvl: declared,
                } => self.store_at(*base, *stride, *len, *idx, *off, *src, *declared as usize, lvl),
                Node::If {
                    cond,
                    then,
                    otherwise,
                } => {
                    if self.varying[*cond as usize] {
                        if !then.nodes.is_empty() {
                            self.enter_where(lvl, lvl + 1, *cond, true);
                            self.block(then, lvl + 1);
                        }
                        if !otherwise.nodes.is_empty() {
                            self.enter_where(lvl, lvl + 1, *cond, false);
                            self.block(otherwise, lvl + 1);
                        }
                    } else {
                        let taken = if self.uni[*cond as usize] != 0 {
                            then
                        } else {
                            otherwise
                        };
                        if !taken.nodes.is_empty() {
                            self.enter(lvl, lvl + 1);
                            self.block(taken, lvl + 1);
                        }
                    }
                }
                Node::Loop(l) => self.run_loop(l, lvl),
                Node::Call(body) => {
                    self.enter(lvl, lvl + 1);
                    self.block(body, lvl + 1);
                }
                Node::Break { lvl: from } | Node::Continue { lvl: from } | Node::Return { lvl: from } => {
                    self.leave(lvl, *from as usize);
                }
                Node::Discard => {
                    let n = self.n;
                    for (d, &m) in self.discarded[..n].iter_mut().zip(&self.masks[lvl][..n]) {
                        *d |= m;
                    }
                    self.any_discarded = true;
                    self.leave(lvl, 0);
                }
            }
        }
    }

    /// A loop: level `lvl + 1` is the pixels still going round, `lvl + 2` the ones in this
    /// turn's body.
    fn run_loop(&mut self, l: &Loop, lvl: usize) {
        let (live, body) = (lvl + 1, lvl + 2);
        self.enter(lvl, live);
        self.block(&l.init, live);
        let mut first = true;
        while self.counts[live] != 0 {
            if !(first && l.body_first) {
                self.block(&l.test, live);
                if let Some(cond) = l.cond {
                    let cond = cond as usize;
                    let n = self.n;
                    if self.varying[cond] {
                        for (m, &c) in self.masks[live][..n].iter_mut().zip(&self.vals[cond][..n]) {
                            *m &= c;
                        }
                        self.counts[live] = count(&self.masks[live][..n]);
                    } else if self.uni[cond] == 0 {
                        self.masks[live][..n].fill(0);
                        self.counts[live] = 0;
                    }
                }
                if self.counts[live] == 0 {
                    break;
                }
            }
            first = false;
            // A turn costs something even when it does nothing, so no loop is free.
            self.charge(1, live);
            self.enter(live, body);
            self.block(&l.body, body);
            self.block(&l.step, live);
            self.stop_overworked(live);
        }
    }

    #[inline(always)]
    fn copy(&mut self, src: usize, dst: usize) {
        if self.varying[src] {
            let mut out = std::mem::take(&mut self.vals[dst]);
            out[..self.n].copy_from_slice(&self.vals[src][..self.n]);
            self.vals[dst] = out;
            self.varying[dst] = true;
        } else {
            self.uni[dst] = self.uni[src];
            self.varying[dst] = false;
        }
    }

    /// A variable declared at level `declared` takes `src`, at level `lvl`.
    ///
    /// When every pixel that can see the variable is running here, it simply becomes `src`, and
    /// stays uniform if `src` is. Otherwise only the running pixels take it.
    fn store(&mut self, dst: Slot, src: Slot, declared: usize, lvl: usize) {
        let (dst, src) = (dst as usize, src as usize);
        if dst == src {
            return;
        }
        if self.counts[lvl] == self.counts[declared.min(lvl)] {
            self.copy(src, dst);
            return;
        }
        let n = self.n;
        let mut out = std::mem::take(&mut self.vals[dst]);
        if !self.varying[dst] {
            out[..n].fill(self.uni[dst]);
        }
        let mask = &self.masks[lvl][..n];
        if self.varying[src] {
            for ((o, &s), &m) in out[..n].iter_mut().zip(&self.vals[src][..n]).zip(mask) {
                *o = (s & m) | (*o & !m);
            }
        } else {
            let s = self.uni[src];
            for (o, &m) in out[..n].iter_mut().zip(mask) {
                *o = (s & m) | (*o & !m);
            }
        }
        self.vals[dst] = out;
        self.varying[dst] = true;
    }

    /// The element an index picks: the nearest there is, for one outside the array.
    #[inline]
    fn element(index: u32, len: u32) -> usize {
        (index as i32).clamp(0, len.max(1) as i32 - 1) as usize
    }

    #[allow(clippy::too_many_arguments)]
    fn load(&mut self, d: Slot, base: Slot, stride: u32, len: u32, idx: Slot, off: u32, count: u32) {
        let (d, base, stride, idx) = (d as usize, base as usize, stride as usize, idx as usize);
        let n = self.n;
        for j in 0..count as usize {
            let first = base + off as usize + j;
            if !self.varying[idx] {
                self.copy(first + Self::element(self.uni[idx], len) * stride, d + j);
                continue;
            }
            let mut out = std::mem::take(&mut self.vals[d + j]);
            for (k, o) in out[..n].iter_mut().enumerate() {
                let from = first + Self::element(self.vals[idx][k], len) * stride;
                *o = if self.varying[from] {
                    self.vals[from][k]
                } else {
                    self.uni[from]
                };
            }
            self.vals[d + j] = out;
            self.varying[d + j] = true;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn store_at(
        &mut self,
        base: Slot,
        stride: u32,
        len: u32,
        idx: Slot,
        off: u32,
        src: Slot,
        declared: usize,
        lvl: usize,
    ) {
        let first = base as usize + off as usize;
        let (stride, idx, src) = (stride as usize, idx as usize, src as usize);
        if !self.varying[idx] {
            let to = first + Self::element(self.uni[idx], len) * stride;
            self.store(to as Slot, src as Slot, declared, lvl);
            return;
        }
        // Every element becomes a number a pixel (written out once, if it was the same for all),
        // and then each running pixel writes the one its index picks.
        let n = self.n;
        for e in 0..len as usize {
            let to = first + e * stride;
            if !self.varying[to] {
                let same = self.uni[to];
                self.vals[to][..n].fill(same);
                self.varying[to] = true;
            }
        }
        for k in 0..n {
            if self.masks[lvl][k] == 0 {
                continue;
            }
            let to = first + Self::element(self.vals[idx][k], len) * stride;
            self.vals[to][k] = if self.varying[src] {
                self.vals[src][k]
            } else {
                self.uni[src]
            };
        }
    }

    #[inline(always)]
    fn un<A: Bits, O: Bits>(&mut self, i: &Ins, f: impl Fn(A) -> O) {
        let (a, d) = (i.a as usize, i.d as usize);
        if self.varying[a] {
            let mut out = std::mem::take(&mut self.vals[d]);
            for (o, &x) in out[..self.n].iter_mut().zip(&self.vals[a][..self.n]) {
                *o = f(A::from_slot(x)).to_slot();
            }
            self.vals[d] = out;
            self.varying[d] = true;
        } else {
            self.uni[d] = f(A::from_slot(self.uni[a])).to_slot();
            self.varying[d] = false;
        }
    }

    #[inline(always)]
    fn bin<A: Bits, B: Bits, O: Bits>(&mut self, i: &Ins, f: impl Fn(A, B) -> O) {
        let (a, b, d) = (i.a as usize, i.b as usize, i.d as usize);
        let n = self.n;
        match (self.varying[a], self.varying[b]) {
            (false, false) => {
                self.uni[d] = f(A::from_slot(self.uni[a]), B::from_slot(self.uni[b])).to_slot();
                self.varying[d] = false;
                return;
            }
            (true, true) => {
                let mut out = std::mem::take(&mut self.vals[d]);
                let (xs, ys) = (&self.vals[a][..n], &self.vals[b][..n]);
                for ((o, &x), &y) in out[..n].iter_mut().zip(xs).zip(ys) {
                    *o = f(A::from_slot(x), B::from_slot(y)).to_slot();
                }
                self.vals[d] = out;
            }
            (true, false) => {
                let mut out = std::mem::take(&mut self.vals[d]);
                let y = B::from_slot(self.uni[b]);
                for (o, &x) in out[..n].iter_mut().zip(&self.vals[a][..n]) {
                    *o = f(A::from_slot(x), y).to_slot();
                }
                self.vals[d] = out;
            }
            (false, true) => {
                let mut out = std::mem::take(&mut self.vals[d]);
                let x = A::from_slot(self.uni[a]);
                for (o, &y) in out[..n].iter_mut().zip(&self.vals[b][..n]) {
                    *o = f(x, B::from_slot(y)).to_slot();
                }
                self.vals[d] = out;
            }
        }
        self.varying[d] = true;
    }

    #[inline(always)]
    fn tern<A: Bits, B: Bits, C: Bits, O: Bits>(&mut self, i: &Ins, f: impl Fn(A, B, C) -> O) {
        let (a, b, c, d) = (i.a as usize, i.b as usize, i.c as usize, i.d as usize);
        if !self.varying[a] && !self.varying[b] && !self.varying[c] {
            let (x, y, z) = (self.uni[a], self.uni[b], self.uni[c]);
            self.uni[d] = f(A::from_slot(x), B::from_slot(y), C::from_slot(z)).to_slot();
            self.varying[d] = false;
            return;
        }
        let n = self.n;
        let mut out = std::mem::take(&mut self.vals[d]);
        let mut spread = std::mem::take(&mut self.scratch);
        // A uniform among them is written out along the batch, so one loop does for all.
        for (j, slot) in [a, b, c].into_iter().enumerate() {
            if !self.varying[slot] {
                spread[j][..n].fill(self.uni[slot]);
            }
        }
        {
            let pick = |j: usize, slot: usize| -> &[u32] {
                if self.varying[slot] {
                    &self.vals[slot][..n]
                } else {
                    &spread[j][..n]
                }
            };
            let (xs, ys, zs) = (pick(0, a), pick(1, b), pick(2, c));
            for (((o, &x), &y), &z) in out[..n].iter_mut().zip(xs).zip(ys).zip(zs) {
                *o = f(A::from_slot(x), B::from_slot(y), C::from_slot(z)).to_slot();
            }
        }
        self.scratch = spread;
        self.vals[d] = out;
        self.varying[d] = true;
    }

    fn ins(&mut self, i: &Ins) {
        use Op::*;
        match i.op {
            FAdd => self.bin(i, |x: f32, y: f32| x + y),
            FSub => self.bin(i, |x: f32, y: f32| x - y),
            FMul => self.bin(i, |x: f32, y: f32| x * y),
            FDiv => self.bin(i, |x: f32, y: f32| x / y),
            FMin => self.bin(i, |x: f32, y: f32| if y < x { y } else { x }),
            FMax => self.bin(i, |x: f32, y: f32| if x < y { y } else { x }),
            FMod => self.bin(i, |x: f32, y: f32| x - y * (x / y).floor()),
            FPow => self.bin(i, math::pow),
            FAtan2 => self.bin(i, math::atan2),
            FStep => self.bin(i, |edge: f32, x: f32| if x < edge { 0.0f32 } else { 1.0 }),
            FLt => self.bin(i, |x: f32, y: f32| x < y),
            FLe => self.bin(i, |x: f32, y: f32| x <= y),
            FGt => self.bin(i, |x: f32, y: f32| x > y),
            FGe => self.bin(i, |x: f32, y: f32| x >= y),
            FEq => self.bin(i, |x: f32, y: f32| x == y),
            FNe => self.bin(i, |x: f32, y: f32| x != y),
            FNeg => self.un(i, |x: f32| -x),
            FAbs => self.un(i, |x: f32| x.abs()),
            FSign => self.un(i, |x: f32| {
                if x > 0.0 {
                    1.0f32
                } else if x < 0.0 {
                    -1.0
                } else {
                    0.0
                }
            }),
            FFloor => self.un(i, |x: f32| x.floor()),
            FCeil => self.un(i, |x: f32| x.ceil()),
            FFract => self.un(i, |x: f32| x - x.floor()),
            FTrunc => self.un(i, |x: f32| x.trunc()),
            FRound => self.un(i, |x: f32| x.round()),
            FSqrt => self.un(i, |x: f32| x.sqrt()),
            FInvSqrt => self.un(i, |x: f32| 1.0 / x.sqrt()),
            FSin => self.un(i, math::sin),
            FCos => self.un(i, math::cos),
            FTan => self.un(i, math::tan),
            FAsin => self.un(i, math::asin),
            FAcos => self.un(i, math::acos),
            FAtan => self.un(i, math::atan),
            FSinh => self.un(i, math::sinh),
            FCosh => self.un(i, math::cosh),
            FTanh => self.un(i, math::tanh),
            FExp => self.un(i, math::exp),
            FLog => self.un(i, math::ln),
            FExp2 => self.un(i, math::exp2),
            FLog2 => self.un(i, math::log2),
            FMix => self.tern(i, |x: f32, y: f32, t: f32| x * (1.0 - t) + y * t),
            FClamp => self.tern(i, |x: f32, lo: f32, hi: f32| {
                let x = if x < lo { lo } else { x };
                if hi < x { hi } else { x }
            }),
            FSmooth => self.tern(i, |e0: f32, e1: f32, x: f32| {
                let t = (x - e0) / (e1 - e0);
                let t = if t < 0.0 { 0.0 } else { t };
                let t = if 1.0 < t { 1.0 } else { t };
                t * t * (3.0 - 2.0 * t)
            }),
            IAdd => self.bin(i, |x: i32, y: i32| x.wrapping_add(y)),
            ISub => self.bin(i, |x: i32, y: i32| x.wrapping_sub(y)),
            IMul => self.bin(i, |x: i32, y: i32| x.wrapping_mul(y)),
            IDiv => self.bin(i, |x: i32, y: i32| if y == 0 { 0 } else { x.wrapping_div(y) }),
            IMod => self.bin(i, |x: i32, y: i32| if y == 0 { 0 } else { x.wrapping_rem(y) }),
            IMin => self.bin(i, |x: i32, y: i32| x.min(y)),
            IMax => self.bin(i, |x: i32, y: i32| x.max(y)),
            IAnd => self.bin(i, |x: u32, y: u32| x & y),
            IOr => self.bin(i, |x: u32, y: u32| x | y),
            IXor => self.bin(i, |x: u32, y: u32| x ^ y),
            IShl => self.bin(i, |x: i32, y: i32| x.wrapping_shl(y as u32)),
            IShr => self.bin(i, |x: i32, y: i32| x.wrapping_shr(y as u32)),
            ILt => self.bin(i, |x: i32, y: i32| x < y),
            ILe => self.bin(i, |x: i32, y: i32| x <= y),
            IGt => self.bin(i, |x: i32, y: i32| x > y),
            IGe => self.bin(i, |x: i32, y: i32| x >= y),
            IEq => self.bin(i, |x: i32, y: i32| x == y),
            INe => self.bin(i, |x: i32, y: i32| x != y),
            INeg => self.un(i, |x: i32| x.wrapping_neg()),
            IAbs => self.un(i, |x: i32| x.wrapping_abs()),
            ISign => self.un(i, |x: i32| x.signum()),
            INot => self.un(i, |x: u32| !x),
            IntToFloat => self.un(i, |x: i32| x as f32),
            FloatToInt => self.un(i, |x: f32| x as i32),
            BoolToFloat => self.un(i, |x: bool| if x { 1.0f32 } else { 0.0 }),
            BoolToInt => self.un(i, |x: bool| i32::from(x)),
            FloatToBool => self.un(i, |x: f32| x != 0.0),
            IntToBool => self.un(i, |x: i32| x != 0),
            Select => {
                let (cond, d) = (i.a as usize, i.d as usize);
                if self.varying[cond] {
                    self.tern(i, |m: u32, yes: u32, no: u32| (yes & m) | (no & !m));
                } else if self.uni[cond] != 0 {
                    self.copy(i.b as usize, d);
                } else {
                    self.copy(i.c as usize, d);
                }
            }
            Copy => self.copy(i.a as usize, i.d as usize),
        }
    }
}
