//! Custom models: a grid (optionally layered) whose cells hold 1-based node numbers. Port of
//! `XmlSerialize::ParseCustomModel` / `ParseCompressed` and `CustomModel::InitCustomMatrix`.

use super::xform::boxed;
use super::{Ctx, MAX_LIGHTS, Raw, RawNode, V3, compute_string_start_node, strtol0};
use std::collections::{BTreeMap, HashMap};

/// An occupied cell: node number, row (0 = top), column, layer.
#[derive(Clone, Copy)]
struct Cell {
    value: i64,
    row: i64,
    col: i64,
    layer: i64,
}

/// `ParseCustomModel`: layers split by `|`, rows by `;`, columns by `,`; empty cells hold nothing.
fn parse_grid(s: &str) -> Vec<Cell> {
    let mut cells = Vec::new();
    for (layer, l) in s.split('|').enumerate() {
        for (row, r) in l.split(';').enumerate() {
            for (col, c) in r.split(',').enumerate() {
                let c = c.trim_start_matches(' ');
                if c.is_empty() {
                    continue;
                }
                let value = strtol0(c);
                if value > 0 {
                    cells.push(Cell {
                        value,
                        row: row as i64,
                        col: col as i64,
                        layer: layer as i64,
                    });
                }
            }
        }
    }
    cells
}

/// `ParseCompressed`: `node,row,col[,layer]` records separated by `;`. Returns the cells in grid
/// order (layer, row, col) with later duplicates of a cell winning, plus whether any record had
/// a negative position (which xLights cannot represent either).
fn parse_compressed(s: &str) -> (Vec<Cell>, bool) {
    let mut grid: BTreeMap<(i64, i64, i64), i64> = BTreeMap::new();
    let mut bad = false;
    for rec in s.split(';') {
        let f: Vec<i64> = rec.split(',').map(strtol0).collect();
        let (value, row, col, layer) = match f.len() {
            3 => (f[0], f[1], f[2], 0),
            4 => (f[0], f[1], f[2], f[3]),
            _ => continue,
        };
        if row < 0 || col < 0 || layer < 0 {
            bad = true;
            continue;
        }
        grid.insert((layer, row, col), value);
    }
    let cells = grid
        .into_iter()
        .filter(|&(_, v)| v > 0)
        .map(|((layer, row, col), value)| Cell {
            value,
            row,
            col,
            layer,
        })
        .collect();
    (cells, bad)
}

/// A custom model's occupied cells as `[node number, row, column, layer]`, read as xLights does
/// (the compressed form when there is one); `None` when some cells had negative positions.
pub(crate) fn custom_cells(grid: &str, compressed: &str) -> Option<Vec<[i64; 4]>> {
    let cells = if compressed.is_empty() {
        parse_grid(grid)
    } else {
        let (cells, bad) = parse_compressed(compressed);
        if bad {
            return None;
        }
        cells
    };
    Some(cells.iter().map(|c| [c.value, c.row, c.col, c.layer]).collect())
}

/// `CustomModel` (`InitCustomMatrix` and its `SetStringStartChannels` override).
pub(super) fn custom(cx: &mut Ctx) -> Raw {
    let depth = cx.int("Depth", 1).max(1);
    let strings = cx.int("CustomStrings", 1);
    let compressed = cx.text("CustomModelCompressed", "");
    let cells = if compressed.is_empty() {
        parse_grid(cx.text("CustomModel", ""))
    } else {
        let (cells, bad) = parse_compressed(compressed);
        if bad {
            cx.note("custom model cells with negative positions were skipped");
        }
        cells
    };
    if cx.over_cap(cells.len() as i64) || cells.is_empty() {
        return Raw::empty();
    }
    let cpn = cx.cpn;
    let max_val = cells.iter().map(|c| c.value).max().unwrap_or(0);
    let (mut min_r, mut max_r) = (i64::MAX, i64::MIN);
    let (mut min_c, mut max_c) = (i64::MAX, i64::MIN);
    let (mut min_l, mut max_l) = (i64::MAX, i64::MIN);
    for c in &cells {
        (min_r, max_r) = (min_r.min(c.row), max_r.max(c.row));
        (min_c, max_c) = (min_c.min(c.col), max_c.max(c.col));
        (min_l, max_l) = (min_l.min(c.layer), max_l.max(c.layer));
    }
    let center = |a: i64, b: i64| f64::from((a + b) as f32 / 2.0);
    let (cr, cc, cl) = (center(min_r, max_r), center(min_c, max_c), center(min_l, max_l));
    let mut by_node: HashMap<i64, Vec<V3>> = HashMap::new();
    let mut order: Vec<i64> = Vec::new();
    for c in &cells {
        let p = [c.col as f64 - cc, cr - c.row as f64, cl - c.layer as f64];
        by_node
            .entry(c.value)
            .or_insert_with(|| {
                order.push(c.value);
                Vec::new()
            })
            .push(p);
    }
    order.sort_unstable();
    let first = first_start_channel(cx, strings, max_val, order.len() as i64);
    let nodes = order
        .into_iter()
        .map(|v| RawNode::new(first + (v - 1) * cpn, by_node.remove(&v).unwrap_or_default()))
        .collect();
    let perspective = if depth > 1 { f64::from(0.1f32) } else { 0.0 };
    let xf = boxed(cx, perspective, [1.0; 3]);
    Raw { nodes, xf }
}

/// The lowest string start channel (`firstStartChan`), relative to the model start: every node's
/// channel is `firstStartChan + (number - 1) * cpn`.
fn first_start_channel(cx: &mut Ctx, strings: i64, max_val: i64, node_count: i64) -> i64 {
    let cpn = cx.cpn;
    let advanced = cx.int("Advanced", 0) != 0;
    if strings == 1 || strings <= 0 {
        let s0 = cx.string_starts(1, 0, &[]).first().copied().unwrap_or(0);
        if cx.single_node && max_val > 1 {
            let cps = if cx.single_channel { 1 } else { cpn };
            return s0.min(cps);
        }
        return s0;
    }
    if advanced {
        let cps = if cx.single_node {
            cx.default_cps(1)
        } else {
            max_val * cpn / strings
        };
        return cx.string_starts(strings, cps, &[]).into_iter().min().unwrap_or(0);
    }
    (0..strings.min(MAX_LIGHTS))
        .map(|i| {
            let explicit = cx
                .attr(&format!("NodeStart{}", i + 1))
                .filter(|v| !v.is_empty())
                .or_else(|| cx.attr(&format!("String{}", i + 1)))
                .map_or(0, strtol0);
            let mut node = if explicit == 0 {
                compute_string_start_node(i, strings, node_count)
            } else {
                explicit
            };
            if node > max_val {
                node = max_val;
            }
            (node - 1) * cpn
        })
        .min()
        .unwrap_or(0)
}
