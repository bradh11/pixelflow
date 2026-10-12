//! The functions a platform's math library answers a little differently from one system to the
//! next, worked out the same way everywhere.
//!
//! Each takes and gives a single-precision number and works in double precision from additions,
//! multiplications, divisions, and square roots alone: operations IEEE 754 fixes to the last
//! bit, in the order written here (Rust neither reorders them nor fuses a multiply with an add).
//! The series are the textbook ones, carried far enough that the double-precision answer is
//! good to about 1e-15, so the single-precision one is the nearest or the one beside it.
//!
//! Where GLSL leaves a result undefined (`pow` of a negative number, `asin` of 2) the answer is
//! NaN, which comes out of a shader as no light.

use std::f64::consts::{FRAC_2_PI, FRAC_PI_2, FRAC_PI_6, LN_2, LOG2_E, PI, SQRT_2};

/// π/2 in two parts: the first with its low bits clear, so a whole number of them is exact.
const PI_2_HIGH: f64 = 1.570_796_326_734_125_6;
const PI_2_LOW: f64 = 6.077_100_506_506_192e-11;
/// ln 2 in two parts, the same way.
const LN_2_HIGH: f64 = 0.693_147_180_369_123_8;
const LN_2_LOW: f64 = 1.908_214_929_270_587_7e-10;
const SQRT_3: f64 = 1.732_050_807_568_877_2;
/// tan(π/12): below it the arctangent's series settles quickly.
const TAN_PI_12: f64 = 0.267_949_192_431_122_7;

/// `x` less the nearest whole number of quarter turns (so within π/4 of 0), and that number.
#[inline]
fn quarter_turns(x: f64) -> (f64, i64) {
    let k = (x * FRAC_2_PI).round();
    ((x - k * PI_2_HIGH) - k * PI_2_LOW, k as i64)
}

/// The sine of `r`, within π/4 of 0.
#[inline]
fn sin_near(r: f64) -> f64 {
    let z = r * r;
    let p = 1.0 / 6_227_020_800.0 - z / 1_307_674_368_000.0;
    let p = -1.0 / 39_916_800.0 + z * p;
    let p = 1.0 / 362_880.0 + z * p;
    let p = -1.0 / 5_040.0 + z * p;
    let p = 1.0 / 120.0 + z * p;
    let p = -1.0 / 6.0 + z * p;
    r + r * z * p
}

/// The cosine of `r`, within π/4 of 0.
#[inline]
fn cos_near(r: f64) -> f64 {
    let z = r * r;
    let p = -1.0 / 87_178_291_200.0 + z / 20_922_789_888_000.0;
    let p = 1.0 / 479_001_600.0 + z * p;
    let p = -1.0 / 3_628_800.0 + z * p;
    let p = 1.0 / 40_320.0 + z * p;
    let p = -1.0 / 720.0 + z * p;
    let p = 1.0 / 24.0 + z * p;
    let p = -0.5 + z * p;
    1.0 + z * p
}

fn sin_cos(x: f64) -> (f64, f64) {
    if !x.is_finite() {
        return (f64::NAN, f64::NAN);
    }
    let (r, k) = quarter_turns(x);
    // An angle so large that the turns can't be taken off it exactly (single precision can't
    // tell one turn from the next there anyway) is answered as the quarter turn it is nearest.
    let r = if r.abs() <= 0.8 { r } else { 0.0 };
    let (s, c) = (sin_near(r), cos_near(r));
    match k & 3 {
        0 => (s, c),
        1 => (c, -s),
        2 => (-s, -c),
        _ => (-c, s),
    }
}

pub fn sin(x: f32) -> f32 {
    sin_cos(f64::from(x)).0 as f32
}

pub fn cos(x: f32) -> f32 {
    sin_cos(f64::from(x)).1 as f32
}

pub fn tan(x: f32) -> f32 {
    let (s, c) = sin_cos(f64::from(x));
    (s / c) as f32
}

/// 2 to the whole number `k`, from −1022 to 1023.
#[inline]
fn two_to(k: i64) -> f64 {
    f64::from_bits(((1023 + k.clamp(-1022, 1023)) as u64) << 52)
}

fn exp64(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    // Past these a single-precision answer is infinite or nothing.
    if x > 90.0 {
        return f64::INFINITY;
    }
    if x < -110.0 {
        return 0.0;
    }
    let k = (x * LOG2_E).round();
    let r = (x - k * LN_2_HIGH) - k * LN_2_LOW;
    // e to the r, r within half of ln 2 of 0.
    let mut p = 1.0 / 6_227_020_800.0;
    for divisor in [
        479_001_600.0,
        39_916_800.0,
        3_628_800.0,
        362_880.0,
        40_320.0,
        5_040.0,
        720.0,
        120.0,
        24.0,
        6.0,
        2.0,
        1.0,
        1.0,
    ] {
        p = 1.0 / divisor + r * p;
    }
    p * two_to(k as i64)
}

