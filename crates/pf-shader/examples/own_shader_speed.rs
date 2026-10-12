//! Spike: how the batch machine compares with the same shader compiled ahead of time.
//!
//! A shader written for this (marching a ray through a turning grid of spheres), run three
//! ways on every cell of a 206 by 175 picture: by the machine, a pixel at a time and in batches;
//! as the same arithmetic written out in Rust and compiled, one pixel after another; and that
//! Rust with what doesn't depend on the pixel worked out once a frame by hand. The compiled
//! versions are what compiling a shader further could buy at best. All three must give the same
//! bits.
//!
//! ```sh
//! cargo run --release -p pf-shader --example own_shader_speed
//! ```

use pf_shader::{Limits, Machine, Program, math};
use std::time::Instant;

const SHADER: &str = r#"/*{ "INPUTS": [ { "NAME": "spin", "TYPE": "float", "MIN": 0.0, "MAX": 2.0, "DEFAULT": 0.5 } ] }*/
const int STEPS = 24;
vec2 turn(vec2 v, float a) {
    return vec2(cos(a) * v.x - sin(a) * v.y, sin(a) * v.x + cos(a) * v.y);
}
float cells(vec3 p) {
    float d = 100.0;
    float scale = 1.0;
    for (int k = 0; k < 3; k++) {
        p.xy = turn(p.xy, TIME * spin + float(k));
        p.yz = turn(p.yz, 0.7 * float(k) + 0.3);
        vec3 q = mod(p * scale, 2.0) - 1.0;
        d = min(d, (length(q) - 0.45) / scale);
        scale *= 1.7;
    }
    return d;
}
void main() {
    vec2 uv = (gl_FragCoord.xy / RENDERSIZE) * 2.0 - 1.0;
    vec3 from = vec3(0.0, 0.0, TIME * 0.2);
    vec3 dir = normalize(vec3(uv, 1.5));
    float gone = 0.0;
    float steps = 0.0;
    for (int i = 0; i < STEPS; i++) {
        float d = cells(from + dir * gone);
        if (d < 0.002) break;
        gone += d;
        steps += 1.0;
    }
    float glow = 1.0 - steps / float(STEPS);
    vec3 tint = 0.5 + 0.5 * cos(vec3(0.0, 2.0, 4.0) + gone * 0.6);
    gl_FragColor = vec4(tint * glow, 1.0);
}"#;

const COLUMNS: usize = 206;
const ROWS: usize = 175;
const TIME: f32 = 3.25;
const SPIN: f32 = 0.5;

fn glsl_mod(x: f32, y: f32) -> f32 {
    x - y * (x / y).floor()
}

fn glsl_min(x: f32, y: f32) -> f32 {
    if y < x { y } else { x }
}

/// The sines and cosines of the two turns of each of the three rounds.
type Turns = [[(f32, f32); 2]; 3];

fn turns() -> Turns {
    let mut out = [[(0.0, 0.0); 2]; 3];
    for (k, round) in out.iter_mut().enumerate() {
        let a = TIME * SPIN + k as f32;
        let b = 0.7 * k as f32 + 0.3;
        *round = [(math::sin(a), math::cos(a)), (math::sin(b), math::cos(b))];
    }
    out
}

#[inline(always)]
fn cells(mut p: [f32; 3], turns: Option<&Turns>) -> f32 {
    let mut d = 100.0f32;
    let mut scale = 1.0f32;
    for k in 0..3 {
        let [(sa, ca), (sb, cb)] = match turns {
            Some(turns) => turns[k],
            None => {
                let a = TIME * SPIN + k as f32;
                let b = 0.7 * k as f32 + 0.3;
                [(math::sin(a), math::cos(a)), (math::sin(b), math::cos(b))]
            }
        };
        (p[0], p[1]) = (ca * p[0] - sa * p[1], sa * p[0] + ca * p[1]);
        (p[1], p[2]) = (cb * p[1] - sb * p[2], sb * p[1] + cb * p[2]);
        let q = p.map(|v| glsl_mod(v * scale, 2.0) - 1.0);
        let length = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2]).sqrt();
        d = glsl_min(d, (length - 0.45) / scale);
        scale *= 1.7;
    }
    d
}

