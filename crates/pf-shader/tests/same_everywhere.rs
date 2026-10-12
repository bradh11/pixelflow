//! The same bits on every system.
//!
//! Each number below was worked out once (on macOS, on an ARM processor) and must come out the
//! same wherever these tests run: Linux and Windows on x86 among them. If one doesn't, the
//! failure names what differs: one of the math functions, or the machine itself.

use pf_shader::{Limits, Machine, Program, math};

/// A fingerprint of a run of numbers (FNV-1a over their bits). What isn't a number has no one
/// bit pattern across processors, and comes out of a shader as no light whatever its bits, so
/// every such value counts as the same.
fn fingerprint(values: impl Iterator<Item = f32>) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for v in values {
        let bits = if v.is_nan() { 0x7fc0_0000 } else { v.to_bits() };
        for byte in bits.to_le_bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

/// Numbers from `lo` to `hi`, worked out by arithmetic that is the same everywhere.
fn spread(lo: f32, hi: f32) -> impl Iterator<Item = f32> {
    (0..=6000).map(move |i| lo + (hi - lo) * (i as f32 / 6000.0))
}

/// Every built-in function that leans on the math library, on numbers that differ pixel to pixel.
const EVERYTHING: &str = r#"
void main() {
    vec2 p = gl_FragCoord.xy / RENDERSIZE * 4.0 - 2.0;
    float a = sin(p.x * 3.0) + cos(p.y * 5.0) + tan(p.x * 0.3);
    float b = asin(clamp(p.x * 0.5, -1.0, 1.0)) + acos(clamp(p.y * 0.5, -1.0, 1.0)) + atan(p.y, p.x) + atan(p.x * 7.0);
    float c = exp(p.x) + log(abs(p.y) + 0.001) + exp2(p.y) + log2(abs(p.x) + 0.001)
        + pow(abs(p.x) + 0.1, p.y) + sqrt(abs(p.x)) + inversesqrt(abs(p.y) + 0.01);
    float d = sinh(p.x) + cosh(p.y) + tanh(p.x * p.y) + mod(p.x, 0.37) + fract(p.y * 9.0)
        + smoothstep(-1.0, 1.0, p.x) + mix(p.x, p.y, 0.3);
    vec3 n = normalize(vec3(p, 0.5));
    gl_FragColor = vec4(a + TIME, b, c, d + length(n.xy) + dot(n, vec3(0.2, 0.3, 0.4)));
}"#;

/// A march through a turning grid: loops that pixels leave at different turns.
const MARCH: &str = r#"
vec2 turn(vec2 v, float a) { return vec2(cos(a) * v.x - sin(a) * v.y, sin(a) * v.x + cos(a) * v.y); }
float cells(vec3 p) {
    float d = 100.0;
    float scale = 1.0;
    for (int k = 0; k < 3; k++) {
        p.xy = turn(p.xy, TIME * 0.5 + float(k));
        vec3 q = mod(p * scale, 2.0) - 1.0;
        d = min(d, (length(q) - 0.45) / scale);
        scale *= 1.7;
    }
    return d;
}
void main() {
    vec2 uv = (gl_FragCoord.xy / RENDERSIZE) * 2.0 - 1.0;
    vec3 dir = normalize(vec3(uv, 1.5));
    float gone = 0.0;
    for (int i = 0; i < 20; i++) {
        float d = cells(vec3(0.0, 0.0, TIME * 0.2) + dir * gone);
        if (d < 0.002) break;
        gone += d;
    }
    gl_FragColor = vec4(0.5 + 0.5 * cos(vec3(0.0, 2.0, 4.0) + gone * 0.6), 1.0);
}"#;

fn shader(source: &str) -> u64 {
    let program = Program::compile(source, &Limits::default()).expect("the shader compiles");
    let mut uniforms = program.uniforms();
    program.set_size(&mut uniforms, 64.0, 48.0);
    program.set_time(&mut uniforms, 1.25, 0.025, 50);
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    for y in 0..48 {
        for x in 0..64 {
            xs.push(x as f32 + 0.5);
            ys.push(y as f32 + 0.5);
        }
    }
    let mut out = vec![[0.0f32; 4]; xs.len()];
    let work = program.work_for(xs.len());
    Machine::new(&program, 37).run(&program, &uniforms, work, &xs, &ys, &mut out);
    fingerprint(out.into_iter().flatten())
}

#[test]
fn the_same_bits_on_every_system() {
    let found: Vec<(&str, u64)> = vec![
        ("sin", fingerprint(spread(-40.0, 40.0).map(math::sin))),
        ("cos", fingerprint(spread(-40.0, 40.0).map(math::cos))),
        ("tan", fingerprint(spread(-1.5, 1.5).map(math::tan))),
        ("asin", fingerprint(spread(-1.1, 1.1).map(math::asin))),
        ("acos", fingerprint(spread(-1.1, 1.1).map(math::acos))),
        ("atan", fingerprint(spread(-50.0, 50.0).map(math::atan))),
        (
            "atan2",
            fingerprint(spread(-3.0, 3.0).map(|y| math::atan2(y, 1.5 - y * y))),
        ),
        ("exp", fingerprint(spread(-110.0, 95.0).map(math::exp))),
        ("exp2", fingerprint(spread(-150.0, 130.0).map(math::exp2))),
        ("ln", fingerprint(spread(-1.0, 50.0).map(math::ln))),
        ("log2", fingerprint(spread(0.0, 5000.0).map(math::log2))),
        (
            "pow",
            fingerprint(spread(-0.5, 30.0).map(|x| math::pow(x, 2.5 - x * 0.3))),
        ),
        ("sinh", fingerprint(spread(-8.0, 8.0).map(math::sinh))),
        ("cosh", fingerprint(spread(-8.0, 8.0).map(math::cosh))),
        ("tanh", fingerprint(spread(-25.0, 25.0).map(math::tanh))),
        ("a shader of every function", shader(EVERYTHING)),
        ("a shader of loops", shader(MARCH)),
    ];
    let expected: [(&str, u64); 17] = [
        ("sin", 0x444c_a8c5_d2bf_371a),
        ("cos", 0x76fa_6872_5481_271a),
        ("tan", 0xe8d5_ad02_252d_e29d),
        ("asin", 0x2b97_6227_edcc_7056),
        ("acos", 0xa680_7d9b_c09a_8c28),
        ("atan", 0xe746_f910_53ad_6304),
        ("atan2", 0xd68a_f803_049f_914d),
        ("exp", 0x3df6_720f_0c2a_6fc7),
        ("exp2", 0xbb19_c611_9875_89c6),
        ("ln", 0x268e_132e_4089_5a1b),
        ("log2", 0x5d19_9028_443f_d866),
        ("pow", 0x74ed_52fe_88dc_f3e0),
        ("sinh", 0x7d7f_da17_ff17_858c),
        ("cosh", 0xc4f5_98d7_bb46_4c72),
        ("tanh", 0x0b71_d18c_5968_4352),
        ("a shader of every function", 0x649f_194a_a157_fe77),
        ("a shader of loops", 0x113d_9d28_5249_c179),
    ];
    let differ: Vec<String> = found
        .iter()
        .zip(&expected)
        .filter(|(f, e)| f != e)
        .map(|(f, _)| format!("(\"{}\", {:#018x}),", f.0, f.1))
        .collect();
    assert!(
        differ.is_empty(),
        "not the bits these were on the system they were first worked out on:\n{}",
        differ.join("\n")
    );
}
