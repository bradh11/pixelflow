//! Picture effects in the engine: a picture copied into the show's images folder is previewed,
//! played, and exported; one that's missing draws nothing and is named in the sequence's
//! problems, then found again or pointed elsewhere as one undo step; and only the images folder
//! and files the user chose are ever read.

use pf_engine::{DraftRenderer, Edit, Engine, FileRole, SequenceEdit};
use pf_model::{Controller, Generator, Port, PortSlot, Prop, Protocol, ShapeSource, path_to_text};
use pf_sequence::{Effect, EffectId, EffectKind, EffectParams, PictureFit, PictureParams, Row, Target};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// A PNG of one color.
fn png(color: [u8; 3]) -> Vec<u8> {
    let picture = image::RgbImage::from_pixel(2, 2, image::Rgb(color));
    let mut bytes = Cursor::new(Vec::new());
    picture.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    bytes.into_inner()
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

/// A 10-pixel strip wired to a controller.
fn strip() -> Vec<Edit> {
    let prop = Prop::new(
        "Strip",
        ShapeSource::Generator(Generator::Line {
            nodes: 10,
            length: 1.0,
        }),
    );
    let mut controller = Controller::new("Bench", "127.0.0.1:4048", Protocol::Ddp);
    let mut port = Port::new(1);
    port.slots.push(PortSlot::new(prop.id));
    controller.ports.push(port);
    vec![Edit::AddProp { prop }, Edit::AddController { controller }]
}

/// An engine with the strip, saved as a show in `folder/Show`, and an open sequence with a
/// Picture effect on the strip for its whole length. Returns the show's folder and the effect.
fn engine(folder: &Path, file: &str) -> (Engine, PathBuf, EffectId) {
    let mut engine = Engine::new(folder.join("data")).with_home(None);
    engine.apply(strip()).unwrap();
    let show = folder.join("Show");
    std::fs::create_dir_all(&show).unwrap();
    engine.save_as(&show.join("House.pixelflow.json")).unwrap();
    engine.new_sequence_doc("Song", 4_000, None).unwrap();
    let mut row = Row::new(Target::Prop(engine.show().props[0].id));
    let effect = Effect::new(EffectKind::Picture, 0, 4_000).with_params(picture(file));
    let id = effect.id;
    row.layers[0].effects.push(effect);
    engine
        .edit_sequence(vec![SequenceEdit::AddRow { row, index: None }])
        .unwrap();
    (engine, show, id)
}

fn picture(file: &str) -> EffectParams {
    EffectParams::Picture(PictureParams {
        file: file.to_string(),
        fit: PictureFit::Stretch,
        ..PictureParams::default()
    })
}

fn file_of(engine: &Engine) -> String {
    let doc = engine.sequence_document().unwrap();
    let EffectParams::Picture(p) = &doc.rows[0].layers[0].effects[0].params else {
        panic!("a picture")
    };
    p.file.clone()
}

/// The strip's color in the preview at 1 second, once it has settled: a preview draws without a
/// picture until it has been read.
fn shown(engine: &mut Engine) -> [u8; 3] {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let frame = engine.sequence_doc_frame(1_000).unwrap();
        assert!(
            frame.chunks(3).all(|px| px == &frame[..3]),
            "one color: {frame:?}"
        );
        if frame[..3] != [0, 0, 0] || Instant::now() > deadline {
            return [frame[0], frame[1], frame[2]];
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Whether the strip stays dark: drawn once (which asks for the picture), then again once the
/// picture is known not to be readable.
fn dark(engine: &mut Engine) -> bool {
    let asked = engine.sequence_doc_frame(1_000).unwrap();
    let file = file_of(engine);
    let deadline = Instant::now() + Duration::from_secs(5);
    while engine.pictures().problem(&file).is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(engine.pictures().problem(&file).is_some(), "{file} was read");
    let settled = engine.sequence_doc_frame(1_000).unwrap();
    asked.iter().chain(&settled).all(|&b| b == 0)
}

#[test]
fn a_picture_in_the_images_folder_is_previewed_and_exported() {
    let dir = tempfile::tempdir().unwrap();
    let (mut engine, show, _) = engine(dir.path(), "images/red.png");
    write(&show.join("images/red.png"), &png([255, 0, 0]));
    assert_eq!(engine.check_sequence_pictures(), vec![]);
    assert_eq!(shown(&mut engine), [255, 0, 0]);
    assert_eq!(engine.sequence_doc().unwrap().issues, vec![]);
    // An export waits for its pictures: the first frame it draws has them.
    let export = engine.sequence_export().unwrap();
    let mut renderer = DraftRenderer::new(export.show());
    renderer.set_pictures(export.pictures());
    let fresh = Engine::new(dir.path().join("other"));
    assert!(!export.pictures().same(&fresh.pictures()));
    assert!(
        renderer
            .frame(export.sequence(), 0)
            .chunks(3)
            .all(|px| px == [255, 0, 0])
    );
    let out = dir.path().join("song.fseq");
    let summary = export.run(&out, |_, _| true).unwrap();
    assert!(summary.frames > 0 && out.is_file());
    // The file is saved by what the effect stores: relative to the show, so the folder can move.
    let seq = show.join("Song.pfseq.json");
    engine.save_sequence_doc_as(&seq).unwrap();
    let text = std::fs::read_to_string(&seq).unwrap();
    assert!(text.contains(r#""file": "images/red.png""#), "{text}");
    let moved = dir.path().join("Moved");
    std::fs::rename(&show, &moved).unwrap();
    let mut engine = Engine::new(dir.path().join("data2")).with_home(None);
    engine.open(&moved.join("House.pixelflow.json")).unwrap();
    engine.open_sequence_doc(&moved.join("Song.pfseq.json")).unwrap();
    assert_eq!(engine.check_sequence_pictures(), vec![]);
    assert_eq!(shown(&mut engine), [255, 0, 0]);
}

#[test]
fn a_missing_picture_draws_nothing_is_named_and_is_found_again() {
    let dir = tempfile::tempdir().unwrap();
    let (mut engine, show, effect) = engine(dir.path(), "images/santa dancing.gif");
    // Nothing is read under the engine: the check is copied out, run, and handed back.
    let check = engine.sequence_picture_check().unwrap();
    assert_eq!(check.len(), 1);
    let missing = engine.publish_picture_status(check.run());
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].file, FileRole::Picture);
    assert_eq!(missing[0].name, "santa dancing.gif");
    assert_eq!(missing[0].message, "santa dancing.gif isn't where it was.");
    assert_eq!(
        missing[0].path,
        path_to_text(&show.join("images/santa dancing.gif"))
    );
    assert_eq!(missing[0].owner, "Picture effect at 0:00.000");
    assert!(dark(&mut engine), "it draws nothing, and nothing crashes");
    // The sequence's problems name the file and point at the effect.
    let issues = engine.sequence_doc().unwrap().issues;
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0].effect, Some(effect));
    assert!(
        issues[0]
            .message
            .starts_with("The Picture effect at 0:00.000 shows nothing: santa dancing.gif isn't"),
        "{}",
        issues[0].message
    );
    // An edit's reply carries them too.
    let reply = engine
        .edit_sequence(vec![SequenceEdit::SetEffectTiming {
            id: effect,
            start_ms: 0,
            end_ms: 3_000,
        }])
        .unwrap();
    assert!(
        reply
            .issues
            .iter()
            .any(|i| i.message.contains("santa dancing.gif"))
    );

    // It turns up elsewhere in the show's folder: found by name and copied into images, where
    // the effect was looking all along.
    write(&show.join("Assets/santa dancing.gif"), &png([0, 255, 0]));
    let search = engine.sequence_picture_search().unwrap();
    assert_eq!(search.folders(), std::slice::from_ref(&show));
    let found = search.run();
    assert_eq!(found.found.len(), 1);
    assert_eq!(found.found[0].from, missing[0].path);
    let files = engine.picture_files();
    let stored = files.adopt(Path::new(&found.found[0].to)).unwrap();
    assert_eq!(stored, "images/santa dancing.gif");
    let changes = vec![(found.found[0].from.clone(), stored)];
    let result = engine.relink_sequence_pictures(&changes).unwrap();
    assert!(
        !result.changed,
        "the effect already says images/santa dancing.gif"
    );
    assert_eq!(file_of(&engine), "images/santa dancing.gif");
    assert_eq!(engine.sequence_pictures_missing(), vec![]);
    assert_eq!(engine.check_sequence_pictures(), vec![]);
    assert_eq!(shown(&mut engine), [0, 255, 0]);
    assert!(engine.sequence_doc().unwrap().issues.is_empty());
    // With nothing missing there's nothing to search for.
    assert!(engine.sequence_picture_search().is_err());
}

#[test]
fn a_picture_from_another_computer_is_found_by_name_and_its_effects_follow() {
    let dir = tempfile::tempdir().unwrap();
    let elsewhere = r"C:\Users\someone\xLights\Images\elf.png";
    let (mut engine, show, _) = engine(dir.path(), elsewhere);
    // A second effect with the same picture.
    let row = engine.sequence_document().unwrap().rows[0].id;
    let again = Effect::new(EffectKind::Picture, 0, 1_000).with_params(picture(elsewhere));
    engine
        .edit_sequence(vec![SequenceEdit::AddEffect {
            row,
            layer: 1,
            effect: again,
        }])
        .unwrap();
    let missing = engine.check_sequence_pictures();
    assert_eq!(missing.len(), 1, "one file, however many effects");
    assert_eq!(missing[0].name, "elf.png");
    write(&show.join("Assets/elf.png"), &png([0, 0, 255]));
    let found = engine.sequence_picture_search().unwrap().run();
    let stored = engine
        .picture_files()
        .adopt(Path::new(&found.found[0].to))
        .unwrap();
    assert_eq!(stored, "images/elf.png");
    engine
        .relink_sequence_pictures(&[(missing[0].path.clone(), stored)])
        .unwrap();
    let doc = engine.sequence_document().unwrap();
    let files: Vec<&str> = doc
        .effects()
        .filter_map(|e| match &e.params {
            EffectParams::Picture(p) => Some(p.file.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(files, ["images/elf.png", "images/elf.png"]);
    assert_eq!(engine.check_sequence_pictures(), vec![]);
    assert_eq!(shown(&mut engine), [0, 0, 255]);
    // Both effects went back together.
    engine.undo_sequence().unwrap();
    assert!(
        engine
            .sequence_document()
            .unwrap()
            .effects()
            .all(|e| match &e.params {
                EffectParams::Picture(p) => p.file == elsewhere,
                _ => true,
            })
    );
}

#[test]
fn only_the_images_folder_and_chosen_files_are_read() {
    let dir = tempfile::tempdir().unwrap();
    let outside = dir.path().join("Desktop/secret.png");
    write(&outside, &png([255, 255, 255]));
    // A path an edit put in an effect isn't enough: the file is there, but it isn't read.
    let (mut engine, show, effect) = engine(dir.path(), &path_to_text(&outside));
    let missing = engine.check_sequence_pictures();
    assert_eq!(missing.len(), 1);
    assert_eq!(
        missing[0].message,
        "secret.png isn't in the show's images folder, so PixelFlow doesn't read it."
    );
    assert!(dark(&mut engine));
    // Nor is one reached by climbing out of the images folder.
    write(&show.join("notes.png"), &png([9, 9, 9]));
    engine
        .edit_sequence(vec![SequenceEdit::SetEffectParams {
            id: effect,
            params: picture("images/../notes.png"),
        }])
        .unwrap();
    assert_eq!(engine.check_sequence_pictures().len(), 1);
    assert!(dark(&mut engine));
    // Chosen by the user, the outside file is copied into the show and read from there.
    let stored = engine.picture_files().adopt(&outside).unwrap();
    assert_eq!(stored, "images/secret.png");
    engine
        .edit_sequence(vec![SequenceEdit::SetEffectParams {
            id: effect,
            params: picture(&stored),
        }])
        .unwrap();
    assert_eq!(engine.check_sequence_pictures(), vec![]);
    assert_eq!(shown(&mut engine), [255, 255, 255]);
}

#[test]
fn a_picture_that_changes_on_disk_is_read_again_at_the_next_check() {
    let dir = tempfile::tempdir().unwrap();
    let (mut engine, show, _) = engine(dir.path(), "images/sign.png");
    let sign = show.join("images/sign.png");
    write(&sign, &png([255, 0, 0]));
    engine.check_sequence_pictures();
    assert_eq!(shown(&mut engine), [255, 0, 0]);
    // Repainted (another size, so it's another file whatever the clock says).
    let green = image::RgbImage::from_pixel(5, 3, image::Rgb([0, 255, 0]));
    let mut bytes = Cursor::new(Vec::new());
    green.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    write(&sign, &bytes.into_inner());
    assert_eq!(
        shown(&mut engine),
        [255, 0, 0],
        "not noticed until it's looked at"
    );
    assert_eq!(engine.check_sequence_pictures(), vec![]);
    let deadline = Instant::now() + Duration::from_secs(5);
    while shown(&mut engine) != [0, 255, 0] {
        assert!(Instant::now() < deadline, "the new picture never showed");
        std::thread::sleep(Duration::from_millis(5));
    }
    // Deleted: missing at the next check, and dark.
    std::fs::remove_file(&sign).unwrap();
    assert_eq!(engine.check_sequence_pictures().len(), 1);
    assert!(dark(&mut engine));
    // Something that isn't a picture under its name: there, but it can't be read.
    write(&sign, b"\x89PNG but not really");
    engine.check_sequence_pictures();
    assert!(dark(&mut engine));
    let missing = engine.check_sequence_pictures();
    assert_eq!(missing.len(), 1);
    assert!(
        missing[0].message.starts_with("sign.png couldn't be read: "),
        "{}",
        missing[0].message
    );
}

#[test]
fn an_unsaved_show_uses_a_chosen_picture_where_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = Engine::new(dir.path().join("data")).with_home(None);
    engine.apply(strip()).unwrap();
    let chosen = dir.path().join("Downloads/star.png");
    write(&chosen, &png([255, 255, 0]));
    // No show folder yet, so no images folder: the file itself is used, and may be read.
    let stored = engine.picture_files().adopt(&chosen).unwrap();
    assert_eq!(stored, path_to_text(&chosen));
    engine.new_sequence_doc("Song", 4_000, None).unwrap();
    let mut row = Row::new(Target::Prop(engine.show().props[0].id));
    row.layers[0]
        .effects
        .push(Effect::new(EffectKind::Picture, 0, 4_000).with_params(picture(&stored)));
    engine
        .edit_sequence(vec![SequenceEdit::AddRow { row, index: None }])
        .unwrap();
    assert_eq!(engine.check_sequence_pictures(), vec![]);
    assert_eq!(shown(&mut engine), [255, 255, 0]);
    // A relative name has nowhere to be looked for until the show is saved.
    let id = engine.sequence_document().unwrap().rows[0].layers[0].effects[0].id;
    engine
        .edit_sequence(vec![SequenceEdit::SetEffectParams {
            id,
            params: picture("images/star.png"),
        }])
        .unwrap();
    assert_eq!(engine.check_sequence_pictures().len(), 1);
    // Saved: the images folder is the place, and what's in it shows.
    let show = dir.path().join("Show");
    write(&show.join("images/star.png"), &png([0, 255, 255]));
    engine.save_as(&show.join("House.pixelflow.json")).unwrap();
    assert_eq!(engine.check_sequence_pictures(), vec![]);
    assert_eq!(shown(&mut engine), [0, 255, 255]);
    assert_eq!(engine.picture_files().listed(), ["images/star.png"]);
}
