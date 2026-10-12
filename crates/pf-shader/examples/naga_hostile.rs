//! Spike: what naga's GLSL front end does with a shader written to do harm, next to this
//! crate's. Each case is its own run of the program, because the point is whether the program
//! survives:
//!
//! ```sh
//! cargo run --release -p pf-shader --example naga_hostile -- naga parens 100000
//! cargo run --release -p pf-shader --example naga_hostile -- ours macros 40
//! ```
//!
//! The cases: `parens N` (N nested parentheses), `sum N` (N terms added), `blocks N` (N nested
//! blocks), `macros N` (N macros, each the one before it twice).

use naga::front::glsl::{Frontend, Options};
use pf_shader::{Limits, Program};
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, who, case, n] = args.as_slice() else {
        eprintln!("usage: naga_hostile <naga|ours> <parens|sum|blocks|macros> <N>");
        std::process::exit(2);
    };
    let n: usize = n.parse().unwrap_or(1);
    let body = match case.as_str() {
        "parens" => format!(
            "void main() {{ float x = {}1.0{}; gl_FragColor = vec4(x); }}",
            "(".repeat(n),
            ")".repeat(n)
        ),
        "sum" => format!(
            "void main() {{ float x = 1.0{}; gl_FragColor = vec4(x); }}",
            " + 1.0".repeat(n)
        ),
        "blocks" => format!(
            "void main() {{ {} gl_FragColor = vec4(1.0); {} }}",
            "{".repeat(n),
            "}".repeat(n)
        ),
        _ => {
            let mut source = String::from("#define M0 1.0 +\n");
            for i in 1..n {
                source.push_str(&format!("#define M{i} M{} M{}\n", i - 1, i - 1));
            }
            source.push_str(&format!("void main() {{ gl_FragColor = vec4(M{} 1.0); }}", n - 1));
            source
        }
    };
    let started = Instant::now();
    let outcome = if who == "naga" {
        let source = format!(
            "#version 450\nlayout(location = 0) out vec4 pf_color;\n#define gl_FragColor pf_color\n{body}"
        );
        match Frontend::default().parse(&Options::from(naga::ShaderStage::Fragment), &source) {
            // Checked as a host would check it (which is also what a program using naga links).
            Ok(module) => match naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            {
                Ok(_) => "read".to_string(),
                Err(e) => format!("read, then rejected: {}", e.into_inner()),
            },
            Err(e) => format!(
                "refused: {}",
                e.errors.first().map(|e| e.to_string()).unwrap_or_default()
            ),
        }
    } else {
        // Under the limits a shader file is read with, but for the size of the file, so that
        // what stops it is what the text does and not how long it is.
        let limits = Limits {
            source_bytes: usize::MAX,
            ..Limits::default()
        };
        match Program::compile(&body, &limits) {
            Ok(_) => "read".to_string(),
            Err(e) => format!("refused: {e}"),
        }
    };
    println!(
        "{who} {case} {n}: {outcome} after {:.1} ms",
        started.elapsed().as_secs_f64() * 1e3
    );
}
