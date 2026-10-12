//! The shader files an xLights sequence's Shader effects name, found on this computer.
//!
//! xLights stores a shader's full path as it was on the computer the sequence was made on, so
//! the file is looked for by name: where the sequence says (when that is this computer), in
//! the sequence's own folder and the ones above it up to the xLights show folder, and in a
//! `Shaders` folder of each, which is where xLights itself keeps them. A path written as a
//! network share is never looked at as written (see [`pf_model::network_path_refusal`]).
//!
//! What is found is read as any shader file is (`pf_shader::read_shader_file`: a text file of a
//! shader's size) and compiled under the limits PixelFlow runs shaders with. The importer needs
//! the file itself, not only its name: which settings of the effect are the shader's inputs,
//! and what each means, is in the file's header. So a Shader effect whose file isn't found, or
//! can't be run, can't be translated, and comes across as a dim stand-in with a note naming
//! the file.

use pf_shader::{Header, Limits, Program};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The folder xLights keeps a show's shader files in, as it's usually written.
const SHADER_FOLDERS: [&str; 2] = ["Shaders", "shaders"];

/// A shader file a sequence names, found and compiled.
#[derive(Debug, Clone, PartialEq)]
pub struct FoundShader {
    /// Where it is on this computer, in full.
    pub path: String,
    /// What the file says about itself: its inputs most of all.
    pub header: Header,
}

/// What came of looking for a shader file.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Lookup {
    Found(Arc<FoundShader>),
    /// It was found and can't be run: why, in plain words.
    Refused(String),
    Missing,
}

/// The shader files near a sequence, each looked for once.
#[derive(Debug, Default)]
pub struct NearbyShaders {
    folders: Vec<PathBuf>,
    looked: RefCell<HashMap<String, Lookup>>,
}

impl NearbyShaders {
    /// The shader files to be found near the sequence at `sequence`.
    pub fn near(sequence: &Path) -> Self {
        let mut folders = Vec::new();
        for folder in super::picture_folders(sequence) {
            for inside in SHADER_FOLDERS {
                let inside = folder.join(inside);
                if !folders.contains(&inside) {
                    folders.push(inside);
                }
            }
            folders.push(folder);
        }
        Self {
            folders,
            looked: RefCell::default(),
        }
    }

    /// Looks for the shader file a sequence writes as `written`, reads it, and compiles it.
    pub(crate) fn find(&self, written: &str) -> Lookup {
        if let Some(known) = self.looked.borrow().get(written) {
            return known.clone();
        }
        let found = self
            .folders
            .iter()
            .find_map(|folder| crate::background::find(written, folder));
        let lookup = match found {
            None => Lookup::Missing,
            Some(at) => {
                let at = std::path::absolute(&at).unwrap_or(at);
                let compiled = pf_shader::read_shader_file(&at).and_then(|source| {
                    Program::compile(&source, &Limits::default()).map_err(|e| e.to_string())
                });
                let path = pf_model::path_to_text(&at);
                match compiled {
                    _ if path.chars().count() > pf_sequence::MAX_TEXT_LEN => {
                        Lookup::Refused("its path is too long to keep".into())
                    }
                    Ok(program) => Lookup::Found(Arc::new(FoundShader {
                        path,
                        header: program.header().clone(),
                    })),
                    Err(why) => Lookup::Refused(why),
                }
            }
        };
        self.looked
            .borrow_mut()
            .insert(written.to_string(), lookup.clone());
        lookup
    }
}
