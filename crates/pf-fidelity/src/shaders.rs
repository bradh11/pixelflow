//! Spike: draws a sequence's xLights Shader effects with their shader files, so their look can
//! be held against xLights' frames.
//!
//! Off unless `PF_SHADER_DIR` names a folder holding the shader files the sequence uses (looked
//! up there by file name, since a sequence names them by a path on the computer it was made on).
//! Each Shader effect's settings are read as the sequence has them and handed to the renderer
//! beside the import (see `pf_render::glsl`); the import itself still shows a dim stand-in.
//!
//! What a sequence keeps for a shader's inputs, as found in sequences:
//!
//! - a number: `E_SLIDER_SHADERXYZZY_<name>`, a hundred times the value;
//! - a switch: `E_CHECKBOX_SHADERXYZZY_<name>`, 0 or 1;
//! - a choice: `E_CHOICE_SHADERXYZZY_<name>`, the label of the one chosen;
//! - a point: `E_SLIDER_SHADERXYZZY_<name>X` and `…Y`, a hundred times each;
//! - the effect's own: `E_SLIDER_Shader_Speed` (100 is the shader's own pace) and
//!   `E_TEXTCTRL_Shader_LeadIn` (frames).

use pf_render::glsl::{ShaderSetup, set_shader_batch, set_shader_effects, set_shader_platform_math};
use pf_shader::{InputKind, Limits, Program};
use pf_xlights::SequenceImport;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// The largest file read as a shader.
const MAX_FILE_BYTES: u64 = 1024 * 1024;

fn setting<'a>(settings: &'a [(String, String)], key: &str) -> Option<&'a str> {
    settings.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

fn number(settings: &[(String, String)], key: &str) -> Option<f32> {
    setting(settings, key)?
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|v| v.is_finite())
}

/// The last part of a path written on any system.
fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn read(dir: &Path, name: &str) -> Result<String, String> {
    let path = dir.join(name);
    let size = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
    if size > MAX_FILE_BYTES {
        return Err("the file is too large to be a shader".into());
    }
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}

/// The inputs a Shader effect sets, from its settings.
fn inputs(program: &Program, settings: &[(String, String)], curves: &mut usize) -> Vec<(String, Vec<f32>)> {
    let mut set = Vec::new();
    for input in &program.header().inputs {
        let name = &input.name;
        if setting(settings, &format!("E_VALUECURVE_SHADERXYZZY_{name}"))
            .is_some_and(|v| v.contains("Active=TRUE"))
        {
            *curves += 1;
        }
        let value = match &input.kind {
            InputKind::Float { .. } => {
                number(settings, &format!("E_SLIDER_SHADERXYZZY_{name}")).map(|v| vec![v / 100.0])
            }
            InputKind::Bool { .. } | InputKind::Event => {
                number(settings, &format!("E_CHECKBOX_SHADERXYZZY_{name}")).map(|v| vec![v])
            }
            InputKind::Long { values, labels, .. } => {
                setting(settings, &format!("E_CHOICE_SHADERXYZZY_{name}")).and_then(|chosen| {
                    let at = labels.iter().position(|label| label.trim() == chosen.trim())?;
                    values.get(at).map(|&v| vec![v as f32])
                })
            }
            InputKind::Point { .. } => {
                let x = number(settings, &format!("E_SLIDER_SHADERXYZZY_{name}X"));
                let y = number(settings, &format!("E_SLIDER_SHADERXYZZY_{name}Y"));
                x.zip(y).map(|(x, y)| vec![x / 100.0, y / 100.0])
            }
            // Colors aren't among an effect's settings; where xLights takes them from isn't
            // known from the files, so they stay as the shader has them.
            InputKind::Color { .. } | InputKind::Image | InputKind::Audio | InputKind::AudioFft => None,
        };
        if let Some(value) = value {
            set.push((name.clone(), value));
        }
    }
    set
}

/// Hands the renderer the shaders of `import`'s Shader effects (none, when `PF_SHADER_DIR` isn't
/// set). Says what it did in `notes`, the shaders by letter in the order they first appear.
pub fn install(import: &SequenceImport, notes: &mut Vec<String>) {
    let mut table = HashMap::new();
    let Some(dir) = std::env::var_os("PF_SHADER_DIR") else {
        set_shader_effects(table);
        return;
    };
    if let Some(width) = std::env::var("PF_SHADER_BATCH").ok().and_then(|v| v.parse().ok()) {
        set_shader_batch(width);
    }
    set_shader_platform_math(std::env::var_os("PF_SHADER_PLATFORM_MATH").is_some());
    let dir = Path::new(&dir);
    let mut programs: HashMap<String, Result<Arc<Program>, String>> = HashMap::new();
    let mut letters: Vec<String> = Vec::new();
    let mut uses: Vec<usize> = Vec::new();
    let mut curves = 0;
    for effect in import.effects.iter().filter(|e| !e.shader_settings.is_empty()) {
        let settings = &effect.shader_settings;
        let name = file_name(setting(settings, "E_0FILEPICKERCTRL_IFS").unwrap_or_default()).to_string();
        let letter = match letters.iter().position(|n| *n == name) {
            Some(at) => at,
            None => {
                letters.push(name.clone());
                uses.push(0);
                letters.len() - 1
            }
        };
        uses[letter] += 1;
        let program = programs.entry(name.clone()).or_insert_with(|| {
            let source = read(dir, &name)?;
            Program::compile(&source, &Limits::default())
                .map(Arc::new)
                .map_err(|e| e.to_string())
        });
        let Ok(program) = program else {
            continue;
        };
        let mut inputs = inputs(program, settings, &mut curves);
        // For trying things: `PF_SHADER_SET=name=value;name=value,value` sets inputs of that name
        // over what the sequence says.
        if let Ok(forced) = std::env::var("PF_SHADER_SET") {
            for (name, value) in forced.split(';').filter_map(|pair| pair.split_once('=')) {
                let numbers: Vec<f32> = value.split(',').filter_map(|v| v.trim().parse().ok()).collect();
                if !numbers.is_empty() {
                    inputs.retain(|(n, _)| n != name.trim());
                    inputs.push((name.trim().to_string(), numbers));
                }
            }
        }
        let setup = ShaderSetup {
            program: Arc::clone(program),
            inputs,
            speed: number(settings, "E_SLIDER_Shader_Speed").map_or(1.0, |v| v / 100.0),
            lead_in_frames: number(settings, "E_TEXTCTRL_Shader_LeadIn").map_or(0, |v| v.max(0.0) as u32),
        };
        table.insert(effect.id.seed(), Arc::new(setup));
    }
    for (at, name) in letters.iter().enumerate() {
        let letter = char::from(b'A' + (at as u8).min(25));
        let what = match &programs[name] {
            Ok(program) => format!(
                "runs ({} instructions, {} values a pixel, {} inputs)",
                program.instructions(),
                program.slots(),
                program.header().inputs.len()
            ),
            Err(why) => format!("can't be run: {why}"),
        };
        notes.push(format!("Shader {letter}, used {} times, {what}.", uses[at]));
    }
    if curves > 0 {
        notes.push(format!(
            "{curves} shader inputs follow a value curve, which isn't carried over: they sit at their slider's value."
        ));
    }
    set_shader_effects(table);
}
