//! What shaders written to do harm cost before they are refused or stopped, under the limits
//! PixelFlow runs shader files with ([`Limits::default`]).
//!
//! ```sh
//! cargo run --release -p pf-shader --example hostile
//! ```
//!
//! Each is read and, if it can be, run for a frame on one thread: a whole house's worth of
//! pixels (4,959), then a quarter of a million, where the limit on a frame's work is what holds.
//! The first table is shaders that try to take memory, time to read, or time to run; the second
//! is loops that never end, each doing nothing but the dearest thing of one kind, to find what a
//! unit of work can cost at most. `docs/security/threat-model.md` quotes the results.

use pf_shader::{Limits, Machine, Program};
use std::time::Instant;

/// The pixels of a whole-house group, and of a prop far larger than any.
const HOUSE: usize = 4959;
const HUGE: usize = 250_000;

/// A frame of `pixels` pixels on one thread: the milliseconds it took, the work done for each
/// pixel, and how many were stopped.
fn frame(program: &Program, pixels: usize) -> (f64, f64, u64) {
    let columns = (pixels as f64).sqrt().ceil() as usize;
    let mut uniforms = program.uniforms();
    program.set_size(&mut uniforms, columns as f32, columns as f32);
    program.set_time(&mut uniforms, 1.0, 0.025, 40);
    let xs: Vec<f32> = (0..pixels).map(|k| (k % columns) as f32 + 0.5).collect();
    let ys: Vec<f32> = (0..pixels).map(|k| (k / columns) as f32 + 0.5).collect();
    let mut out = vec![[0.0f32; 4]; pixels];
    let mut machine = Machine::new(program, 256);
    let work = program.work_for(pixels);
    // Once to warm the machine's memory, then the best of three.
    machine.run(program, &uniforms, work, &xs, &ys, &mut out);
    let mut best = f64::INFINITY;
    let mut stats = Default::default();
    for _ in 0..3 {
        let started = Instant::now();
        stats = machine.run(program, &uniforms, work, &xs, &ys, &mut out);
        best = best.min(started.elapsed().as_secs_f64() * 1e3);
    }
    (best, stats.work as f64 / pixels as f64, stats.stopped)
}

/// Reads and runs one shader, and says what came of it. Returns the slowest frame, in
/// milliseconds, and what a unit of work cost there, in nanoseconds.
fn try_one(name: &str, source: &str) -> (f64, f64) {
    let limits = Limits::default();
    let started = Instant::now();
    let read = Program::compile(source, &limits);
    let reading = started.elapsed().as_secs_f64() * 1e3;
    let program = match read {
        Ok(program) => program,
        Err(e) => {
            println!("{name}\n    refused in {reading:.1} ms: {e}");
            return (0.0, 0.0);
        }
    };
    println!(
        "{name}\n    read in {reading:.1} ms ({} instructions, {} values a pixel, at least {} work a pixel)",
        program.instructions(),
        program.slots(),
        program.least_work(),
    );
    let mut worst = (0.0f64, 0.0f64);
    for pixels in [HOUSE, HUGE] {
        let (ms, work, stopped) = frame(&program, pixels);
        let unit = ms * 1e6 / (work * pixels as f64).max(1.0);
        println!(
            "    {pixels:>7} pixels: {ms:8.1} ms a frame; {work:9.0} work a pixel of {:>6} allowed; {stopped} stopped; {unit:.3} ns a unit",
            program.work_for(pixels),
        );
        if ms > worst.0 {
            worst = (ms, unit);
        }
    }
    worst
}

/// A loop that can't be counted when it's read and never ends when it's run, doing `body` each
/// turn on numbers that differ from pixel to pixel.
fn forever(body: &str) -> String {
    format!(
        "void main() {{ vec2 p = gl_FragCoord.xy / RENDERSIZE + 0.25; float x = p.x; float y = p.y; vec3 v = vec3(p, 0.5); float a[64]; int k = 0;
         for (float i = 0.0; i < 2.0e9; i += 1.0) {{ {body} }}
         gl_FragColor = vec4(x, y, v.x, 1.0); }}"
    )
}

