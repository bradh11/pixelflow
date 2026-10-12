//! A spike: fragment shader files run on the CPU.
//!
//! A shader file is a GLSL fragment shader with a JSON header in a comment that declares its
//! inputs (the Interactive Shader Format). [`Program::compile`] reads one; a [`Machine`] runs it
//! for a batch of pixels at a time, each instruction working along the whole batch.
//!
//! What a pixel comes out as depends only on the program, the inputs, and that pixel's place:
//! never on the pixels drawn beside it, the thread, or the computer. The arithmetic is IEEE
//! single precision, and the functions the platform's math library would answer differently from
//! one system to the next (`sin`, `exp`, `pow`, and the rest) are worked out here instead (see
//! [`math`]).
//!
//! A shader is a program from a file, so nothing in it is trusted. It can't read or write
//! anything but its own variables; its size, its nesting, its memory, and the work it does for a
//! pixel are all capped (see [`Limits`]); recursion is refused; and no input makes it panic.

mod ast;
mod isf;
mod lex;
mod lower;
pub mod math;
mod parse;
mod vm;

pub use isf::{Header, Input, InputKind};

/// What a shader file says about itself, without reading the shader that follows.
pub fn read_header(source: &str) -> Result<Header, Error> {
    isf::header(source)
}
pub use lower::Program;
pub use vm::{Machine, RunStats, Uniforms};

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
    /// Instructions run for one pixel in one frame. A pixel that reaches it stops and comes out
    /// clear.
    pub work: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            source_bytes: 256 * 1024,
            tokens: 200_000,
            depth: 128,
            instructions: 200_000,
            slots: 16_384,
            array_len: 1024,
            work: 400_000,
        }
    }
}
