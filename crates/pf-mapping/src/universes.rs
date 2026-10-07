//! Packs controller channels into sACN universes and assigns universe numbers.

use crate::layout::{Addressing, UniverseSpan};
use pf_model::{Issue, IssueCode, Protocol, Show, ValidationReport};

/// Consecutive physical pixels of one size on a controller (used for universe packing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PixelRun {
    pub pixels: u32,
    pub channels_per_pixel: u8,
}

/// Highest valid sACN universe number.
pub(crate) const MAX_UNIVERSE: u32 = 63_999;

/// Splits a controller's channels into universe-sized chunks `(first_channel, len)`.
///
/// Without straddling, a chunk ends early rather than split a pixel's channels. A universe too
/// small for one of the pixels can't avoid splitting it, so then channels run straight through
/// every universe, as xLights numbers them.
pub(crate) fn chunk_channels(
    runs: &[PixelRun],
    universe_size: u16,
    allow_straddle: bool,
) -> Vec<(usize, u16)> {
    let size = universe_size as usize;
    if size == 0 {
        return Vec::new();
    }
    let total: usize = runs
        .iter()
        .map(|r| r.pixels as usize * r.channels_per_pixel as usize)
        .sum();
    let too_small = runs
        .iter()
        .any(|r| r.pixels > 0 && r.channels_per_pixel as usize > size);
    if allow_straddle || too_small {
        return (0..total)
            .step_by(size)
            .map(|start| (start, (total - start).min(size) as u16))
            .collect();
    }
    let mut chunks = Vec::new();
    let (mut start, mut len) = (0usize, 0usize);
    for run in runs {
        let cpp = run.channels_per_pixel as usize;
        let mut remaining = if cpp == 0 { 0 } else { run.pixels as usize };
        while remaining > 0 {
            let fit = (size - len) / cpp;
            if fit == 0 {
                chunks.push((start, len as u16));
                start += len;
                len = 0;
                continue;
            }
            let take = fit.min(remaining);
            len += take * cpp;
            remaining -= take;
        }
    }
    if len > 0 {
        chunks.push((start, len as u16));
    }
    chunks
}

/// Chooses each controller's first universe. Pinned starts are kept; the rest are
/// packed from universe 1 upward, skipping pinned ranges.
pub(crate) fn allocate(requests: &[(Option<u16>, u32)]) -> Vec<u32> {
    let pinned: Vec<(u32, u32)> = requests
        .iter()
        .filter_map(|&(start, count)| start.map(|s| (u32::from(s), u32::from(s) + count)))
        .collect();
    let mut next = 1u32;
    requests
        .iter()
        .map(|&(start, count)| {
            if let Some(s) = start {
                return u32::from(s);
            }
            while let Some(&(_, end)) = pinned
                .iter()
                .find(|&&(ps, pe)| count > 0 && next < pe && ps < next + count)
            {
                next = end;
            }
            let chosen = next;
            next += count;
            chosen
        })
        .collect()
}

/// Packs and numbers universes for every sACN controller. `runs[i]` are controller `i`'s pixels.
pub(crate) fn assign(show: &Show, runs: &[&[PixelRun]], report: &mut ValidationReport) -> Vec<Addressing> {
    let chunks: Vec<Vec<(usize, u16)>> = show
        .controllers
        .iter()
        .zip(runs)
        .map(|(c, r)| match c.protocol {
            Protocol::Sacn(cfg) => chunk_channels(r, cfg.universe_size.channels(), cfg.allow_pixel_straddle),
            Protocol::Ddp => Vec::new(),
        })
        .collect();
    let requests: Vec<(Option<u16>, u32)> = show
        .controllers
        .iter()
        .zip(&chunks)
        .map(|(c, ch)| match c.protocol {
            Protocol::Sacn(cfg) => (cfg.start_universe, ch.len() as u32),
            Protocol::Ddp => (None, 0),
        })
        .collect();
    let starts = allocate(&requests);

    let mut ranges: Vec<(usize, u32, u32, bool)> = Vec::new();
    let addressing = show
        .controllers
        .iter()
        .enumerate()
        .map(|(i, c)| match c.protocol {
            Protocol::Ddp => Addressing::Ddp,
            Protocol::Sacn(cfg) => {
                let start = starts[i];
                let count = chunks[i].len() as u32;
                if count > 0 {
                    let last = start + count - 1;
                    if start == 0 || last > MAX_UNIVERSE {
                        report.push(
                            Issue::error(
                                IssueCode::UniverseOutOfRange,
                                format!(
                                    "'{}' needs universes {start}–{last}, but sACN universes must be between 1 and {MAX_UNIVERSE}.",
                                    c.name
                                ),
                            )
                            .with_fix("Choose a lower start universe, or clear it to assign automatically."),
                        );
                    }
                    ranges.push((i, start, start + count, cfg.multicast));
                }
                let universes = chunks[i]
                    .iter()
                    .enumerate()
                    .map(|(n, &(channel, len))| UniverseSpan {
                        universe: u16::try_from(start + n as u32).unwrap_or(u16::MAX),
                        controller_channel: channel,
                        len,
                    })
                    .collect();
                Addressing::Sacn {
                    universes,
                    multicast: cfg.multicast,
                }
            }
        })
        .collect();

    check_collisions(show, &ranges, report);
    addressing
}

