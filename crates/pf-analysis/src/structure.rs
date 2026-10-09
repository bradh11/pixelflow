//! Song structure: where sections start (where the sound changes) and which sections repeat.
//!
//! 1. **Self-similarity**: every beat-long unit compared with every other, once by timbre (and
//!    loudness) and once by harmony (chroma).
//! 2. **Novelty**: a checkerboard kernel slid along each matrix's diagonal (Foote, 2000) scores
//!    how different what comes after each unit is from what came before, at two scales.
//! 3. **Boundaries**: novelty peaks that stand out from their surroundings, at least 4 bars
//!    apart, moved to the nearest bar line.
//! 4. **Groups**: sections compared along the matrix's diagonals (the same material played again
//!    lines up there) and clustered; repeats share a letter (A, B, A, C …).
//! 5. **Names**: Intro and Outro at the ends, the loudest repeated group the Chorus, the repeated
//!    group before it the Verse, one-offs late in the song the Bridge, quiet stretches a Break.

use crate::grid::{Grid, Synced};

/// Beats per bar (4/4 is assumed).
pub(crate) const BEATS_PER_BAR: usize = 4;
/// The shortest section, in bars (an intro or outro may be half that).
const MIN_SECTION_BARS: usize = 4;
/// Kernel half-widths for the novelty, in units.
const KERNELS: [usize; 2] = [8, 16];
/// Sections aimed for: about one per this many seconds, and at most one per that many.
const SECONDS_PER_SECTION: f64 = 18.0;
const SECONDS_PER_SECTION_MOST: f64 = 12.0;
/// How far a novelty peak must rise above the novelty around it to be a boundary at all.
const MIN_RISE: f32 = 0.1;
/// Sections at most (a very long song's are merged into fewer).
pub(crate) const MAX_SECTIONS: usize = 32;
/// Units compared at most (about 16 minutes at 120 BPM): the matrices grow with the square.
pub(crate) const MAX_UNITS: usize = 2000;
/// How alike two sections must be (in spreads above the song's average pair) to share a group.
const SAME_GROUP: f32 = 0.9;
/// And how alike, at least, compared with how alike each is within itself.
const REPEAT_LIKENESS: f32 = 0.8;

/// A similarity matrix over units (symmetric, 1 on the diagonal).
pub(crate) struct Ssm {
    n: usize,
    values: Vec<f32>,
}

impl Ssm {
    /// Similarity from pairwise distance: exp(-d / median d), so a typical pair scores about 0.37.
    fn from_distance(n: usize, distance: impl Fn(usize, usize) -> f32) -> Self {
        let mut values = vec![0.0f32; n * n];
        for i in 0..n {
            for j in i + 1..n {
                let d = distance(i, j);
                values[i * n + j] = d;
                values[j * n + i] = d;
            }
        }
        let mut sample: Vec<f32> = (0..n)
            .flat_map(|i| (i + 1..n).step_by(1 + n / 200).map(move |j| (i, j)))
            .map(|(i, j)| values[i * n + j])
            .collect();
        let scale = if sample.is_empty() {
            1.0
        } else {
            let mid = sample.len() / 2;
            *sample.select_nth_unstable_by(mid, f32::total_cmp).1
        }
        .max(1e-6);
        for (k, v) in values.iter_mut().enumerate() {
            *v = if k / n == k % n { 1.0 } else { (-*v / scale).exp() };
        }
        Self { n, values }
    }

    /// By timbre and loudness (squared distance).
    pub fn timbre(synced: &Synced) -> Self {
        let t = &synced.timbre;
        Self::from_distance(t.len(), |i, j| {
            t[i].iter().zip(&t[j]).map(|(a, b)| (a - b).powi(2)).sum()
        })
    }

    /// By harmony (one minus the cosine of the chroma).
    pub fn chroma(synced: &Synced) -> Self {
        let c = &synced.chroma;
        let norm: Vec<f32> = c
            .iter()
            .map(|v| v.iter().map(|x| x * x).sum::<f32>().sqrt())
            .collect();
        Self::from_distance(c.len(), |i, j| {
            if norm[i] < 1e-6 || norm[j] < 1e-6 {
                return if norm[i] < 1e-6 && norm[j] < 1e-6 {
                    0.0
                } else {
                    1.0
                };
            }
            1.0 - c[i].iter().zip(&c[j]).map(|(a, b)| a * b).sum::<f32>() / (norm[i] * norm[j])
        })
    }

