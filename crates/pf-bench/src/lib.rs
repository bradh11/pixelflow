//! PixelFlow's end-to-end scale benchmark.
//!
//! It answers one question with evidence: how many pixels can PixelFlow play at 40 frames a
//! second? Synthetic shows ([`show`]) and sequences ([`sequence`]) at any size are run through
//! the engine's own render, mapping, packetizing, and sending ([`pipeline`]), flat out or in real
//! time ([`run`]), into a null sink or UDP listeners on the loopback interface ([`sink`]), and
//! every stage, frame, packet, and byte of memory is counted ([`measure`]). [`effects`] says
//! which effects the time goes on, and [`engine`] plays the same thing in the engine itself as a
//! check. `docs/benchmarks.md` has the method, the commands, and a baseline.
//!
//! Nothing here ever sends to a real controller: every address is on the loopback interface, and
//! the transport refuses anything else.

pub mod alloc;
pub mod effects;
pub mod engine;
pub mod measure;
pub mod pipeline;
pub mod report;
pub mod run;
pub mod sequence;
pub mod show;
pub mod sink;
pub mod stats;
pub mod system;