fn ln64(x: f64) -> f64 {
    if x.is_nan() || x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }
    if x.is_infinite() {
        return f64::INFINITY;
    }
    // x = m × 2^e with m from √½ to √2. (A single-precision number is never so small that the
    // double holding it is subnormal.)
    let bits = x.to_bits();
    let mut e = ((bits >> 52) & 0x7ff) as i64 - 1023;
    let mut m = f64::from_bits((bits & 0x000f_ffff_ffff_ffff) | (1023 << 52));
    if m > SQRT_2 {
        m *= 0.5;
        e += 1;
    }
    // ln m = 2 atanh s, s = (m − 1) / (m + 1), at most 0.172 from 0.
    let s = (m - 1.0) / (m + 1.0);
    let z = s * s;
    let mut p = 1.0 / 21.0;
    for odd in [19.0, 17.0, 15.0, 13.0, 11.0, 9.0, 7.0, 5.0, 3.0, 1.0] {
        p = 1.0 / odd + z * p;
    }
    let e = e as f64;
    e * LN_2_HIGH + (2.0 * s * p + e * LN_2_LOW)
}

pub fn exp(x: f32) -> f32 {
    exp64(f64::from(x)) as f32
}

pub fn exp2(x: f32) -> f32 {
    exp64(f64::from(x) * LN_2) as f32
}

pub fn ln(x: f32) -> f32 {
    ln64(f64::from(x)) as f32
}

pub fn log2(x: f32) -> f32 {
    (ln64(f64::from(x)) * LOG2_E) as f32
}

/// `x` to the `y`. GLSL leaves it undefined for a negative `x`; here that is NaN.
pub fn pow(x: f32, y: f32) -> f32 {
    if x.is_nan() || y.is_nan() {
        return f32::NAN;
    }
    if y == 0.0 || x == 1.0 {
        return 1.0;
    }
    if x < 0.0 {
        return f32::NAN;
    }
    if x == 0.0 {
        return if y > 0.0 { 0.0 } else { f32::INFINITY };
    }
    exp64(f64::from(y) * ln64(f64::from(x))) as f32
}

/// The arctangent of `t`, from 0 to 1.
fn atan_unit(t: f64) -> f64 {
    // Past tan(π/12), turn it back by π/6 first.
    let (t, turned) = if t > TAN_PI_12 {
        ((t * SQRT_3 - 1.0) / (SQRT_3 + t), FRAC_PI_6)
    } else {
        (t, 0.0)
    };
    let z = t * t;
    let mut p = 1.0 / 29.0;
    for odd in [
        27.0, 25.0, 23.0, 21.0, 19.0, 17.0, 15.0, 13.0, 11.0, 9.0, 7.0, 5.0, 3.0, 1.0,
    ] {
        p = 1.0 / odd - z * p;
    }
    turned + t * p
}

fn atan64(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    let a = x.abs();
    let angle = if a > 1.0 {
        FRAC_PI_2 - atan_unit(1.0 / a)
    } else {
        atan_unit(a)
    };
    if x < 0.0 { -angle } else { angle }
}

fn atan2_64(y: f64, x: f64) -> f64 {
    if x.is_nan() || y.is_nan() {
        return f64::NAN;
    }
    let (y, x) = if x.is_infinite() && y.is_infinite() {
        (y.signum(), x.signum())
    } else {
        (y, x)
    };
    if x > 0.0 {
        atan64(y / x)
    } else if x < 0.0 {
        if y >= 0.0 {
            atan64(y / x) + PI
        } else {
            atan64(y / x) - PI
        }
    } else if y > 0.0 {
        FRAC_PI_2
    } else if y < 0.0 {
        -FRAC_PI_2
    } else {
        0.0
    }
}

pub fn atan(x: f32) -> f32 {
    atan64(f64::from(x)) as f32
}

/// The angle of the point (x, y): GLSL's `atan(y, x)`.
pub fn atan2(y: f32, x: f32) -> f32 {
    atan2_64(f64::from(y), f64::from(x)) as f32
}

pub fn asin(x: f32) -> f32 {
    let x = f64::from(x);
    if !(-1.0..=1.0).contains(&x) {
        return f32::NAN;
    }
    atan2_64(x, ((1.0 - x) * (1.0 + x)).sqrt()) as f32
}

pub fn acos(x: f32) -> f32 {
    let x = f64::from(x);
    if !(-1.0..=1.0).contains(&x) {
        return f32::NAN;
    }
    atan2_64(((1.0 - x) * (1.0 + x)).sqrt(), x) as f32
}

pub fn sinh(x: f32) -> f32 {
    let x = f64::from(x);
    if x.abs() < 1e-5 {
        return (x + x * x * x / 6.0) as f32;
    }
    ((exp64(x) - exp64(-x)) * 0.5) as f32
}

pub fn cosh(x: f32) -> f32 {
    let x = f64::from(x);
    ((exp64(x) + exp64(-x)) * 0.5) as f32
}

