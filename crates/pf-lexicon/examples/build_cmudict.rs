//! Packs the CMU Pronouncing Dictionary (`cmudict.dict` from github.com/cmusphinx/cmudict; the
//! embedded one is from commit 74790861f652) into `data/cmudict.zst`, the form PixelFlow embeds:
//!
//! ```text
//! cargo run -p pf-lexicon --release --example build_cmudict -- path/to/cmudict.dict
//! ```
//!
//! Each word (lowercase letters and apostrophes only) keeps its first pronunciation, written as
//! the word, its phones as bytes from 0x80 up ([`pf_lexicon::Phone::code`]), and a newline;
//! words are in byte order, so a lookup is a binary search. The whole is zstd-compressed.

use pf_lexicon::{Phone, parse_phones};
use std::collections::BTreeMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = std::env::args().nth(1).ok_or("give the path to cmudict.dict")?;
    let text = std::fs::read_to_string(&source)?;
    let mut words: BTreeMap<String, Vec<Phone>> = BTreeMap::new();
    for line in text.lines() {
        // A comment can follow the phones: "aalborg AO1 L B AO0 R G # place, danish".
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((word, phones)) = line.split_once(' ') else {
            continue;
        };
        // Other pronunciations are "word(2)"; abbreviations and compounds ("a.m.", "ad-lib")
        // are left to the word's parts.
        if !word.bytes().all(|b| b.is_ascii_lowercase() || b == b'\'') {
            continue;
        }
        let Some(phones) = parse_phones(phones) else {
            return Err(format!("unreadable phones for {word}: {phones}").into());
        };
        words.entry(word.to_string()).or_insert(phones);
    }
    let mut packed = Vec::new();
    for (word, phones) in &words {
        packed.extend_from_slice(word.as_bytes());
        packed.extend(phones.iter().map(|p| p.code()));
        packed.push(b'\n');
    }
    let compressed = zstd::bulk::compress(&packed, 22)?;
    let out = concat!(env!("CARGO_MANIFEST_DIR"), "/data/cmudict.zst");
    std::fs::write(out, &compressed)?;
    println!(
        "{} words, {} bytes packed, {} bytes compressed, written to {out}",
        words.len(),
        packed.len(),
        compressed.len()
    );
    Ok(())
}
