//! Screen-location transforms (local model space to layout coordinates) as xLights applies them
//! when drawing the 2D layout: `BoxedScreenLocation::ApplyModelViewMatrices`,
//! `TwoPointScreenLocation`/`ThreePointScreenLocation::PrepareToDraw` and
//! `PolyPointScreenLocation::PrepareToDraw`, plus the `VectorMath` rotation helpers.

use super::{Ctx, V3};
use std::f64::consts::PI;

/// `out = m * v + t` with `m` row-major.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Affine {
    pub m: [[f64; 3]; 3],
    pub t: V3,
}

impl Affine {
    pub const IDENTITY: Affine = Affine {
        m: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        t: [0.0; 3],
    };

    pub fn translate(t: V3) -> Self {
        Affine { t, ..Self::IDENTITY }
    }

    pub fn scale(s: V3) -> Self {
        Affine {
            m: [[s[0], 0.0, 0.0], [0.0, s[1], 0.0], [0.0, 0.0, s[2]]],
            t: [0.0; 3],
        }
    }

    /// `glm::rotate(angle, axis)`: right-handed rotation about the normalized axis.
    pub fn rotate(angle: f64, axis: V3) -> Self {
        let len = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
        if len.is_nan() || len <= 0.0 || !angle.is_finite() {
            return Self::IDENTITY;
        }
        let [x, y, z] = [axis[0] / len, axis[1] / len, axis[2] / len];
        let (s, c) = angle.sin_cos();
        let t = 1.0 - c;
        Affine {
            m: [
                [c + t * x * x, t * x * y - s * z, t * x * z + s * y],
                [t * x * y + s * z, c + t * y * y, t * y * z - s * x],
                [t * x * z - s * y, t * y * z + s * x, c + t * z * z],
            ],
            t: [0.0; 3],
        }
    }

    pub fn rot_x(angle: f64) -> Self {
        Self::rotate(angle, [1.0, 0.0, 0.0])
    }

    pub fn rot_y(angle: f64) -> Self {
        Self::rotate(angle, [0.0, 1.0, 0.0])
    }

    pub fn rot_z(angle: f64) -> Self {
        Self::rotate(angle, [0.0, 0.0, 1.0])
    }

    /// `glm::shearY(mat3(1), s)` promoted to 3D: `y += s * x`.
    pub fn shear_y(s: f64) -> Self {
        let mut a = Self::IDENTITY;
        a.m[1][0] = s;
        a
    }

    /// `self * other`: applies `other` first.
    pub fn then(&self, other: &Affine) -> Affine {
        let mut m = [[0.0; 3]; 3];
        for (i, row) in m.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v = (0..3).map(|k| self.m[i][k] * other.m[k][j]).sum();
            }
        }
        let ot = self.apply_linear(other.t);
        Affine {
            m,
            t: [ot[0] + self.t[0], ot[1] + self.t[1], ot[2] + self.t[2]],
        }
    }

    fn apply_linear(&self, v: V3) -> V3 {
        let r = |i: usize| self.m[i][0] * v[0] + self.m[i][1] * v[1] + self.m[i][2] * v[2];
        [r(0), r(1), r(2)]
    }

    pub fn apply(&self, v: V3) -> V3 {
        let l = self.apply_linear(v);
        [l[0] + self.t[0], l[1] + self.t[1], l[2] + self.t[2]]
    }
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn length(a: V3) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

/// `VectorMath::rotationMatrixFromXAxisToVector` (note its near-axis snapping thresholds).
pub(crate) fn rot_from_x_axis(a: V3) -> Affine {
    let len = length(a);
    if len.is_nan() || len <= 0.0 {
        return Affine::IDENTITY;
    }
    let ax = a[0] / len;
    let angle = ax.clamp(-1.0, 1.0).acos();
    if ax > 0.9999 || angle == 0.0 {
        Affine::IDENTITY
    } else if ax < -0.9999 || (angle - PI).abs() < 0.0001 {
        Affine::rot_y(PI)
    } else {
        Affine::rotate(angle, [0.0, -a[2], a[1]])
    }
}

/// `VectorMath::rotationMatrixFromXAxisToVector2`.
fn rot_from_x_axis2(o: V3, p: V3) -> Affine {
    let a = sub(p, o);
    if o[1] != p[1] || o[2] != p[2] {
        let len = length(a);
        let angle = if len == 0.0 {
            0.0
        } else {
            (a[0] / len).clamp(-1.0, 1.0).acos()
        };
        Affine::rotate(angle, [0.0, -a[2], a[1]])
    } else if p[0] < o[0] {
        Affine::rot_z(PI)
    } else {
        Affine::IDENTITY
    }
}

/// The value of pi xLights' graphics contexts use when converting degrees to radians.
#[allow(clippy::approx_constant)] // deliberately xLights' truncated pi, not std's
const CTX_PI: f64 = 3.14159;

/// xLights' degrees-to-radians in the graphics context (`* 3.14159 / 180`).
fn ctx_radians(deg: f64) -> f64 {
    deg * CTX_PI / 180.0
}

