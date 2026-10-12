//! The shaders that come with PixelFlow, so a Shader effect has something to draw before anyone
//! has a shader file of their own.
//!
//! They are PixelFlow's own, written for it (see `shaders/`), each for props of few pixels:
//! large soft shapes, strong colors, and a handful of inputs. An effect names one as
//! `pixelflow:<name>`, which no path in a show's folder can be mistaken for.

/// One of PixelFlow's own shaders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltIn {
    /// What follows `pixelflow:` in an effect's `file` setting.
    pub name: &'static str,
    /// The name people see.
    pub title: &'static str,
    /// The shader file.
    pub source: &'static str,
}

/// What an effect's `file` setting starts with to name one of [`BUILT_IN`].
pub const BUILT_IN_PREFIX: &str = "pixelflow:";

macro_rules! built_in {
    ($( $name:literal, $title:literal; )*) => {
        /// PixelFlow's own shaders, in the order they're listed.
        pub const BUILT_IN: &[BuiltIn] = &[
            $( BuiltIn {
                name: $name,
                title: $title,
                source: include_str!(concat!("../shaders/", $name, ".fs")),
            }, )*
        ];
    };
}

built_in! {
    "aurora", "Aurora";
    "prism", "Prism";
    "lava-lamp", "Lava Lamp";
    "pond", "Pond";
    "nebula", "Nebula";
    "stained-glass", "Stained Glass";
    "kaleidoscope", "Kaleidoscope";
    "tunnel", "Tunnel";
}

/// The shader an effect's `file` setting names, when it names one of PixelFlow's own.
pub fn built_in(file: &str) -> Option<&'static BuiltIn> {
    let name = file.trim().strip_prefix(BUILT_IN_PREFIX)?;
    BUILT_IN.iter().find(|shader| shader.name == name)
}

/// Whether `file` is written as one of PixelFlow's own shaders is (there may be none by that
/// name: one from a newer PixelFlow, say).
pub fn is_built_in_name(file: &str) -> bool {
    file.trim().starts_with(BUILT_IN_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InputKind, Limits, Machine, Program};

    #[test]
    fn each_is_found_by_its_name_and_no_path_is_mistaken_for_one() {
        for shader in BUILT_IN {
            let named = format!("{BUILT_IN_PREFIX}{}", shader.name);
            assert_eq!(built_in(&named), Some(shader));
            assert!(is_built_in_name(&named));
        }
        let names: std::collections::HashSet<&str> = BUILT_IN.iter().map(|s| s.name).collect();
        assert_eq!(names.len(), BUILT_IN.len(), "no two share a name");
        for file in [
            "aurora",
            "shaders/aurora.fs",
            "pixelflow/aurora",
            "",
            "pixelflow:",
        ] {
            assert_eq!(built_in(file), None, "{file}");
        }
        assert!(is_built_in_name("pixelflow:from-a-newer-version"));
        assert!(!is_built_in_name("shaders/pixelflow:aurora.fs"));
    }

    #[test]
    fn each_compiles_says_what_it_is_and_costs_little() {
        let limits = Limits::default();
        for shader in BUILT_IN {
            let program =
                Program::compile(shader.source, &limits).unwrap_or_else(|e| panic!("{}: {e}", shader.name));
            let header = program.header();
            assert!(header.description.len() > 40, "{}: a description", shader.name);
            assert!(!header.credit.is_empty(), "{}: a credit", shader.name);
            assert!(
                (3..=8).contains(&header.inputs.len()),
                "{}: a few inputs, not {}",
                shader.name,
                header.inputs.len()
            );
            for input in &header.inputs {
                assert!(
                    input.label != input.name,
                    "{}: `{}` has a name people can read",
                    shader.name,
                    input.name
                );
                if let InputKind::Long { values, labels, .. } = &input.kind {
                    assert_eq!(values.len(), labels.len(), "{}: {}", shader.name, input.name);
                }
            }
            // A frame of a small matrix, at a moment when everything is moving: no pixel comes
            // near what a pixel may do, and every number that comes out is one.
            let mut uniforms = program.uniforms();
            program.set_size(&mut uniforms, 32.0, 16.0);
            program.set_time(&mut uniforms, 7.3, 0.025, 292);
            let xs: Vec<f32> = (0..512).map(|k| (k % 32) as f32 + 0.5).collect();
            let ys: Vec<f32> = (0..512).map(|k| (k / 32) as f32 + 0.5).collect();
            let mut out = vec![[0.0f32; 4]; 512];
            let mut machine = Machine::new(&program, 256);
            let stats = machine.run(&program, &uniforms, limits.work, &xs, &ys, &mut out);
            assert_eq!(stats.stopped, 0, "{}", shader.name);
            let each = stats.work / 512;
            assert!(
                each < u64::from(limits.work) / 40,
                "{}: {each} work a pixel",
                shader.name
            );
            assert!(
                out.iter().flatten().all(|v| v.is_finite()),
                "{}: numbers",
                shader.name
            );
            // Something is lit, and not everything is the same.
            let lit = out
                .iter()
                .filter(|px| px[3] > 0.05 && px[0] + px[1] + px[2] > 0.05)
                .count();
            assert!(lit > 40, "{}: {lit} of 512 pixels lit", shader.name);
            assert!(out.iter().any(|px| px != &out[0]), "{}: a pattern", shader.name);
        }
    }
}