fn check_collisions(show: &Show, ranges: &[(usize, u32, u32, bool)], report: &mut ValidationReport) {
    for (a, &(ia, sa, ea, ma)) in ranges.iter().enumerate() {
        for &(ib, sb, eb, mb) in &ranges[a + 1..] {
            if sa < eb && sb < ea {
                let (lo, hi) = (sa.max(sb), ea.min(eb) - 1);
                let (na, nb) = (&show.controllers[ia].name, &show.controllers[ib].name);
                let message = format!("'{na}' and '{nb}' both use universes {lo}–{hi}.");
                let fix = "Clear the pinned start universe on one controller so PixelFlow assigns it automatically.";
                let issue = if ma || mb {
                    Issue::error(IssueCode::UniverseCollision, message)
                } else {
                    Issue::warning(
                        IssueCode::UniverseCollision,
                        format!(
                            "{message} This works over unicast but breaks if either switches to multicast."
                        ),
                    )
                };
                report.push(issue.with_fix(fix));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(pixels: u32, channels_per_pixel: u8) -> PixelRun {
        PixelRun {
            pixels,
            channels_per_pixel,
        }
    }

    #[test]
    fn rgb_pixels_fill_510_channel_universes_exactly() {
        let chunks = chunk_channels(&[run(400, 3)], 510, false);
        assert_eq!(chunks, vec![(0, 510), (510, 510), (1020, 180)]);
    }

    #[test]
    fn rgbw_pixels_do_not_straddle_unless_allowed() {
        assert_eq!(
            chunk_channels(&[run(200, 4)], 510, false),
            vec![(0, 508), (508, 292)]
        );
        assert_eq!(
            chunk_channels(&[run(200, 4)], 510, true),
            vec![(0, 510), (510, 290)]
        );
    }

    #[test]
    fn mixed_runs_break_at_pixel_boundaries() {
        // 169 RGB pixels = 507 channels; the next RGBW pixel doesn't fit in the remaining 3.
        assert_eq!(
            chunk_channels(&[run(169, 3), run(2, 4)], 510, false),
            vec![(0, 507), (507, 8)]
        );
    }

    #[test]
    fn odd_universe_sizes_pack_whole_pixels() {
        // 15 channels = 5 RGB pixels; 16 leaves one channel over each time.
        assert_eq!(
            chunk_channels(&[run(12, 3)], 15, false),
            vec![(0, 15), (15, 15), (30, 6)]
        );
        assert_eq!(chunk_channels(&[run(6, 3)], 16, false), vec![(0, 15), (15, 3)]);
        assert_eq!(chunk_channels(&[run(6, 3)], 16, true), vec![(0, 16), (16, 2)]);
    }

    #[test]
    fn a_universe_smaller_than_a_pixel_straddles_as_xlights_does() {
        // A pixel can't fit in a 2-channel universe, so channels run straight through.
        let straight = vec![(0, 2), (2, 2), (4, 2), (6, 1)];
        assert_eq!(chunk_channels(&[run(1, 1), run(2, 3)], 2, false), straight);
        assert_eq!(chunk_channels(&[run(1, 1), run(2, 3)], 2, true), straight);
        assert_eq!(chunk_channels(&[run(2, 3)], 1, false).len(), 6);
    }

    #[test]
    fn no_channels_means_no_universes() {
        assert!(chunk_channels(&[], 510, false).is_empty());
        assert!(chunk_channels(&[], 510, true).is_empty());
    }

    #[test]
    fn automatic_starts_skip_pinned_ranges() {
        // auto(3) , pinned 2..5 (3 universes), auto(2), auto(0)
        let starts = allocate(&[(None, 3), (Some(2), 3), (None, 2), (None, 0)]);
        assert_eq!(starts, vec![5, 2, 8, 10]);
    }

    #[test]
    fn automatic_starts_pack_from_one() {
        assert_eq!(allocate(&[(None, 2), (None, 3)]), vec![1, 3]);
    }
}