    pub fn len(&self) -> usize {
        self.n
    }

    pub fn at(&self, i: usize, j: usize) -> f32 {
        self.values[i * self.n + j]
    }

    /// The two matrices averaged.
    pub fn mean(a: &Ssm, b: &Ssm) -> Ssm {
        Ssm {
            n: a.n,
            values: a
                .values
                .iter()
                .zip(&b.values)
                .map(|(x, y)| 0.5 * (x + y))
                .collect(),
        }
    }
}

/// Foote novelty with a Gaussian-tapered checkerboard kernel `half` units each way: how much
/// more alike the units within the blocks before and after each unit are than the units across
/// them (each unit's likeness to itself left out). Near the ends only the part of the kernel
/// inside the song counts.
fn foote(ssm: &Ssm, half: usize) -> Vec<f32> {
    let n = ssm.len();
    let sigma = half as f32 / 2.0;
    // Taper by offset, -half..half.
    let taper: Vec<f32> = (0..2 * half)
        .map(|k| (-((k as f32 - half as f32 + 0.5) / sigma).powi(2) / 2.0).exp())
        .collect();
    let mut out = vec![0.0f32; n];
    for (i, slot) in out.iter_mut().enumerate() {
        let (lo, hi) = (half.saturating_sub(i), (n + half - i).min(2 * half));
        // A unit or two from an end there's too little on one side to compare.
        if half - lo < 2 || hi - half < 2 {
            continue;
        }
        // Weighted means within the blocks and across them.
        let (mut same, mut same_w, mut cross, mut cross_w) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for a in lo..hi {
            let x = i + a - half;
            for b in lo..hi {
                if a == b {
                    continue;
                }
                let w = taper[a] * taper[b];
                let s = w * ssm.at(x, i + b - half);
                if (a < half) == (b < half) {
                    (same, same_w) = (same + s, same_w + w);
                } else {
                    (cross, cross_w) = (cross + s, cross_w + w);
                }
            }
        }
        if cross_w > 0.0 {
            *slot = (same / same_w - cross / cross_w).max(0.0);
        }
    }
    out
}

/// How much the song changes at each unit: the timbre and harmony novelty at each kernel size,
/// summed (0 where nothing changes; a few tenths at a clear change of section).
pub(crate) fn novelty(timbre: &Ssm, chroma: &Ssm) -> Vec<f32> {
    let n = timbre.len();
    let mut total = vec![0.0f32; n];
    for ssm in [timbre, chroma] {
        for half in KERNELS {
            for (t, x) in total.iter_mut().zip(foote(ssm, half)) {
                *t += x;
            }
        }
    }
    total
}

