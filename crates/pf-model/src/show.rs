//! The top-level show document.

use crate::{Controller, ControllerId, Group, Prop, PropId, Region, RegionId, SequenceId, Vec3};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Schema version written by this build.
///
/// Policy: bump this for **every** change to the show file format, even an additive one with
/// a no-op migration, so an older PixelFlow refuses a newer file instead of silently dropping
/// fields it doesn't know on save. Add the migration in `io.rs` in the same change.
///
/// History: 1 = initial format; 2 = adds the `falcon` controller adapter; 3 = adds a
/// controller's `sequenceChannels`; 4 = adds the show's `sequences`; 5 = adds the show's
/// `background` photo; 6 = adds the show's `houseModel`; 7 = submodels and faces: regions get
/// an `id`, `nodes` regions become `lines` with a `layout` and `buffer` style, `subBuffer`
/// regions, face colors, and a group's `submodels`; 8 = file paths (sequences, music, the photo,
/// the house model) are relative to the show file when the file is inside its folder, and keep
/// bytes that aren't UTF-8 (see `paths.rs`); the file also records the folder it was saved in
/// (`savedIn`), so a show file moved on its own still finds its files.
pub const CURRENT_SCHEMA_VERSION: u32 = 8;

/// Show-wide settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowSettings {
    /// Output frames per second (20–100).
    #[serde(default = "default_frame_rate")]
    pub frame_rate: u16,
}

fn default_frame_rate() -> u16 {
    40
}

impl Default for ShowSettings {
    fn default() -> Self {
        Self {
            frame_rate: default_frame_rate(),
        }
    }
}

/// A complete show: layout, groups, and controller wiring.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Show {
    pub schema_version: u32,
    pub name: String,
    #[serde(default)]
    pub settings: ShowSettings,
    #[serde(default)]
    pub props: Vec<Prop>,
    #[serde(default)]
    pub groups: Vec<Group>,
    #[serde(default)]
    pub controllers: Vec<Controller>,
    /// Rendered sequences in the show, in playlist order.
    #[serde(default)]
    pub sequences: Vec<SequenceEntry>,
    /// A photo of the house drawn behind the layout, if the user chose one.
    #[serde(default)]
    pub background: Option<Background>,
    /// A 3D model of the house shown in the 3D view, if the user chose one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub house_model: Option<HouseModel>,
}

/// A photo drawn behind the layout so props can be placed over the real house.
///
/// Its height follows the image's own shape, so only the width is stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Background {
    /// The image file.
    pub path: String,
    /// Layout position of the photo's top-left corner.
    pub x: f32,
    pub y: f32,
    /// Width in layout units.
    pub width: f32,
    /// How strongly the photo shows, from 0 (hidden) to 1 (full strength).
    #[serde(default = "default_opacity")]
    pub opacity: f32,
}

fn default_opacity() -> f32 {
    1.0
}

impl Background {
    /// A photo at `path` with its top-left corner at (`x`, `y`), `width` units wide, at full
    /// strength.
    pub fn new(path: impl Into<String>, x: f32, y: f32, width: f32) -> Self {
        Self {
            path: path.into(),
            x,
            y,
            width,
            opacity: 1.0,
        }
    }

    /// Why this background can't be used, in plain language, or `None` when it's fine.
    pub fn problem(&self) -> Option<String> {
        if self.path.trim().is_empty() {
            return Some("Choose a photo file for the background.".into());
        }
        if !(self.x.is_finite() && self.y.is_finite()) {
            return Some("The background photo's position must be a number.".into());
        }
        if !(self.width.is_finite() && self.width > 0.0) {
            return Some("The background photo must be wider than zero.".into());
        }
        if !(0.0..=1.0).contains(&self.opacity) {
            return Some("The background photo's strength must be between 0% and 100%.".into());
        }
        None
    }
}

/// A 3D model of the house (glTF/GLB or OBJ file) for the 3D view, placed in layout units.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HouseModel {
    /// The model file.
    pub path: String,
    #[serde(default)]
    pub position: Vec3,
    /// Rotation about X, Y, Z in degrees.
    #[serde(default)]
    pub rotation_deg: Vec3,
    /// One factor for all three axes (a model's units rarely match the layout's).
    #[serde(default = "default_scale")]
    pub scale: f32,
    /// How solid the model looks, from 0 (hidden) to 1 (solid).
    #[serde(default = "default_opacity")]
    pub opacity: f32,
}

fn default_scale() -> f32 {
    1.0
}

