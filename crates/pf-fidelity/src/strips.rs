//! Frames side by side, for looking into a difference: a row of xLights' frames above the same
//! frames from PixelFlow, each drawn on the target's own grid.
//!
//! Both rows place pixels where PixelFlow's layout has them, so a picture that is right in
//! xLights' row but turned or scrambled says the layout differs, and one that is right in shape
//! but different in content says the effect does.

use crate::FidelityError;
use crate::compare::Prepared;
use image::{Rgb, RgbImage};
use pf_model::Show;
use pf_sequence::Target;

/// Pixels between frames and around the picture.
const GAP: u32 = 4;
/// What shows between pixels and where a side has no data.
const BACKGROUND: Rgb<u8> = Rgb([44, 44, 52]);

/// The prop, group, or submodel (`Prop/Submodel`) called `name`.
pub fn find_target(show: &Show, name: &str) -> Option<Target> {
    if let Some(prop) = show.props.iter().find(|p| p.name == name) {
        return Some(Target::Prop(prop.id));
    }
    if let Some(group) = show.groups.iter().find(|g| g.name == name) {
        return Some(Target::Group(group.id));
    }
    let (prop, region) = name.split_once('/')?;
    let prop = show.props.iter().find(|p| p.name == prop)?;
    let region = prop.regions.iter().find(|r| r.name == region)?;
    Some(Target::Region {
        prop: prop.id,
        region: region.id,
    })
}

/// `count` frames spread evenly over the scorecard's effect `index`.
pub fn effect_frames(prepared: &Prepared, index: usize, count: u32) -> Option<Vec<u32>> {
    let effect = prepared.effects().get(index)?;
    let frame_ms = u64::from(prepared.sequence().frame_ms.max(1));
    let first = effect.start_ms.div_ceil(frame_ms);
    let last = effect.end_ms.div_ceil(frame_ms).saturating_sub(1).max(first);
    let count = u64::from(count.max(1));
    Some(
        (0..count)
            .map(|k| {
                let at = if count == 1 {
                    first
                } else {
                    first + (last - first) * k / (count - 1)
                };
                at as u32
            })
            .collect(),
    )
}

/// One frame of a target, in numbers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Level {
    pub frame: u32,
    /// Mean red, green, and blue over the target's measured pixels.
    pub pixelflow: [f64; 3],
    pub xlights: [f64; 3],
    /// Pixels with any channel above 24.
    pub lit_pixelflow: usize,
    pub lit_xlights: usize,
    /// Mean absolute error per channel.
    pub error: f64,
}

/// The same frames [`draw`] shows, as numbers: how bright and what color each side is.
pub fn levels(prepared: &mut Prepared, target: Target, frames: &[u32]) -> Result<Vec<Level>, FidelityError> {
    let buffer = prepared.geometry().buffer(target);
    let pixels: Vec<usize> = buffer
        .show_pixels()
        .iter()
        .map(|&g| g as usize)
        .filter(|&g| prepared.measured(g))
        .collect();
    let count = prepared.pixel_count();
    let (mut ours, mut theirs) = (vec![[0u8; 3]; count], vec![[0u8; 3]; count]);
    let mut out = Vec::new();
    for &frame in frames {
        if pixels.is_empty() || !prepared.read_frame(frame, &mut ours, &mut theirs)? {
            continue;
        }
        let n = pixels.len() as f64;
        let mean = |colors: &[[u8; 3]]| {
            let mut sum = [0f64; 3];
            for &g in &pixels {
                for (total, &c) in sum.iter_mut().zip(&colors[g]) {
                    *total += f64::from(c);
                }
            }
            sum.map(|c| c / n)
        };
        let lit = |colors: &[[u8; 3]]| {
            pixels
                .iter()
                .filter(|&&g| colors[g].iter().any(|&c| c > 24))
                .count()
        };
        let error: f64 = pixels
            .iter()
            .flat_map(|&g| ours[g].into_iter().zip(theirs[g]))
            .map(|(a, b)| f64::from(a.abs_diff(b)))
            .sum();
        out.push(Level {
            frame,
            pixelflow: mean(&ours),
            xlights: mean(&theirs),
            lit_pixelflow: lit(&ours),
            lit_xlights: lit(&theirs),
            error: error / n / 3.0,
        });
    }
    Ok(out)
}

/// Draws `frames` of `target`: xLights' above, PixelFlow's below, each cell `scale` pixels wide
/// (0: sized to fit).
pub fn draw(
    prepared: &mut Prepared,
    target: Target,
    frames: &[u32],
    scale: u32,
) -> Result<RgbImage, FidelityError> {
    let buffer = prepared.geometry().buffer(target);
    if buffer.is_empty() || frames.is_empty() {
        return Err(FidelityError::Strip(
            "There is nothing to draw: the target has no pixels, or no frames were asked for.".to_string(),
        ));
    }
    let (columns, rows) = (buffer.columns.max(1), buffer.rows.max(1));
    let scale = if scale == 0 {
        (240 / columns.max(rows)).clamp(1, 12)
    } else {
        scale
    };
    // Each pixel's cell (column from the left, row from the top) and place in the show.
    let cell = |at: f32, cells: u32| (at.clamp(0.0, 1.0) * (cells - 1) as f32).round() as u32;
    let cells: Vec<(u32, u32, usize)> = buffer
        .pixels
        .iter()
        .zip(buffer.show_pixels())
        .map(|(px, &g)| (cell(px.u, columns), rows - 1 - cell(px.v, rows), g as usize))
        .filter(|&(_, _, g)| prepared.measured(g))
        .collect();
    let (tile_w, tile_h) = (columns * scale, rows * scale);
    let width = GAP + frames.len() as u32 * (tile_w + GAP);
    let height = GAP + 2 * (tile_h + GAP);
    let mut image = RgbImage::from_pixel(width, height, BACKGROUND);
    let count = prepared.pixel_count();
    let (mut ours, mut theirs) = (vec![[0u8; 3]; count], vec![[0u8; 3]; count]);
    for (n, &frame) in frames.iter().enumerate() {
        if !prepared.read_frame(frame, &mut ours, &mut theirs)? {
            continue;
        }
        let left = GAP + n as u32 * (tile_w + GAP);
        for (row, colors) in [&theirs, &ours].into_iter().enumerate() {
            let top = GAP + row as u32 * (tile_h + GAP);
            for &(column, line, g) in &cells {
                for y in 0..scale {
                    for x in 0..scale {
                        image.put_pixel(left + column * scale + x, top + line * scale + y, Rgb(colors[g]));
                    }
                }
            }
        }
    }
    Ok(image)
}