/// Section boundaries (units where a section starts, after the first), from the novelty: peaks
/// ranked by how far they rise above the novelty around them, at least `min_units` apart, moved
/// to the nearest bar line (`bar_phase`: the first unit that starts a bar). About one section per
/// 18 s is aimed for, more where peaks stand out nearly as much, none where nothing changes.
pub(crate) fn boundaries(novelty: &[f32], grid: &Grid, bar_phase: usize, min_units: usize) -> Vec<usize> {
    let n = novelty.len();
    if n < 2 * min_units.max(1) {
        return Vec::new();
    }
    let sections = |seconds: f64| ((grid.end / seconds).round() as usize).clamp(1, MAX_SECTIONS);
    let (aim, most) = (
        sections(SECONDS_PER_SECTION) - 1,
        sections(SECONDS_PER_SECTION_MOST) - 1,
    );
    let reach = BEATS_PER_BAR;
    let context = 8 * BEATS_PER_BAR;
    // Peaks, with how far each rises above the mean around it.
    let mut peaks: Vec<(usize, f32)> = (1..n)
        .filter(|&i| {
            let (lo, hi) = (i.saturating_sub(reach), (i + reach + 1).min(n));
            novelty[lo..hi].iter().all(|&x| x <= novelty[i]) && novelty[i] > 0.0
        })
        .map(|i| {
            let (lo, hi) = (i.saturating_sub(context), (i + context + 1).min(n));
            let mean = novelty[lo..hi].iter().sum::<f32>() / (hi - lo) as f32;
            (i, novelty[i] - mean)
        })
        .filter(|&(_, rise)| rise >= MIN_RISE)
        .collect();
    peaks.sort_by(|a, b| b.1.total_cmp(&a.1));
    let snap = |i: usize| -> usize {
        let offset = (i + BEATS_PER_BAR - bar_phase % BEATS_PER_BAR) % BEATS_PER_BAR;
        let down = i - offset.min(i);
        let up = down + BEATS_PER_BAR;
        if offset <= BEATS_PER_BAR / 2 || up >= n {
            down
        } else {
            up
        }
    };
    let fits = |chosen: &[(usize, f32)], at: usize| {
        at >= min_units / 2
            && at + min_units / 2 <= n
            && chosen.iter().all(|&(c, _)| c.abs_diff(at) >= min_units)
    };
    let mut chosen: Vec<(usize, f32)> = Vec::new();
    for &(i, rise) in &peaks {
        if chosen.len() >= most {
            break;
        }
        // Past the aim, only peaks that stand out nearly as much as the typical one chosen (none,
        // for a song too short to aim for any).
        if chosen.len() >= aim {
            let mut rises: Vec<f32> = chosen.iter().map(|c| c.1).collect();
            rises.sort_by(f32::total_cmp);
            if rises
                .get(rises.len() / 2)
                .is_none_or(|&typical| rise < 0.7 * typical)
            {
                break;
            }
        }
        let at = snap(i);
        if fits(&chosen, at) {
            chosen.push((at, rise));
        }
    }
    let mut chosen: Vec<usize> = chosen.into_iter().map(|c| c.0).collect();
    chosen.sort_unstable();
    chosen
}

/// How alike two runs of units are: the best mean similarity along a diagonal of `ssm`, sliding
/// the shorter run along the longer a bar at a time, a little less for runs of different length.
fn alike(ssm: &Ssm, a: (usize, usize), b: (usize, usize)) -> f32 {
    let (long, short) = if a.1 - a.0 >= b.1 - b.0 { (a, b) } else { (b, a) };
    let (long_len, m) = (long.1 - long.0, short.1 - short.0);
    if m == 0 {
        return 0.0;
    }
    let slack = long_len - m;
    let offsets = (0..=slack).step_by(BEATS_PER_BAR).chain(std::iter::once(slack));
    let best = offsets
        .map(|off| (0..m).map(|k| ssm.at(long.0 + off + k, short.0 + k)).sum::<f32>() / m as f32)
        .fold(0.0, f32::max);
    best * (0.8 + 0.2 * m as f32 / long_len as f32)
}

