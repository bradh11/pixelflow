//! Two rules of timing carried over from xLights, kept apart from the effects that use them
//! until they are replaced: where an effect is in its cycle (`GetEffectTimeIntervalPosition`),
//! and its acceleration (`RenderBuffer::calcAccel`).

/// Where an effect on its `frame`-th frame of `frames` is in its current cycle (0–1), with
/// `cycles` cycles over the effect.
pub(crate) fn cycle_position(frame: u64, frames: u64, cycles: f64) -> f64 {
    let periods = frames as f64;
    if periods <= 1.0 {
        return 0.0;
    }
    let per_cycle = periods / cycles;
    if per_cycle.is_nan() || per_cycle <= 1.0 || per_cycle.is_infinite() {
        return 0.0;
    }
    let at = (frame as f64).rem_euclid(per_cycle);
    (at / (per_cycle - 1.0)).min(1.0)
}

/// Progress through an effect (0–1) bent so it speeds up (positive, up to 10) or slows down
/// (negative).
pub(crate) fn accelerate(ratio: f64, accel: f64) -> f64 {
    if accel == 0.0 || !accel.is_finite() {
        return ratio;
    }
    let pct = (accel.abs() - 1.0) / 9.0;
    let a1 = pct * 5.0 + (1.0 - pct) * 1.5;
    let a2 = 1.5 + ratio * a1;
    let exponent = pct * a2 + (1.0 - pct) * a1;
    if accel > 0.0 {
        ratio.powf(exponent)
    } else {
        1.0 - (1.0 - ratio).powf(a1)
    }
}
