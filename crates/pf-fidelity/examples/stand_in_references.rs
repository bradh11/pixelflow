//! Writes PixelFlow's own render of each sequence of a corpus as its `.fseq`, where xLights'
//! reference would go: a way to try the corpus checks (`pf-fidelity regress`) end to end
//! without xLights, and to measure how far chance moves each effect's look. Never commit what
//! it writes: a reference is what xLights rendered.
//!
//! ```sh
//! cp -R crates/pf-fidelity/corpus /tmp/corpus-trial
//! cargo run --release -p pf-fidelity --example stand_in_references -- /tmp/corpus-trial 7
//! cargo run --release -p pf-fidelity -- regress --corpus /tmp/corpus-trial --accept
//! cargo run --release -p pf-fidelity -- regress --corpus /tmp/corpus-trial
//! ```
//!
//! The second argument picks the set of effect ids the stand-ins are drawn from (99 when it is
//! left out): any but the ones the checks score from, so that what chance draws differs.

use pf_fidelity::compare::render_fseq;
use pf_fidelity::corpus::Manifest;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let corpus =
        PathBuf::from(args.next().ok_or_else(|| {
            anyhow::anyhow!("name a copy of the corpus folder to write the stand-ins into")
        })?);
    // Never the corpus in the repository: what is there must come from xLights.
    if corpus.canonicalize()? == PathBuf::from(pf_fidelity::madeup::CORPUS_DIR).canonicalize()? {
        anyhow::bail!("that is the corpus in the repository: copy it somewhere else first");
    }
    let ids: u64 = args.next().map_or(Ok(99), |a| a.parse())?;
    let manifest = Manifest::read(&corpus.join("manifest.json"))?;
    for entry in &manifest.sequences {
        let bytes = render_fseq(&entry.show, &entry.xsq, ids)?;
        std::fs::write(&entry.fseq, bytes)?;
        println!("{}", entry.fseq.display());
    }
    Ok(())
}