pub fn tanh(x: f32) -> f32 {
    let x = f64::from(x);
    if x.is_nan() {
        return f32::NAN;
    }
    if x.abs() > 20.0 {
        return if x > 0.0 { 1.0 } else { -1.0 };
    }
    if x.abs() < 1e-5 {
        return (x - x * x * x / 3.0) as f32;
    }
    let e = exp64(2.0 * x);
    ((e - 1.0) / (e + 1.0)) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How many single-precision numbers apart two answers are.
    fn apart(a: f32, b: f32) -> u32 {
        if a == b || (a.is_nan() && b.is_nan()) {
            return 0;
        }
        if a.is_nan() != b.is_nan() || a.is_infinite() || b.is_infinite() {
            return u32::MAX;
        }
        let key = |x: f32| {
            let bits = x.to_bits() as i32;
            i64::from(if bits < 0 {
                i32::MIN.wrapping_sub(bits)
            } else {
                bits
            })
        };
        (key(a) - key(b)).unsigned_abs().min(u64::from(u32::MAX)) as u32
    }

    /// A spread of numbers from `lo` to `hi`, the ends and many between.
    fn spread(lo: f32, hi: f32) -> impl Iterator<Item = f32> {
        (0..=20_000).map(move |i| lo + (hi - lo) * (i as f32 / 20_000.0))
    }

    /// The most `ours` differs over `inputs` from `exact`, worked out in double precision and
    /// rounded once to single: as near the true answer as single precision holds, and the same on
    /// every platform (the platforms' own single-precision functions differ from one another by a
    /// step here and there).
    fn worst(inputs: impl Iterator<Item = f32>, ours: fn(f32) -> f32, exact: fn(f64) -> f64) -> u32 {
        inputs
            .map(|x| apart(ours(x), exact(f64::from(x)) as f32))
            .max()
            .unwrap_or(0)
    }

    #[test]
    fn each_function_is_within_a_step_of_the_true_answer() {
        assert!(worst(spread(-40.0, 40.0), sin, f64::sin) <= 1);
        assert!(worst(spread(-40.0, 40.0), cos, f64::cos) <= 1);
        assert!(worst(spread(-1.5, 1.5), tan, f64::tan) <= 1);
        assert!(worst(spread(-30.0, 30.0), exp, f64::exp) <= 1);
        assert!(worst(spread(-20.0, 20.0), exp2, f64::exp2) <= 1);
        assert!(worst(spread(1e-6, 50.0), ln, f64::ln) <= 1);
        assert!(worst(spread(1e-6, 5000.0), log2, f64::log2) <= 1);
        assert!(worst(spread(-50.0, 50.0), atan, f64::atan) <= 1);
        assert!(worst(spread(-1.0, 1.0), asin, f64::asin) <= 2);
        assert!(worst(spread(-1.0, 1.0), acos, f64::acos) <= 2);
        assert!(worst(spread(-8.0, 8.0), sinh, f64::sinh) <= 1);
        assert!(worst(spread(-8.0, 8.0), cosh, f64::cosh) <= 1);
        assert!(worst(spread(-8.0, 8.0), tanh, f64::tanh) <= 1);
        let mut far = 0;
        for x in spread(0.01, 30.0).step_by(40) {
            for y in spread(-6.0, 6.0).step_by(160) {
                let (wx, wy) = (f64::from(x), f64::from(y));
                far = far.max(apart(pow(x, y), wx.powf(wy) as f32));
                far = far.max(apart(atan2(y, x - 15.0), wy.atan2(f64::from(x - 15.0)) as f32));
            }
        }
        assert!(far <= 1, "{far}");
    }

    #[test]
    fn the_edges_are_settled() {
        assert_eq!(sin(0.0), 0.0);
        assert_eq!(cos(0.0), 1.0);
        assert!(sin(f32::INFINITY).is_nan() && cos(f32::NAN).is_nan());
        assert_eq!(exp(0.0), 1.0);
        assert_eq!(exp(1000.0), f32::INFINITY);
        assert_eq!(exp(-1000.0), 0.0);
        assert_eq!(ln(1.0), 0.0);
        assert_eq!(ln(0.0), f32::NEG_INFINITY);
        assert!(ln(-1.0).is_nan());
        assert_eq!(log2(8.0), 3.0);
        assert_eq!(exp2(10.0), 1024.0);
        assert_eq!(pow(2.0, 10.0), 1024.0);
        assert_eq!(pow(0.0, 2.0), 0.0);
        assert_eq!(pow(5.0, 0.0), 1.0);
        assert!(pow(-2.0, 2.0).is_nan());
        assert_eq!(atan2(0.0, 0.0), 0.0);
        assert_eq!(atan2(1.0, 0.0), std::f32::consts::FRAC_PI_2);
        assert_eq!(atan2(0.0, -1.0), std::f32::consts::PI);
        assert!(asin(2.0).is_nan() && acos(-2.0).is_nan());
        assert_eq!(tanh(100.0), 1.0);
        // Very large angles are answered, if not well: no hang, no panic.
        assert!(sin(1e30).is_finite() && cos(f32::MAX).is_finite());
    }
}