fn main() {
    println!("== shaders written to do harm ==\n");
    let mut worst = (0.0f64, String::new());
    let mut note = |name: &str, source: &str| {
        let (ms, _) = try_one(name, source);
        if ms > worst.0 {
            worst = (ms, name.to_string());
        }
    };
    note(
        "a loop with no end, the same for every pixel",
        "void main() { float x = 0.0; for (;;) { x += 1.0; } gl_FragColor = vec4(x); }",
    );
    note(
        "a loop with no end, different for every pixel",
        "void main() { float x = gl_FragCoord.x; while (true) { x = sin(x) + gl_FragCoord.y; } gl_FragColor = vec4(x); }",
    );
    note(
        "a loop that ends for no pixel, though it asks each turn",
        "void main() { float x = gl_FragCoord.x; while (x > -1.0) { x = sin(x) + gl_FragCoord.y; } gl_FragColor = vec4(x); }",
    );
    note(
        "a loop of two billion turns",
        "void main() { float s = 0.0; for (int i = 0; i < 2000000000; i++) { s += gl_FragCoord.x; } gl_FragColor = vec4(s); }",
    );
    note(
        "a loop of two billion turns that can't be counted",
        "void main() { float s = 0.0; for (float i = 0.0; i < 2.0e9; i += 1.0) { s += gl_FragCoord.x; } gl_FragColor = vec4(s); }",
    );
    note(
        "three loops of a thousand turns, one inside another",
        "void main() { float s = 0.0; for (int i = 0; i < 1000; i++) for (int j = 0; j < 1000; j++) for (int k = 0; k < 1000; k++) s += gl_FragCoord.x; gl_FragColor = vec4(s); }",
    );
    note(
        "a function that calls itself",
        "float f(float x) { return x < 1.0 ? x : f(x * 0.5); } void main() { gl_FragColor = vec4(f(8.0)); }",
    );
    note(
        "two functions that call each other",
        "float g(float x); float f(float x) { return g(x) + 1.0; } float g(float x) { return f(x) + 1.0; } void main() { gl_FragColor = vec4(f(1.0)); }",
    );
    let mut swell = String::from("float f0(float x) { return x * 0.5; }\n");
    for i in 1..31 {
        swell.push_str(&format!(
            "float f{i}(float x) {{ return f{}(x) + f{}(x + 1.0); }}\n",
            i - 1,
            i - 1
        ));
    }
    swell.push_str("void main() { gl_FragColor = vec4(f30(0.0)); }");
    note(
        "thirty functions, each calling the one before it twice (a billion calls written out)",
        &swell,
    );
    let mut macros = String::from("#define M0 gl_FragCoord.x +\n");
    for i in 1..40 {
        macros.push_str(&format!("#define M{i} M{} M{}\n", i - 1, i - 1));
    }
    macros.push_str("void main() { gl_FragColor = vec4(M39 1.0); }");
    note("forty macros, each the one before it twice", &macros);
    note(
        "an array of two billion numbers",
        "void main() { float a[2000000000]; a[0] = 1.0; gl_FragColor = vec4(a[0]); }",
    );
    let mut arrays = String::from("void main() {\n");
    for i in 0..64 {
        arrays.push_str(&format!("vec4 a{i}[1024];\n"));
    }
    arrays.push_str("gl_FragColor = a0[0]; }");
    note("sixty-four arrays of the largest size", &arrays);
    let mut fits = String::from("void main() {\n");
    for i in 0..3 {
        fits.push_str(&format!("vec4 a{i}[1024];\n"));
    }
    fits.push_str(
        "for (float i = 0.0; i < 2.0e9; i += 1.0) { int k = int(gl_FragCoord.x * i); a0[k] = a1[k + 1] + a2[k * 3]; a1[k] += a0[k].yzwx; }\ngl_FragColor = a0[0]; }",
    );
    note(
        "the largest arrays that fit, stored to and read by an index that differs for every pixel",
        &fits,
    );
    note(
        "a hundred thousand parentheses",
        &format!(
            "void main() {{ gl_FragColor = vec4({}1.0{}); }}",
            "(".repeat(100_000),
            ")".repeat(100_000)
        ),
    );
    note(
        "a sum of sixty thousand terms",
        &format!(
            "void main() {{ gl_FragColor = vec4(1.0{}); }}",
            " + 1.0".repeat(60_000)
        ),
    );
    note(
        "a file of a megabyte",
        &format!(
            "void main() {{ gl_FragColor = vec4(1.0); }}\n{}",
            "// padding\n".repeat(100_000)
        ),
    );
    let mut long = String::from("void main() { float x = gl_FragCoord.x;\n");
    for _ in 0..9_000 {
        long.push_str("x = sin(x) * 1.0001 + 0.5;\n");
    }
    long.push_str("gl_FragColor = vec4(x); }");
    note("nine thousand lines of sines", &long);
    let mut longest = String::from("void main() { float x = gl_FragCoord.x; float y = gl_FragCoord.y;\n");
    for _ in 0..16_000 {
        longest.push_str("x = y - x * y;\n");
    }
    longest.push_str("gl_FragColor = vec4(x); }");
    note(
        "the longest shader that fits: sixteen thousand lines of arithmetic",
        &longest,
    );
    note(
        "dividing by zero, and what isn't a number",
        "void main() { float z = gl_FragCoord.x - gl_FragCoord.x; int i = int(z); gl_FragColor = vec4(1.0 / z, z / z, float(7 / i), sqrt(-1.0)); }",
    );
    note(
        "an index far outside its array",
        "void main() { float a[4]; a[0] = 0.25; a[3] = 0.75; int k = int(gl_FragCoord.x * 1e9); a[k * 3] += 0.125; gl_FragColor = vec4(a[k], a[-k], a[k + k], 1.0); }",
    );

    println!("\n== the dearest a unit of work can be: loops without end, each of one kind of work ==\n");
    let mut dearest = (0.0f64, 0.0f64, String::new());
    for (name, body) in [
        ("additions", "x = x + y; y = y - x * 0.5;"),
        ("multiplications of vectors", "v = v * v.yzx + v.zxy;"),
        ("divisions", "x = y / (x + 1.5); y = x / (y + 1.5);"),
        (
            "square roots",
            "x = sqrt(abs(y) + x); y = inversesqrt(abs(x) + 1.0);",
        ),
        (
            "floor, fract, and mod",
            "x = fract(x * 3.7) + floor(y); y = mod(x, 0.37) + ceil(y * 0.1);",
        ),
        ("sines", "x = sin(x) + y;"),
        ("sines and cosines of vectors", "v = sin(v) + cos(v.yzx);"),
        ("tangents", "x = tan(x) * 0.5 + y;"),
        ("arcsines and arccosines", "x = asin(fract(x)) + acos(fract(y));"),
        ("arctangents of two", "x = atan(x, y) + 1.5;"),
        (
            "exponentials and logarithms",
            "x = exp(fract(x)) + log(abs(y) + 1.0);",
        ),
        ("powers", "x = pow(abs(x) + 0.5, fract(y) + 0.5);"),
        (
            "hyperbolic functions",
            "x = tanh(x) + sinh(fract(y)) + cosh(fract(x));",
        ),
        (
            "smoothstep, mix, and clamp",
            "x = smoothstep(0.0, 1.0, x) + mix(x, y, 0.3); y = clamp(x * y, 0.1, 0.9);",
        ),
        (
            "comparisons and choices",
            "x = x > y ? x - y : y + x; y = (x < 0.5 || y > 0.5) ? 0.3 : x;",
        ),
        (
            "lengths and dot products",
            "x = length(v) + dot(v, v.yzx); v = normalize(v + x);",
        ),
        (
            "matrices",
            "mat3 m = mat3(v, v.yzx, v.zxy); v = m * v + 0.1; v = fract(v);",
        ),
        (
            "stores by an index that differs",
            "k = int(x * 64.0 + i); a[k] = x; x = fract(x + 0.37);",
        ),
        (
            "reads by an index that differs",
            "k = int(x * 64.0 + i); x = fract(a[k] + x + 0.37);",
        ),
        (
            "ifs that split the pixels",
            "if (fract(x * 7.0) > 0.5) { x += 0.1; } else { y += 0.1; } if (fract(y * 3.0) > 0.5) { x = fract(x); }",
        ),
        ("calls", "x = turn(x, y); y = turn(y, x);"),
        (
            "whole numbers",
            "k = (k * 31 + int(x * 9.0)) % 977; k ^= k << 3; x = fract(x + float(k) * 0.001);",
        ),
    ] {
        let source = format!(
            "float turn(float a, float b) {{ if (a > b) return a - b; return b + a * 0.5; }}\n{}",
            forever(body)
        );
        let (ms, unit) = try_one(name, &source);
        if unit > dearest.1 {
            dearest = (ms, unit, name.to_string());
        }
        if ms > worst.0 {
            worst = (ms, name.to_string());
        }
    }
    println!(
        "\nthe dearest unit of work: {:.3} ns ({}); the slowest frame of all: {:.1} ms ({})",
        dearest.1, dearest.2, worst.0, worst.1
    );
}
