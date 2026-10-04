//! Packs controller channels into sACN universes and assigns universe numbers.

/// Consecutive physical pixels of one size on a controller (used for universe packing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PixelRun {
    pub pixels: u32,
    pub channels_per_pixel: u8,
}

/// Splits a controller's channels into universe-sized chunks `(first_channel, len)`.
///
/// Without straddling, a chunk ends early rather than split a pixel's channels.
pub(crate) fn chunk_channels(
    runs: &[PixelRun],
    universe_size: u16,
    allow_straddle: bool,
) -> Vec<(usize, u16)> {
    let size = universe_size as usize;
    let total: usize = runs
        .iter()
        .map(|r| r.pixels as usize * r.channels_per_pixel as usize)
        .sum();
    if allow_straddle {
        return (0..total)
            .step_by(size)
            .map(|start| (start, (total - start).min(size) as u16))
            .collect();
    }
    let mut chunks = Vec::new();
    let (mut start, mut len) = (0usize, 0usize);
    for run in runs {
        let cpp = run.channels_per_pixel as usize;
        let mut remaining = run.pixels as usize;
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