#[inline(always)]
fn pixel(x: f32, y: f32, turns: Option<&Turns>) -> [f32; 4] {
    let uv = [(x / COLUMNS as f32) * 2.0 - 1.0, (y / ROWS as f32) * 2.0 - 1.0];
    let from = [0.0, 0.0, TIME * 0.2];
    let length = (uv[0] * uv[0] + uv[1] * uv[1] + 1.5 * 1.5).sqrt();
    let dir = [uv[0] / length, uv[1] / length, 1.5 / length];
    let (mut gone, mut steps) = (0.0f32, 0.0f32);
    for _ in 0..24 {
        let at = [
            from[0] + dir[0] * gone,
            from[1] + dir[1] * gone,
            from[2] + dir[2] * gone,
        ];
        let d = cells(at, turns);
        if d < 0.002 {
            break;
        }
        gone += d;
        steps += 1.0;
    }
    let glow = 1.0 - steps / 24.0;
    let tint = [0.0f32, 2.0, 4.0].map(|phase| 0.5 + 0.5 * math::cos(phase + gone * 0.6));
    [tint[0] * glow, tint[1] * glow, tint[2] * glow, 1.0]
}

/// The quickest of several timings of `work`, in nanoseconds a pixel.
fn quickest(pixels: usize, mut work: impl FnMut()) -> f64 {
    let mut best = f64::MAX;
    for _ in 0..7 {
        let started = Instant::now();
        work();
        best = best.min(started.elapsed().as_secs_f64());
    }
    best * 1e9 / pixels as f64
}

fn main() {
    let program = Program::compile(SHADER, &Limits::default()).expect("the shader compiles");
    let mut uniforms = program.uniforms();
    program.set_size(&mut uniforms, COLUMNS as f32, ROWS as f32);
    program.set_time(&mut uniforms, TIME, 0.025, 130);
    program.set_input(&mut uniforms, "spin", &[SPIN]);
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    for y in 0..ROWS {
        for x in 0..COLUMNS {
            xs.push(x as f32 + 0.5);
            ys.push(y as f32 + 0.5);
        }
    }
    let pixels = xs.len();
    println!(
        "{pixels} pixels; the program is {} instructions and {} values a pixel",
        program.instructions(),
        program.slots()
    );

    let mut compiled = vec![[0.0f32; 4]; pixels];
    let plain = quickest(pixels, || {
        for ((out, &x), &y) in compiled.iter_mut().zip(&xs).zip(&ys) {
            *out = pixel(x, y, None);
        }
    });
    let mut hoisted = vec![[0.0f32; 4]; pixels];
    let by_hand = quickest(pixels, || {
        let turns = turns();
        for ((out, &x), &y) in hoisted.iter_mut().zip(&xs).zip(&ys) {
            *out = pixel(x, y, Some(&turns));
        }
    });
    let bits = |all: &[[f32; 4]]| -> Vec<[u32; 4]> { all.iter().map(|p| p.map(f32::to_bits)).collect() };
    println!("compiled Rust, as written:                {plain:8.1} ns a pixel");
    println!(
        "compiled Rust, the turns worked out once: {by_hand:8.1} ns a pixel  (same bits: {})",
        bits(&compiled) == bits(&hoisted)
    );
    for width in [1usize, 4, 16, 64, 256, 1024, 4096] {
        let mut machine = Machine::new(&program, width);
        let mut out = vec![[0.0f32; 4]; pixels];
        let mut stats = Default::default();
        let ns = quickest(pixels, || {
            stats = machine.run(&program, &uniforms, &xs, &ys, &mut out)
        });
        println!(
            "the machine, {:4} pixels at a time:        {ns:8.1} ns a pixel  ({:5.1}x compiled as written, {:5.1}x by hand; {:.0} instructions a pixel; same bits: {})",
            machine.width(),
            ns / plain,
            ns / by_hand,
            stats.work as f64 / pixels as f64,
            bits(&out) == bits(&compiled),
        );
    }
}