impl HouseModel {
    /// The model at `path`, where its file puts it, solid.
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            position: Vec3::default(),
            rotation_deg: Vec3::default(),
            scale: 1.0,
            opacity: 1.0,
        }
    }

    /// Why this model can't be used, in plain language, or `None` when it's fine.
    pub fn problem(&self) -> Option<String> {
        let finite = |v: Vec3| v.x.is_finite() && v.y.is_finite() && v.z.is_finite();
        if self.path.trim().is_empty() {
            return Some("Choose a model file for the house.".into());
        }
        if !finite(self.position) || !finite(self.rotation_deg) {
            return Some("The house model's position and rotation must be numbers.".into());
        }
        if !(self.scale.is_finite() && self.scale > 0.0) {
            return Some("The house model's scale must be more than zero.".into());
        }
        if !(0.0..=1.0).contains(&self.opacity) {
            return Some("The house model's strength must be between 0% and 100%.".into());
        }
        None
    }
}

/// A rendered sequence (`.fseq`) in the show, with its music.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceEntry {
    pub id: SequenceId,
    pub name: String,
    /// The `.fseq` file.
    pub path: String,
    /// The music file, when the sequence has one.
    #[serde(default)]
    pub audio: Option<String>,
    /// How far the lights run ahead of the music, in milliseconds (negative: behind).
    #[serde(default)]
    pub offset_ms: i32,
}

impl SequenceEntry {
    pub fn new(name: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            id: SequenceId::new(),
            name: name.into(),
            path: path.into(),
            audio: None,
            offset_ms: 0,
        }
    }
}