/// Groups for runs of units (`spans`): a group number per span, numbered by first appearance,
/// and how sure each grouping is (0–1).
pub(crate) fn groups(ssm: &Ssm, spans: &[(usize, usize)]) -> (Vec<usize>, Vec<f32>) {
    let k = spans.len();
    let mut pair = vec![0.0f32; k * k];
    for i in 0..k {
        for j in i + 1..k {
            let s = alike(ssm, spans[i], spans[j]);
            pair[i * k + j] = s;
            pair[j * k + i] = s;
        }
    }
    // Pairs scored against the song's typical pair.
    let others: Vec<f32> = (0..k)
        .flat_map(|i| (i + 1..k).map(move |j| (i, j)))
        .map(|(i, j)| pair[i * k + j])
        .collect();
    let mean = others.iter().sum::<f32>() / others.len().max(1) as f32;
    let sd = (others.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / others.len().max(1) as f32)
        .sqrt()
        .max(0.02);
    // How alike each section is within itself: a repeat is about as alike to it.
    let within: Vec<f32> = spans
        .iter()
        .map(|&(a, b)| {
            let n = b - a;
            if n < 2 {
                return 1.0;
            }
            let sum: f32 = (a..b)
                .flat_map(|x| (a..b).filter(move |&y| y != x).map(move |y| (x, y)))
                .map(|(x, y)| ssm.at(x, y))
                .sum();
            sum / (n * (n - 1)) as f32
        })
        .collect();
    let z = |i: usize, j: usize| {
        let z = (pair[i * k + j] - mean) / sd;
        if pair[i * k + j] >= REPEAT_LIKENESS * 0.5 * (within[i] + within[j]) {
            z
        } else {
            z.min(0.0) - 1.0
        }
    };
    // Average-linkage clustering: join the most alike clusters while they're alike enough.
    let mut clusters: Vec<Vec<usize>> = (0..k).map(|i| vec![i]).collect();
    loop {
        let mut best: Option<(usize, usize, f32)> = None;
        for a in 0..clusters.len() {
            for b in a + 1..clusters.len() {
                let total: f32 = clusters[a]
                    .iter()
                    .flat_map(|&i| clusters[b].iter().map(move |&j| (i, j)))
                    .map(|(i, j)| z(i, j))
                    .sum();
                let link = total / (clusters[a].len() * clusters[b].len()) as f32;
                if best.is_none_or(|(_, _, l)| link > l) {
                    best = Some((a, b, link));
                }
            }
        }
        match best {
            Some((a, b, link)) if link >= SAME_GROUP => {
                let moved = clusters.remove(b);
                clusters[a].extend(moved);
            }
            _ => break,
        }
    }
    let mut group = vec![0usize; k];
    clusters.sort_by_key(|c| c.iter().copied().min().unwrap_or(0));
    for (g, members) in clusters.iter().enumerate() {
        for &m in members {
            group[m] = g;
        }
    }
    // Sureness: how far a section's grouping is from the threshold either way.
    let sure = (0..k)
        .map(|i| {
            let mates: Vec<f32> = (0..k)
                .filter(|&j| j != i && group[j] == group[i])
                .map(|j| z(i, j))
                .collect();
            let margin = if mates.is_empty() {
                // Alone: as sure as its most alike other section is unlike it.
                SAME_GROUP
                    - (0..k)
                        .filter(|&j| j != i)
                        .map(|j| z(i, j))
                        .fold(f32::MIN, f32::max)
            } else {
                mates.iter().sum::<f32>() / mates.len() as f32 - SAME_GROUP
            };
            if k == 1 {
                0.5
            } else {
                (0.5 + margin / 3.0).clamp(0.05, 0.95)
            }
        })
        .collect();
    (group, sure)
}

/// The letter for group `g`: A–Z, then AA, AB ….
pub(crate) fn letter(g: usize) -> String {
    let abc = |i: usize| char::from(b'A' + (i % 26) as u8);
    if g < 26 {
        abc(g).to_string()
    } else {
        format!("{}{}", abc(g / 26 - 1), abc(g))
    }
}

