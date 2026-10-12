//! Fragment shader files run on the CPU.
//!
//! A shader file is a GLSL fragment shader with a JSON header in a comment that declares its
//! inputs (the Interactive Shader Format). [`Program::compile`] reads one; a [`Machine`] runs it
//! for a batch of pixels at a time, each instruction working along the whole batch.
//!
//! The reader is PixelFlow's own: a lexer and preprocessor ([`lex`]), a parser ([`parse`]), and
//! a type check that writes the program out as instructions with every call expanded in place
//! ([`lower`]). What it reads is the GLSL that shaders for lights are written in: scalars,
//! vectors, and square matrices of floats, whole numbers, and truths; arrays of them; functions
//! (`out` and `inout` arguments among them), `if`, loops, `break`, `continue`, `return`, and
//! `discard`; `#define` and `#if`; and the built-in functions that work on numbers. Structures,
//! pictures (`texture2D`, `IMG_PIXEL`), and how a value changes from pixel to pixel (`dFdx`)
//! are refused by name.
//!
//! What a pixel comes out as depends only on the program, the inputs, and that pixel's place:
//! never on the pixels drawn beside it, the thread, or the computer. The arithmetic is IEEE
//! single precision, and the functions the platform's math library would answer differently from
//! one system to the next (`sin`, `exp`, `pow`, and the rest) are worked out here instead (see
//! [`math`]).
//!
//! A shader is a program from a file, so nothing in it is trusted. It can't read or write
//! anything but its own variables; its size, its nesting, its memory, and the work it does for a
//! pixel and for a frame are all capped (see [`Limits`]), by counting and never by the clock, so
//! a shader that is stopped is stopped at the same place every time; recursion is refused, and
//! so is a loop nothing can leave; and no input makes it panic.

mod ast;
mod isf;
mod lex;
mod lower;
pub mod math;
mod parse;
mod vm;

pub use isf::{Header, Input, InputKind, MAX_CHOICES, MAX_INPUTS};
pub use lower::Program;
pub use vm::{Machine, RunStats, Uniforms};

/// What a shader file says about itself, without reading the shader that follows.
pub fn read_header(source: &str) -> Result<Header, Error> {
    isf::header(source)
}

/// Why a shader file can't be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// The line of the file it was noticed on (0: no one line).
    pub line: u32,
    pub message: String,
}

impl Error {
    pub(crate) fn new(line: u32, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line == 0 {
            write!(f, "{}", self.message)
        } else {
            write!(f, "line {}: {}", self.line, self.message)
        }
    }
}

impl std::error::Error for Error {}

/// The most a shader may be and do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Bytes of source.
    pub source_bytes: usize,
    /// Tokens once macros are expanded.
    pub tokens: usize,
    /// How deeply expressions, blocks, macros, and calls may nest.
    pub depth: u32,
    /// Instructions once every call is expanded in place.
    pub instructions: usize,
    /// Numbers a pixel keeps at once (every variable and working value, a component each).
    pub slots: usize,
    /// Elements in one array.
    pub array_len: u32,
    /// Work done for one pixel in one frame: each instruction it runs counts for about what it
    /// costs beside an addition (a sine for twelve). A pixel that reaches it stops and comes out
    /// clear, and a shader that does more than this for every pixel is refused when it's read.
    pub work: u32,
    /// Work done for all the pixels of one frame of one effect. Where there are so many pixels
    /// that `work` each would pass it, each may do less (see [`Program::work_for`]).
    pub frame_work: u64,
}

impl Default for Limits {
    /// The limits PixelFlow reads and runs shader files under.
    ///
    /// The two on work bound the time a frame can take whatever the file says. Measured on one
    /// thread of an M-series laptop, the dearest work there is costs about a quarter of a
    /// nanosecond a unit, so no frame of one effect takes longer than about 150 ms, and one on
    /// a few thousand pixels no longer than about 50 ms a thousand; a shader written to be
    /// drawn costs a small part of that (see `docs/benchmarks.md`).
    fn default() -> Self {
        Self {
            source_bytes: 256 * 1024,
            tokens: 200_000,
            depth: 128,
            instructions: 200_000,
            slots: 16_384,
            array_len: 1024,
            work: 200_000,
            frame_work: 600_000_000,
        }
    }
}