/// Boxed models (center-based): `T(WorldPos) * Rz * Ry * Rx * S * RotX(perspective)`, the order
/// `BoxedScreenLocation::ApplyModelViewMatrices` uses for the 2D view. `scale_mul` lets the
/// sphere apply its pre-version-8 rescale before `Init` clamps negative scales.
pub(super) fn boxed(cx: &Ctx, perspective: f64, scale_mul: V3) -> Affine {
    let pos = cx.world_pos();
    let sc = |k: &str, i: usize| {
        let v = cx.float(k, 1.0) * scale_mul[i];
        if v < 0.0 || !v.is_finite() { 1.0 } else { v }
    };
    let rot = |k: &str| {
        let v = cx.float(k, 0.0);
        if (-180.0..=180.0).contains(&v) { v } else { 0.0 }
    };
    let s = [sc("ScaleX", 0), sc("ScaleY", 1), sc("ScaleZ", 2)];
    let m = Affine::translate(pos)
        .then(&Affine::rot_z(ctx_radians(rot("RotateZ"))))
        .then(&Affine::rot_y(ctx_radians(rot("RotateY"))))
        .then(&Affine::rot_x(ctx_radians(rot("RotateX"))))
        .then(&Affine::scale(s));
    if perspective != 0.0 && !cx.upright {
        m.then(&Affine::rot_x(perspective))
    } else {
        m
    }
}

/// Point 2 of a two/three-point model (`X2/Y2/Z2` are offsets from `WorldPos`), with xLights'
/// nudge for a zero-length line, plus the raw `X2`.
fn point2(cx: &Ctx, pos: V3) -> (V3, f64) {
    let (x2, y2, z2) = (cx.float("X2", 0.0), cx.float("Y2", 0.0), cx.float("Z2", 0.0));
    let x = if x2 == 0.0 && y2 == 0.0 && z2 == 0.0 {
        0.001
    } else {
        x2
    };
    ([pos[0] + x, pos[1] + y2, pos[2] + z2], x2)
}

fn sane_render(cx: &mut Ctx, rw: f64) -> f64 {
    if rw > 0.0 && rw.is_finite() {
        rw
    } else {
        cx.note("model has no usable size; positions may be off");
        1.0
    }
}

/// Two-point models (Single Line, Channel Block): local x in `[0, render_wi]` runs from point 1
/// to point 2.
pub(super) fn two_point(cx: &mut Ctx, render_wi: f64) -> Affine {
    let rw = sane_render(cx, render_wi);
    let pos = cx.world_pos();
    let (p2, _) = point2(cx, pos);
    let s = length(sub(p2, pos)) / rw;
    Affine::translate(pos)
        .then(&rot_from_x_axis2(pos, p2))
        .then(&Affine::scale([s, s, s]))
}

/// Three-point models (Arches, Candy Canes, Icicles). `handle_height` models apply `Height`
/// inside their node generator; the others scale local y by it. Only Icicles support shear.
pub(super) fn three_point(cx: &mut Ctx, render_wi: f64, handle_height: bool, shear: bool) -> Affine {
    let rw = sane_render(cx, render_wi);
    let pos = cx.world_pos();
    let (p2, x2) = point2(cx, pos);
    let swapped = x2 < 0.0;
    let a = if swapped { sub(pos, p2) } else { sub(p2, pos) };
    let s = length(a) / rw;
    let mut rot = rot_from_x_axis(a);
    if swapped {
        rot = rot.then(&Affine::rot_y(PI));
    }
    let height = cx.float("Height", 1.0);
    let sy = if handle_height { s } else { s * height };
    let sh = if shear {
        Affine::shear_y(cx.float("Shear", 0.0))
    } else {
        Affine::IDENTITY
    };
    Affine::translate(pos)
        .then(&rot)
        .then(&Affine::rot_x(cx.float("RotateX", 0.0).to_radians()))
        .then(&sh)
        .then(&Affine::scale([s, sy, s]))
}

/// Min/max of point data with xLights' quirky seeds (min from 100000, max from 0), as used by
/// both the poly-point models and their screen location.
pub(super) fn quirky_bounds(pts: &[V3]) -> (V3, V3) {
    let mut lo = [100000.0f64; 3];
    let mut hi = [0.0f64; 3];
    for p in pts {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    (lo, hi)
}

/// Poly-point models (Poly Line, MultiPoint): local coordinates are normalized 0..1 over the
/// point bounds; `PolyPointScreenLocation::PrepareToDraw` maps them back with `main_matrix`.
pub(super) fn poly_point(cx: &Ctx, pts: &[V3], render_ht: f64) -> Affine {
    let pos = cx.world_pos();
    let sc = |k: &str| {
        let v = cx.float(k, 1.0);
        if v <= 0.0 || !v.is_finite() { 1.0 } else { v }
    };
    let s = [sc("ScaleX"), sc("ScaleY"), sc("ScaleZ")];
    let (lo, hi) = quirky_bounds(pts);
    let mut yscale = (hi[1] - lo[1]) * s[1];
    if render_ht > 1.0 && hi[1] - lo[1] < render_ht {
        yscale = render_ht;
    }
    let t = [
        lo[0] * s[0] + pos[0],
        lo[1] * s[1] + pos[1],
        lo[2] * s[2] + pos[2],
    ];
    Affine::translate(t).then(&Affine::scale([
        (hi[0] - lo[0]) * s[0],
        yscale,
        (hi[2] - lo[2]) * s[2],
    ]))
}