/// Friendly names for sections from their groups, energy (0–1), and place in the song, with how
/// sure each name is.
///
/// - One-offs before the first repeated section (early on) are the Intro, after the last (late
///   on) the Outro; so are the first and last sections when much quieter than the song.
/// - Of the repeated groups, the Chorus is the one louder than the sections next to it most
///   often (and louder overall, and repeated more); the Verse the most repeated other one, best
///   one that leads into the chorus; a Pre-Chorus a group always right before the chorus.
/// - Other sections: quiet ones are Breaks, one-offs late in the song Bridges, other one-offs
///   Interludes, and other repeats Parts.
pub(crate) fn names(group: &[usize], energy: &[f32], starts: &[f64], duration: f64) -> Vec<(String, f32)> {
    let k = group.len();
    if k == 0 {
        return Vec::new();
    }
    if k == 1 {
        return vec![("Whole song".into(), 1.0)];
    }
    let groups = group.iter().copied().max().unwrap_or(0) + 1;
    let count = |g: usize| group.iter().filter(|&&x| x == g).count();
    let place = |i: usize| starts[i] / duration.max(1e-9);
    let song_energy = energy.iter().sum::<f32>() / k as f32;
    let mut name: Vec<Option<(&str, f32)>> = vec![None; k];

    let first_repeat = (0..k).find(|&i| count(group[i]) >= 2);
    let last_repeat = (0..k).rev().find(|&i| count(group[i]) >= 2);
    let quieter = |i: usize| count(group[i]) == 1 || energy[i] < song_energy - 0.3;
    for (i, slot) in name.iter_mut().enumerate() {
        if i == 0 && quieter(0) || first_repeat.is_some_and(|f| i < f) && place(i) < 0.2 {
            *slot = Some(("Intro", 0.8));
        } else if i == k - 1 && quieter(i) || last_repeat.is_some_and(|l| i > l) && place(i) > 0.75 {
            *slot = Some(("Outro", 0.8));
        }
    }

    // Repeats in the body of the song.
    let ends: Vec<bool> = name.iter().map(Option::is_some).collect();
    let body = |i: usize| !ends[i];
    let body_count = |g: usize| (0..k).filter(|&i| group[i] == g && body(i)).count();
    let repeated: Vec<usize> = (0..groups).filter(|&g| body_count(g) >= 2).collect();
    let mean_energy = |g: usize| {
        let mine: Vec<f32> = (0..k)
            .filter(|&i| group[i] == g && body(i))
            .map(|i| energy[i])
            .collect();
        mine.iter().sum::<f32>() / mine.len().max(1) as f32
    };
    // How often a group's sections are louder than the repeated sections next to them.
    let wins = |g: usize| {
        let (mut won, mut lost) = (0, 0);
        for i in (0..k).filter(|&i| group[i] == g && body(i)) {
            for j in [i.wrapping_sub(1), i + 1] {
                if j >= k || !body(j) || group[j] == g || body_count(group[j]) < 2 {
                    continue;
                }
                if energy[i] > energy[j] + 0.005 {
                    won += 1;
                } else if energy[i] < energy[j] - 0.005 {
                    lost += 1;
                }
            }
        }
        if won + lost == 0 {
            0.5
        } else {
            won as f32 / (won + lost) as f32
        }
    };
    let score = |g: usize| wins(g) + mean_energy(g) + 0.05 * body_count(g) as f32;
    let chorus = repeated
        .iter()
        .copied()
        .max_by(|&a, &b| score(a).total_cmp(&score(b)));
    if let Some(chorus) = chorus {
        let leads_in = |g: usize| {
            (0..k)
                .filter(|&i| group[i] == g && body(i))
                .filter(|&i| (i + 1..(i + 3).min(k)).any(|j| group[j] == chorus))
                .count()
        };
        let verse = repeated
            .iter()
            .copied()
            .filter(|&g| g != chorus)
            .max_by_key(|&g| (body_count(g) + leads_in(g), leads_in(g)));
        let pre = repeated.iter().copied().find(|&g| {
            Some(g) != verse
                && g != chorus
                && (0..k)
                    .filter(|&i| group[i] == g && body(i))
                    .all(|i| group.get(i + 1) == Some(&chorus))
        });
        let chorus_sure = 0.4 + 0.4 * wins(chorus);
        for (i, slot) in name.iter_mut().enumerate() {
            if !body(i) {
                continue;
            }
            let g = Some(group[i]);
            *slot = if g == Some(chorus) {
                Some(("Chorus", chorus_sure))
            } else if g == verse {
                Some(("Verse", 0.6))
            } else if g == pre {
                Some(("Pre-Chorus", 0.5))
            } else {
                None
            };
        }
    }
    // One-offs and anything left: quiet ones are breaks, ones late in the song bridges.
    name.iter()
        .enumerate()
        .map(|(i, n)| {
            let (word, sure) = n.unwrap_or(if energy[i] < 0.3 || energy[i] < song_energy - 0.3 {
                ("Break", 0.5)
            } else if count(group[i]) == 1 && (0.45..0.85).contains(&place(i)) {
                ("Bridge", 0.5)
            } else if count(group[i]) == 1 {
                ("Interlude", 0.4)
            } else {
                ("Part", 0.4)
            });
            (word.to_string(), sure)
        })
        .collect()
}

/// The shortest section, in units.
pub(crate) fn min_section_units() -> usize {
    MIN_SECTION_BARS * BEATS_PER_BAR
}
