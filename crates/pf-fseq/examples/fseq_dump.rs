//! Prints what PixelFlow's reader sees in a sequence file: the header fields and an FNV-1a 64
//! checksum of every frame's full channel space. The output lines match those of an external
//! FPP-based checker (see `scripts/fseq-fpp-oracle.sh`), so the two can be diffed.
//!
//! ```text
//! cargo run -p pf-fseq --example fseq_dump -- [--quiet] file.fseq
//! ```

use pf_fseq::{Compression, Sequence};
use std::process::ExitCode;

fn fnv1a(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let quiet = args.iter().any(|a| a == "--quiet");
    let Some(path) = args.iter().find(|a| !a.starts_with("--")) else {
        eprintln!("usage: fseq_dump [--quiet] file.fseq");
        return ExitCode::from(2);
    };
    let mut seq = match Sequence::open(path) {
        Ok(seq) => seq,
        Err(e) => {
            println!("OPEN FAILED: {e}");
            return ExitCode::FAILURE;
        }
    };
    let h = seq.header().clone();
    println!("version {}.{}", h.version.0, h.version.1);
    println!("max_channel {}", h.channels);
    println!("frames {}", h.frames);
    println!("step_ms {}", h.step_ms);
    let compression = match h.compression {
        Compression::None => "none",
        Compression::Zstd => "zstd",
        Compression::Zlib => "zlib",
    };
    println!("compression {compression}");
    println!("media \"{}\"", h.media.as_deref().unwrap_or(""));
    println!("producer \"{}\"", h.producer.as_deref().unwrap_or(""));
    let mut frame = vec![0u8; h.channels as usize];
    let mut all: u64 = 0xcbf2_9ce4_8422_2325;
    let mut failed = 0u32;
    for f in 0..h.frames {
        if let Err(e) = seq.read_frame(f, &mut frame) {
            println!("frame {f} FAILED: {e}");
            failed += 1;
            continue;
        }
        let hash = fnv1a(&frame);
        all = (all ^ hash).wrapping_mul(0x0100_0000_01b3);
        if !quiet {
            println!("frame {f} {hash:016x}");
        }
    }
    println!("failed_frames {failed}");
    println!("all_frames {all:016x}");
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
