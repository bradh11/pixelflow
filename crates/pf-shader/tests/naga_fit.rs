//! Spike: would an existing GLSL front end (naga's, from the wgpu project) read the shader files
//! people have? It reads Vulkan-style GLSL 4.5; shader files are written in the older GLSL the
//! Interactive Shader Format grew up with. Each shader is tried as it is, and again behind the
//! most helpful preamble a host could write for it.
//!
//! ```sh
//! PF_SHADER_FILES="one.fs;two.fs" cargo test -p pf-shader --test naga_fit -- --nocapture
//! ```
//!
//! With no files named it tries only the shaders written here.

use naga::front::glsl::{Frontend, Options};
use naga::valid::{Capabilities, ValidationFlags, Validator};
use pf_shader::{InputKind, Limits, Program};

/// What naga makes of `source` as a fragment shader: `Ok`, or its first complaint.
fn naga_reads(source: &str) -> Result<(), String> {
    let mut frontend = Frontend::default();
    let module = frontend
        .parse(&Options::from(naga::ShaderStage::Fragment), source)
        .map_err(|errors| {
            let first = errors.errors.first().map(|e| e.to_string()).unwrap_or_default();
            format!("can't read it: {first} ({} complaints)", errors.errors.len())
        })?;
    Validator::new(ValidationFlags::all(), Capabilities::empty())
        .validate(&module)
        .map_err(|e| format!("reads it, then rejects it: {}", e.into_inner()))?;
    Ok(())
}

/// The shader behind what a host would have to put before it for naga: a version line, the
/// inputs and built-in values in a block, and somewhere for `gl_FragColor` to go.
fn with_preamble(source: &str) -> String {
    let mut block = String::from(
        "float TIME; vec2 RENDERSIZE; float TIMEDELTA; int FRAMEINDEX; int PASSINDEX; vec4 DATE;",
    );
    let mut defines = String::new();
    for input in pf_shader::read_header(source)
        .map(|h| h.inputs)
        .unwrap_or_default()
    {
        let name = &input.name;
        match input.kind {
            InputKind::Float { .. } => block.push_str(&format!(" float {name};")),
            // A truth can't sit in a block of inputs: it goes in as a number.
            InputKind::Bool { .. } | InputKind::Event => {
                block.push_str(&format!(" int {name}_as_int;"));
                defines.push_str(&format!("#define {name} ({name}_as_int != 0)\n"));
            }
            InputKind::Long { .. } => block.push_str(&format!(" int {name};")),
            InputKind::Point { .. } => block.push_str(&format!(" vec2 {name};")),
            InputKind::Color { .. } => block.push_str(&format!(" vec4 {name};")),
            InputKind::Image | InputKind::Audio | InputKind::AudioFft => {}
        }
    }
    format!(
        "#version 450\nlayout(set = 0, binding = 0) uniform Inputs {{ {block} }};\n\
         layout(location = 0) out vec4 pf_color;\n#define gl_FragColor pf_color\n{defines}#line 1\n{source}"
    )
}

fn try_shader(name: &str, source: &str) -> (bool, bool, bool) {
    let ours = Program::compile(source, &Limits::default());
    let plain = naga_reads(source);
    let helped = naga_reads(&with_preamble(source));
    let say = |r: &Result<(), String>| match r {
        Ok(()) => "yes".to_string(),
        Err(why) => format!("no ({why})"),
    };
    println!(
        "{name}\n    this crate: {}\n    naga, as written: {}\n    naga, behind a preamble: {}",
        match &ours {
            Ok(_) => "yes".to_string(),
            Err(e) => format!("no ({e})"),
        },
        say(&plain),
        say(&helped)
    );
    (ours.is_ok(), plain.is_ok(), helped.is_ok())
}

/// Shaders written for this test in the ways shader files are commonly written.
const OWN: &[(&str, &str)] = &[
    (
        "plain: a gradient, written as GLSL ES 1.0",
        "void main() { vec2 uv = gl_FragCoord.xy / RENDERSIZE; gl_FragColor = vec4(uv, 0.5, 1.0); }",
    ),
    (
        "a global worked out from an input before main",
        "float pace = TIME * 2.0;\nvoid main() { gl_FragColor = vec4(fract(pace)); }",
    ),
    (
        "whole numbers where floats are wanted",
        "void main() { vec3 c = vec3(1, 0, 0) / 2; float x = 1; gl_FragColor = vec4(c * x, 1); }",
    ),
    (
        "a loop counted in floats, and a macro with arguments",
        "#define sq(x) ((x) * (x))\nvoid main() { float s = 0.0; for (float i = 0.0; i < 4.0; i++) { s += sq(i); } gl_FragColor = vec4(s / 14.0); }",
    ),
    (
        "a function that doesn't return on every path",
        "float pick(float x) { if (x < 0.5) return 0.25; }\nvoid main() { gl_FragColor = vec4(pick(gl_FragCoord.x / RENDERSIZE.x)); }",
    ),
    (
        "an input of each kind",
        "/*{ \"INPUTS\": [ {\"NAME\": \"level\", \"TYPE\": \"float\"}, {\"NAME\": \"on\", \"TYPE\": \"bool\"}, {\"NAME\": \"mode\", \"TYPE\": \"long\", \"VALUES\": [0, 1]}, {\"NAME\": \"at\", \"TYPE\": \"point2D\"}, {\"NAME\": \"tint\", \"TYPE\": \"color\"} ] }*/\nvoid main() { vec3 c = on ? tint.rgb : vec3(level); if (mode == 1) c += at.x; gl_FragColor = vec4(c, 1.0); }",
    ),
    (
        "precision lines, as shaders for phones and the web carry",
        "#ifdef GL_ES\nprecision mediump float;\n#endif\nvoid main() { gl_FragColor = vec4(mod(TIME, 1.0)); }",
    ),
    (
        "a matrix times a vector, and components stored out of order",
        "void main() { mat2 m = mat2(0.0, 1.0, -1.0, 0.0); vec3 v = vec3(m * gl_FragCoord.xy, 0.0); v.zx = v.xy; gl_FragColor = vec4(v, 1.0); }",
    ),
];

#[test]
fn what_each_front_end_reads() {
    let mut tally = [0usize; 3];
    let mut count = 0;
    for (name, source) in OWN {
        let (ours, plain, helped) = try_shader(name, source);
        tally[0] += usize::from(ours);
        tally[1] += usize::from(plain);
        tally[2] += usize::from(helped);
        count += 1;
        // Every one of these is a shader this crate must read.
        assert!(ours, "{name}");
    }
    println!(
        "\nof {count} written here: this crate reads {}, naga {} as written and {} behind a preamble",
        tally[0], tally[1], tally[2]
    );
    // Someone's own shader files, named one by one, by letter in the order given.
    let Ok(files) = std::env::var("PF_SHADER_FILES") else {
        return;
    };
    let mut tally = [0usize; 3];
    let mut count = 0;
    for path in files.split(';').filter(|p| !p.is_empty()) {
        let Ok(source) = std::fs::read_to_string(path) else {
            println!("a file that can't be read");
            continue;
        };
        let letter = char::from(b'A' + (count as u8).min(25));
        let (ours, plain, helped) = try_shader(&format!("shader {letter}"), &source);
        tally[0] += usize::from(ours);
        tally[1] += usize::from(plain);
        tally[2] += usize::from(helped);
        count += 1;
    }
    println!(
        "\nof {count} named: this crate reads {}, naga {} as written and {} behind a preamble",
        tally[0], tally[1], tally[2]
    );
}