impl Show {
    /// An empty show at the current schema version.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            name: name.into(),
            settings: ShowSettings::default(),
            props: Vec::new(),
            groups: Vec::new(),
            controllers: Vec::new(),
            sequences: Vec::new(),
            background: None,
            house_model: None,
        }
    }

    pub fn prop(&self, id: PropId) -> Option<&Prop> {
        self.props.iter().find(|p| p.id == id)
    }

    /// A prop's region (submodel or face), with the prop.
    pub fn region(&self, prop: PropId, region: RegionId) -> Option<(&Prop, &Region)> {
        let prop = self.prop(prop)?;
        Some((prop, prop.region(region)?))
    }

    pub fn controller(&self, id: ControllerId) -> Option<&Controller> {
        self.controllers.iter().find(|c| c.id == id)
    }

    /// Every file path the show holds (sequences, their music, the photo, the house model), to
    /// change in place.
    pub fn file_paths_mut(&mut self) -> impl Iterator<Item = &mut String> {
        let sequences = self
            .sequences
            .iter_mut()
            .flat_map(|s| std::iter::once(&mut s.path).chain(s.audio.as_mut()));
        sequences
            .chain(self.background.as_mut().map(|b| &mut b.path))
            .chain(self.house_model.as_mut().map(|m| &mut m.path))
    }

    /// The show as a file in `folder` stores it: files inside `folder` (or a folder below it)
    /// relative to it, other files in full (see [`crate::relative_text`]).
    pub fn with_paths_relative_to(&self, folder: &Path) -> Show {
        let mut show = self.clone();
        for path in show.file_paths_mut() {
            *path = crate::relative_text(path, folder);
        }
        show
    }

    /// Makes the show's relative file paths full ones, starting in `folder` (the folder of the
    /// file it was read from).
    pub fn resolve_paths(&mut self, folder: &Path) {
        for path in self.file_paths_mut() {
            *path = crate::resolve_text(path, folder);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Generator, ShapeSource};

    #[test]
    fn new_show_is_empty_at_current_version() {
        let show = Show::new("Demo");
        assert_eq!(show.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(show.settings.frame_rate, 40);
        assert!(show.props.is_empty());
    }

    #[test]
    fn lookup_by_id() {
        let mut show = Show::new("Demo");
        let prop = Prop::new(
            "Line",
            ShapeSource::Generator(Generator::Line {
                nodes: 5,
                length: 1.0,
            }),
        );
        let id = prop.id;
        show.props.push(prop);
        assert_eq!(show.prop(id).unwrap().name, "Line");
        assert!(show.prop(PropId::new()).is_none());
    }

    #[test]
    fn house_model_problems_are_explained() {
        let ok = HouseModel::new("/models/house.glb");
        assert_eq!(ok.problem(), None);
        let cases = [
            (
                HouseModel {
                    path: "".into(),
                    ..ok.clone()
                },
                "Choose a model",
            ),
            (
                HouseModel {
                    position: Vec3::new(f32::NAN, 0.0, 0.0),
                    ..ok.clone()
                },
                "must be numbers",
            ),
            (
                HouseModel {
                    scale: 0.0,
                    ..ok.clone()
                },
                "more than zero",
            ),
            (
                HouseModel {
                    opacity: -0.1,
                    ..ok.clone()
                },
                "between 0% and 100%",
            ),
        ];
        for (model, expected) in cases {
            let problem = model.problem().expect("a problem");
            assert!(problem.contains(expected), "{problem}");
        }
    }

    #[test]
    fn background_problems_are_explained() {
        let ok = Background::new("/photos/house.jpg", -10.0, 8.0, 20.0);
        assert_eq!(ok.problem(), None);
        let cases = [
            (
                Background {
                    path: " ".into(),
                    ..ok.clone()
                },
                "Choose a photo",
            ),
            (
                Background {
                    x: f32::NAN,
                    ..ok.clone()
                },
                "position",
            ),
            (
                Background {
                    width: 0.0,
                    ..ok.clone()
                },
                "wider than zero",
            ),
            (
                Background {
                    width: f32::INFINITY,
                    ..ok.clone()
                },
                "wider than zero",
            ),
            (
                Background {
                    opacity: 1.5,
                    ..ok.clone()
                },
                "between 0% and 100%",
            ),
        ];
        for (background, expected) in cases {
            let problem = background.problem().expect("a problem");
            assert!(problem.contains(expected), "{problem}");
        }
    }

    fn show_with_files() -> Show {
        let mut show = Show::new("Files");
        let mut medley = SequenceEntry::new("Medley", "/Shows/Haas/Christmas Medley 2017.fseq");
        medley.audio = Some("/Shows/Haas/MP3 Music/Christmas Medley 2017.mp3".into());
        show.sequences.push(medley);
        show.sequences
            .push(SequenceEntry::new("Elsewhere", "/Volumes/USB/Wizards.fseq"));
        show.background = Some(Background::new("/Shows/Haas/photos/house.jpg", 0.0, 0.0, 10.0));
        show.house_model = Some(HouseModel::new("/Users/me/Models/house.glb"));
        show
    }

    #[test]
    fn files_in_the_show_folder_are_stored_relative_and_read_back_in_full() {
        let show = show_with_files();
        let stored = show.with_paths_relative_to(Path::new("/Shows/Haas"));
        assert_eq!(stored.sequences[0].path, "Christmas Medley 2017.fseq");
        assert_eq!(
            stored.sequences[0].audio.as_deref(),
            Some("MP3 Music/Christmas Medley 2017.mp3")
        );
        assert_eq!(
            stored.sequences[1].path, "/Volumes/USB/Wizards.fseq",
            "outside: in full"
        );
        assert_eq!(stored.sequences[1].audio, None);
        assert_eq!(stored.background.as_ref().unwrap().path, "photos/house.jpg");
        assert_eq!(
            stored.house_model.as_ref().unwrap().path,
            "/Users/me/Models/house.glb"
        );

        // The folder moved: the relative files follow it, the others stay put.
        let mut moved = stored.clone();
        moved.resolve_paths(Path::new("/Users/me/Shows/Haas"));
        assert_eq!(
            moved.sequences[0].path,
            "/Users/me/Shows/Haas/Christmas Medley 2017.fseq"
        );
        assert_eq!(
            moved.sequences[0].audio.as_deref(),
            Some("/Users/me/Shows/Haas/MP3 Music/Christmas Medley 2017.mp3")
        );
        assert_eq!(moved.sequences[1].path, "/Volumes/USB/Wizards.fseq");
        assert_eq!(
            moved.background.as_ref().unwrap().path,
            "/Users/me/Shows/Haas/photos/house.jpg"
        );

        let mut same = stored;
        same.resolve_paths(Path::new("/Shows/Haas"));
        assert_eq!(same, show);
    }

    #[test]
    fn background_json_is_camel_case_and_opacity_defaults_to_full() {
        let json = r#"{ "path": "/p.jpg", "x": 1, "y": 2, "width": 3 }"#;
        let background: Background = serde_json::from_str(json).unwrap();
        assert_eq!(background, Background::new("/p.jpg", 1.0, 2.0, 3.0));
        let value = serde_json::to_value(Show::new("x")).unwrap();
        assert_eq!(value["background"], serde_json::Value::Null);
    }
}
