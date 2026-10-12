//! Spike: what shaders written to do harm cost before they are refused or stopped.
//!
//! ```sh
//! cargo run --release -p pf-shader --example hostile
//! ```
//!
//! Each is read and, if it can be, run for 4,959 pixels (a whole house's worth) on one thread.

use pf_shader::{Limits, Machine, Program};
use std::time::Instant;

const PIXELS: usize = 4959;

fn try_one(name: &str, source: &str) {
    let limits = Limits::default();
    let started = Instant::now();
    let read = Program::compile(source, &limits);
    let reading = started.elapsed();
    let program = match read {
        Ok(program) => program,
        Err(e) => {
            println!(
                "{name}\n    refused in {:.1} ms: {}",
                reading.as_secs_f64() * 1e3,
                e
            );
            return;
        }
    };
    let mut uniforms = program.uniforms();
    program.set_size(&mut uniforms, 87.0, 57.0);
    let xs: Vec<f32> = (0..PIXELS).map(|k| (k % 87) as f32 + 0.5).collect();
    let ys: Vec<f32> = (0..PIXELS).map(|k| (k / 87) as f32 + 0.5).collect();
    let mut out = vec![[0.0f32; 4]; PIXELS];
    let mut machine = Machine::new(&program, 256);
    let started = Instant::now();
    let stats = machine.run(&program, &uniforms, &xs, &ys, &mut out);
    let running = started.elapsed();
    let odd = out.iter().flatten().filter(|v| !v.is_finite()).count();
    println!(
        "{name}\n    read in {:.1} ms ({} instructions, {} values a pixel, {:.1} MB to run); a frame of {PIXELS} pixels in {:.1} ms; {} pixels stopped at the limit of {} instructions; {:.0} instructions a pixel; {odd} numbers out that aren't finite; first pixel {:?}",
        reading.as_secs_f64() * 1e3,
        program.instructions(),
        program.slots(),
        (program.slots() * machine.width() * 4) as f64 / 1e6,
        running.as_secs_f64() * 1e3,
        stats.stopped,
        limits.work,
        stats.work as f64 / PIXELS as f64,
        out[0],
    );
}

fn main() {
    try_one(
        "a loop with no end, the same for every pixel",
        "void main() { float x = 0.0; for (;;) { x += 1.0; } gl_FragColor = vec4(x); }",
    );
    try_one(
        "a loop with no end, different for every pixel",
        "void main() { float x = gl_FragCoord.x; while (true) { x = sin(x) + gl_FragCoord.y; } gl_FragColor = vec4(x); }",
    );
    try_one(
        "a loop of two billion turns",
        "void main() { float s = 0.0; for (int i = 0; i < 2000000000; i++) { s += gl_FragCoord.x; } gl_FragColor = vec4(s); }",
    );
    try_one(
        "three loops of a thousand turns, one inside another",
        "void main() { float s = 0.0; for (int i = 0; i < 1000; i++) for (int j = 0; j < 1000; j++) for (int k = 0; k < 1000; k++) s += gl_FragCoord.x; gl_FragColor = vec4(s); }",
    );
    try_one(
        "a function that calls itself",
        "float f(float x) { return x < 1.0 ? x : f(x * 0.5); } void main() { gl_FragColor = vec4(f(8.0)); }",
    );
    try_one(
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
    try_one(
        "thirty functions, each calling the one before it twice (a billion calls written out)",
        &swell,
    );
    let mut macros = String::from("#define M0 gl_FragCoord.x +\n");
    for i in 1..40 {
        macros.push_str(&format!("#define M{i} M{} M{}\n", i - 1, i - 1));
    }
    macros.push_str("void main() { gl_FragColor = vec4(M39 1.0); }");
    try_one("forty macros, each the one before it twice", &macros);
    try_one(
        "an array of two billion numbers",
        "void main() { float a[2000000000]; a[0] = 1.0; gl_FragColor = vec4(a[0]); }",
    );
    let mut arrays = String::from("void main() {\n");
    for i in 0..64 {
        arrays.push_str(&format!("vec4 a{i}[1024];\n"));
    }
    arrays.push_str("gl_FragColor = a0[0]; }");
    try_one("sixty-four arrays of the largest size", &arrays);
    try_one(
        "a hundred thousand parentheses",
        &format!(
            "void main() {{ gl_FragColor = vec4({}1.0{}); }}",
            "(".repeat(100_000),
            ")".repeat(100_000)
        ),
    );
    try_one(
        "a sum of sixty thousand terms",
        &format!(
            "void main() {{ gl_FragColor = vec4(1.0{}); }}",
            " + 1.0".repeat(60_000)
        ),
    );
    try_one(
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
    try_one(
        "the longest shader that fits: nine thousand lines of sines",
        &long,
    );
    try_one(
        "dividing by zero, and what isn't a number",
        "void main() { float z = gl_FragCoord.x - gl_FragCoord.x; int i = int(z); gl_FragColor = vec4(1.0 / z, z / z, float(7 / i), sqrt(-1.0)); }",
    );
    try_one(
        "an index far outside its array",
        "void main() { float a[4]; a[0] = 0.25; a[3] = 0.75; int k = int(gl_FragCoord.x * 1e9); a[k * 3] += 0.125; gl_FragColor = vec4(a[k], a[-k], a[k + k], 1.0); }",
    );
}
