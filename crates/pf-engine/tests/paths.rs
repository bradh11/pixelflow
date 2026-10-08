//! File paths in the show: stored relative to the show file, followed when the folder moves,
//! and found again when a file isn't where it was.

use pf_engine::{Edit, Engine, FileRole};
use pf_model::{Background, HouseModel, SequenceEntry, path_to_text};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn touch(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, b"x").unwrap();
}

fn text(path: &Path) -> String {
    path_to_text(path)
}

/// A show folder with a sequence and its music, the photo, and a house model kept elsewhere.
struct Folder {
    _dir: tempfile::TempDir,
    root: PathBuf,
    show: PathBuf,
    data: PathBuf,
}

fn folder() -> Folder {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("Haas 2024");
    touch(&root.join("Christmas Medley 2017.fseq"));
    touch(&root.join("MP3 Music/Christmas Medley 2017.mp3"));
    touch(&root.join("photos/house.jpg"));
    touch(&dir.path().join("Models/house.glb"));
    Folder {
        show: root.join("show.pixelflow.json"),
        data: dir.path().join("data"),
        root,
        _dir: dir,
    }
}

fn medley(root: &Path) -> SequenceEntry {
    let mut entry = SequenceEntry::new("Medley", text(&root.join("Christmas Medley 2017.fseq")));
    entry.audio = Some(text(&root.join("MP3 Music/Christmas Medley 2017.mp3")));
    entry
}

fn build(engine: &mut Engine, f: &Folder) -> SequenceEntry {
    let entry = medley(&f.root);
    let models = f.root.parent().unwrap().join("Models/house.glb");
    engine
        .apply(vec![
            Edit::AddSequence {
                sequence: entry.clone(),
            },
            Edit::SetBackground {
                background: Some(Background::new(
                    text(&f.root.join("photos/house.jpg")),
                    0.0,
                    0.0,
                    10.0,
                )),
            },
            Edit::SetHouseModel {
                house_model: Some(HouseModel::new(text(&models))),
            },
        ])
        .unwrap();
    entry
}

fn saved_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn files_in_the_show_folder_are_saved_relative_and_opened_in_full() {
    let f = folder();
    let mut engine = Engine::new(&f.data);
    let entry = build(&mut engine, &f);
    engine.save_as(&f.show).unwrap();

    let json = saved_json(&f.show);
    assert_eq!(json["schemaVersion"], pf_model::CURRENT_SCHEMA_VERSION);
    assert_eq!(json["savedIn"], text(&f.root));
    assert_eq!(json["sequences"][0]["path"], "Christmas Medley 2017.fseq");
    assert_eq!(
        json["sequences"][0]["audio"],
        "MP3 Music/Christmas Medley 2017.mp3"
    );
    assert_eq!(json["background"]["path"], "photos/house.jpg");
    let model = f.root.parent().unwrap().join("Models/house.glb");
    assert_eq!(
        json["houseModel"]["path"],
        text(&model),
        "outside the folder: in full"
    );

    // Saving doesn't change the show the engine holds (and leaves nothing unsaved).
    engine.check_files();
    let snap = engine.snapshot();
    assert!(snap.files_checked);
    assert!(!snap.dirty);
    assert_eq!(snap.show.sequences[0], entry);
    assert!(snap.missing_files.is_empty(), "{:?}", snap.missing_files);

    let mut reopened = Engine::new(&f.data);
    let snap = reopened.open(&f.show).unwrap();
    assert_eq!(snap.show.sequences[0], entry);
    assert_eq!(
        snap.show.background.unwrap().path,
        text(&f.root.join("photos/house.jpg"))
    );
    assert!(snap.missing_files.is_empty());
}

#[test]
fn a_moved_show_folder_keeps_its_files() {
    let f = folder();
    let mut engine = Engine::new(&f.data);
    build(&mut engine, &f);
    engine.save_as(&f.show).unwrap();

    let moved = f.root.parent().unwrap().join("Synced/Haas 2024");
    fs::create_dir_all(moved.parent().unwrap()).unwrap();
    fs::rename(&f.root, &moved).unwrap();

    let mut other = Engine::new(&f.data);
    let snap = other.open(&moved.join("show.pixelflow.json")).unwrap();
    assert_eq!(
        snap.show.sequences[0].path,
        text(&moved.join("Christmas Medley 2017.fseq"))
    );
    assert_eq!(
        snap.show.sequences[0].audio.as_deref(),
        Some(text(&moved.join("MP3 Music/Christmas Medley 2017.mp3")).as_str())
    );
    assert!(snap.missing_files.is_empty(), "{:?}", snap.missing_files);
}

#[test]
fn save_as_elsewhere_keeps_pointing_at_the_same_files() {
    let f = folder();
    let mut engine = Engine::new(&f.data);
    build(&mut engine, &f);
    engine.save_as(&f.show).unwrap();

    // Into a subfolder: still relative, from there.
    let below = f.root.join("photos/copy.pixelflow.json");
    engine.save_as(&below).unwrap();
    let json = saved_json(&below);
    assert_eq!(json["background"]["path"], "house.jpg");
    assert_eq!(
        json["sequences"][0]["path"],
        text(&f.root.join("Christmas Medley 2017.fseq")),
        "now outside the show's folder: in full"
    );

    // Somewhere else entirely: everything in full, nothing copied.
    let away = f.root.parent().unwrap().join("Away/show.pixelflow.json");
    engine.save_as(&away).unwrap();
    let json = saved_json(&away);
    assert_eq!(json["background"]["path"], text(&f.root.join("photos/house.jpg")));
    assert!(!away.parent().unwrap().join("photos").exists());
    let mut reopened = Engine::new(&f.data);
    assert!(reopened.open(&away).unwrap().missing_files.is_empty());
}

#[test]
fn autosaved_versions_restore_against_the_show_file() {
    let f = folder();
    let mut engine = Engine::new(&f.data);
    let entry = build(&mut engine, &f);
    engine.save_as(&f.show).unwrap();
    engine
        .apply(vec![Edit::RenameShow { name: "Later".into() }])
        .unwrap();
    engine.autosave().unwrap().expect("a version");
    let id = engine.history()[0].id.clone();
    engine.apply(vec![Edit::RemoveSequence { id: entry.id }]).unwrap();
    let snap = engine.restore(&id).unwrap();
    assert_eq!(snap.show.name, "Later");
    assert_eq!(
        snap.show.sequences[0], entry,
        "relative paths start at the show file"
    );
}

#[test]
fn missing_files_are_listed_plainly() {
    let f = folder();
    let mut engine = Engine::new(&f.data);
    let entry = build(&mut engine, &f);
    fs::remove_file(f.root.join("MP3 Music/Christmas Medley 2017.mp3")).unwrap();
    fs::remove_file(f.root.join("photos/house.jpg")).unwrap();
    engine.check_files();
    let missing = engine.snapshot().missing_files;
    assert_eq!(missing.len(), 2, "{missing:?}");
    assert_eq!(missing[0].file, FileRole::Music { id: entry.id });
    assert_eq!(missing[0].name, "Christmas Medley 2017.mp3");
    assert_eq!(
        missing[0].message,
        "Christmas Medley 2017.mp3 isn't where it was."
    );
    assert_eq!(missing[0].owner, "Music for Medley");
    assert_eq!(missing[0].was_at, missing[0].path, "nothing better known");
    assert_eq!(missing[1].file, FileRole::Photo);
    assert_eq!(missing[1].owner, "Background photo");

    // Playing a sequence whose file is gone says so plainly.
    fs::remove_file(f.root.join("Christmas Medley 2017.fseq")).unwrap();
    let error = engine.play_sequence(entry.id, 0).unwrap_err().to_string();
    assert_eq!(
        error,
        "Christmas Medley 2017.fseq isn't where it was. Use Find again or Locate… to show PixelFlow where it is now."
    );
}

#[test]
fn finding_missing_files_repoints_them_in_one_undo_step() {
    let f = folder();
    let mut engine = Engine::new(&f.data);
    let entry = build(&mut engine, &f);
    engine.save_as(&f.show).unwrap();
    // Reorganized: the music moved into another folder, the photo into a deeper one.
    fs::create_dir_all(f.root.join("Audio")).unwrap();
    fs::rename(
        f.root.join("MP3 Music/Christmas Medley 2017.mp3"),
        f.root.join("Audio/Christmas Medley 2017.mp3"),
    )
    .unwrap();
    fs::create_dir_all(f.root.join("pics/2024")).unwrap();
    fs::rename(
        f.root.join("photos/house.jpg"),
        f.root.join("pics/2024/house.jpg"),
    )
    .unwrap();
    // The fseq is gone for good.
    fs::remove_file(f.root.join("Christmas Medley 2017.fseq")).unwrap();
    engine.check_files();
    assert_eq!(engine.snapshot().missing_files.len(), 3);

    let search = engine.file_search().unwrap();
    assert_eq!(search.folders(), std::slice::from_ref(&f.root));
    let found = search.run();
    let report = engine.use_found_files(found).unwrap();
    assert_eq!(report.found.len(), 2, "{:?}", report.found);
    assert_eq!(report.found[0].name, "Christmas Medley 2017.mp3");
    assert_eq!(
        report.found[0].to,
        text(&f.root.join("Audio/Christmas Medley 2017.mp3"))
    );
    assert_eq!(report.found[1].file, FileRole::Photo);
    assert_eq!(report.still_missing.len(), 1);
    assert_eq!(report.still_missing[0].file, FileRole::Sequence { id: entry.id });
    let snap = report.snapshot;
    assert_eq!(snap.missing_files.len(), 1);
    assert!(snap.dirty);

    // One undo puts both back.
    let snap = engine.undo();
    assert_eq!(snap.show.sequences[0], entry);
    assert_eq!(snap.missing_files.len(), 3);
    assert!(snap.can_redo);
    assert_eq!(
        engine.redo().missing_files.len(),
        1,
        "and one redo brings them back"
    );
}

#[test]
fn found_files_are_only_used_while_the_show_still_points_at_the_old_place() {
    let f = folder();
    let mut engine = Engine::new(&f.data);
    let entry = build(&mut engine, &f);
    engine.save_as(&f.show).unwrap();
    fs::create_dir_all(f.root.join("Audio")).unwrap();
    fs::rename(
        f.root.join("MP3 Music/Christmas Medley 2017.mp3"),
        f.root.join("Audio/Christmas Medley 2017.mp3"),
    )
    .unwrap();
    engine.check_files();
    let found = engine.file_search().unwrap().run();
    assert_eq!(found.found.len(), 1);
    // Meanwhile the user chose other music.
    let mut changed = entry.clone();
    changed.audio = Some(text(&f.root.join("photos/house.jpg")));
    engine
        .apply(vec![Edit::UpdateSequence {
            sequence: changed.clone(),
        }])
        .unwrap();
    let report = engine.use_found_files(found).unwrap();
    assert!(report.found.is_empty());
    assert_eq!(report.snapshot.show.sequences[0], changed);
}

#[test]
fn finding_one_file_again_leaves_the_others_alone() {
    let f = folder();
    let mut engine = Engine::new(&f.data);
    build(&mut engine, &f);
    engine.save_as(&f.show).unwrap();
    fs::create_dir_all(f.root.join("Audio")).unwrap();
    fs::rename(
        f.root.join("MP3 Music/Christmas Medley 2017.mp3"),
        f.root.join("Audio/Christmas Medley 2017.mp3"),
    )
    .unwrap();
    fs::rename(f.root.join("photos/house.jpg"), f.root.join("house.jpg")).unwrap();
    engine.check_files();
    let search = engine.file_search().unwrap().only(FileRole::Photo);
    assert_eq!(search.wanted().len(), 1);
    let report = engine.use_found_files(search.run()).unwrap();
    assert_eq!(report.found.len(), 1);
    assert_eq!(report.found[0].file, FileRole::Photo);
    assert_eq!(report.still_missing.len(), 1, "the music is still to find");
}

#[test]
fn an_unsaved_show_has_no_folder_to_search() {
    let f = folder();
    let mut engine = Engine::new(&f.data);
    build(&mut engine, &f);
    let error = engine.file_search().unwrap_err().to_string();
    assert_eq!(
        error,
        "Save the show first, so PixelFlow knows which folder to look in. Or use Locate… to choose the file."
    );
}

#[test]
fn locating_a_file_repoints_it_as_one_undo_step() {
    let f = folder();
    let mut engine = Engine::new(&f.data);
    let entry = build(&mut engine, &f);
    let elsewhere = f.root.parent().unwrap().join("USB/Medley.fseq");
    touch(&elsewhere);
    let snap = engine
        .relink_file(FileRole::Sequence { id: entry.id }, &elsewhere)
        .unwrap();
    assert_eq!(snap.show.sequences[0].path, text(&elsewhere));
    assert_eq!(snap.show.sequences[0].audio, entry.audio, "music stays");
    let model = f.root.join("photos/house.jpg");
    let snap = engine.relink_file(FileRole::HouseModel, &model).unwrap();
    assert_eq!(snap.show.house_model.unwrap().path, text(&model));
    let snap = engine.undo();
    assert_eq!(snap.show.sequences[0].path, text(&elsewhere));

    let gone = engine
        .relink_file(
            FileRole::Music {
                id: pf_model::SequenceId::new(),
            },
            &model,
        )
        .unwrap_err();
    assert_eq!(gone.to_string(), "There is no sequence with that id.");
    // What the user chose is checked before the engine is asked (without holding it).
    let error = pf_engine::check_chosen_file(&f.root.join("nothing.jpg")).unwrap_err();
    assert!(pf_engine::check_chosen_file(&model).is_ok());
    assert_eq!(
        error.to_string(),
        "nothing.jpg isn't there anymore. Choose another file."
    );
}

#[cfg(unix)]
#[test]
fn file_names_that_are_not_utf8_survive_saving_and_opening() {
    use std::os::unix::ffi::OsStringExt;
    let f = folder();
    // "Café.mp3" with a Latin-1 é, as older Linux systems name files.
    let name = std::ffi::OsString::from_vec(b"Caf\xe9.mp3".to_vec());
    let song = f.root.join("MP3 Music").join(&name);
    // Some file systems (macOS) only take UTF-8 names; the path must survive either way.
    let on_disk = fs::write(&song, b"x").is_ok();
    let mut engine = Engine::new(&f.data);
    let mut entry = medley(&f.root);
    entry.audio = Some(text(&song));
    engine
        .apply(vec![Edit::AddSequence {
            sequence: entry.clone(),
        }])
        .unwrap();
    engine.save_as(&f.show).unwrap();
    assert_eq!(
        saved_json(&f.show)["sequences"][0]["audio"],
        "MP3 Music/Caf\u{0}e9.mp3"
    );
    let mut reopened = Engine::new(&f.data);
    let snap = reopened.open(&f.show).unwrap();
    assert_eq!(snap.show.sequences[0].audio, entry.audio);
    assert_eq!(
        pf_model::path_from_text(snap.show.sequences[0].audio.as_deref().unwrap()),
        song
    );
    assert_eq!(snap.missing_files.is_empty(), on_disk);
    if !on_disk {
        assert_eq!(snap.missing_files[0].name, "Caf\u{FFFD}.mp3");
    }
}

#[test]
fn snapshots_never_read_the_disk_files_are_checked_apart() {
    let f = folder();
    let mut engine = Engine::new(&f.data);
    build(&mut engine, &f);
    // Nothing checked yet: nothing is called missing, and the snapshot says it hasn't looked.
    let snap = engine.snapshot();
    assert!(!snap.files_checked);
    assert!(snap.missing_files.is_empty());
    // The check is copied out (to run without the engine), run, then handed back.
    let check = engine.file_check(false);
    assert_eq!(check.len(), 4);
    fs::remove_file(f.root.join("photos/house.jpg")).unwrap();
    engine.publish_file_status(check.run());
    let snap = engine.snapshot();
    assert!(snap.files_checked);
    assert_eq!(snap.missing_files.len(), 1);

    // A file coming back shows only after another check: snapshots don't look.
    touch(&f.root.join("photos/house.jpg"));
    assert_eq!(engine.snapshot().missing_files.len(), 1);
    assert_eq!(engine.file_check(false).len(), 0, "nothing new to check");
    let all = engine.file_check(true);
    assert_eq!(all.len(), 4);
    engine.publish_file_status(all.run());
    assert!(engine.snapshot().missing_files.is_empty());

    // A path an edit brings in waits for the next check.
    let photo = Background::new(text(&f.root.join("other.jpg")), 0.0, 0.0, 10.0);
    engine
        .apply(vec![Edit::SetBackground {
            background: Some(photo),
        }])
        .unwrap();
    let snap = engine.snapshot();
    assert!(!snap.files_checked);
    assert!(snap.missing_files.is_empty());
    engine.publish_file_status(engine.file_check(false).run());
    assert_eq!(engine.snapshot().missing_files[0].name, "other.jpg");
}

#[test]
fn a_check_or_search_from_before_another_show_opened_is_dropped() {
    let f = folder();
    let mut engine = Engine::new(&f.data);
    build(&mut engine, &f);
    engine.save_as(&f.show).unwrap();
    fs::remove_file(f.root.join("photos/house.jpg")).unwrap();
    let stale = engine.file_check(true).run();
    touch(&f.root.join("photos/house.jpg"));
    fs::create_dir_all(f.root.join("Audio")).unwrap();
    fs::rename(
        f.root.join("MP3 Music/Christmas Medley 2017.mp3"),
        f.root.join("Audio/Christmas Medley 2017.mp3"),
    )
    .unwrap();
    engine.check_files();
    let search = engine.file_search().unwrap().run();
    assert_eq!(search.found.len(), 1);

    // The same file opened again (a copy, say) is another show.
    engine.open(&f.show).unwrap();
    engine.publish_file_status(stale);
    assert!(
        engine
            .snapshot()
            .missing_files
            .iter()
            .all(|m| m.name != "house.jpg"),
        "the old check doesn't land"
    );
    let error = engine.use_found_files(search).unwrap_err();
    assert_eq!(
        error.to_string(),
        "Another show was opened while PixelFlow was looking. Look again."
    );
    assert_eq!(engine.snapshot().missing_files.len(), 1);
}

#[test]
fn a_show_file_moved_on_its_own_still_finds_its_files() {
    let f = folder();
    let mut engine = Engine::new(&f.data);
    let entry = build(&mut engine, &f);
    engine.save_as(&f.show).unwrap();
    // Only the show file moves, into a folder of its own.
    let elsewhere = f.root.parent().unwrap().join("Elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    let alone = elsewhere.join("show.pixelflow.json");
    fs::rename(&f.show, &alone).unwrap();
    let mut engine = Engine::new(&f.data);
    let snap = engine.open(&alone).unwrap();
    assert_eq!(snap.show.sequences[0], entry, "found where they were saved");
    assert!(snap.files_checked);
    assert!(snap.missing_files.is_empty(), "{:?}", snap.missing_files);

    // A file gone from there too says where it really was, and that folder is searched too.
    fs::create_dir_all(f.root.join("Audio")).unwrap();
    fs::rename(
        f.root.join("MP3 Music/Christmas Medley 2017.mp3"),
        f.root.join("Audio/Christmas Medley 2017.mp3"),
    )
    .unwrap();
    let mut engine = Engine::new(&f.data);
    let snap = engine.open(&alone).unwrap();
    assert_eq!(snap.missing_files.len(), 1);
    let missing = &snap.missing_files[0];
    assert_eq!(
        missing.was_at,
        text(&f.root.join("MP3 Music/Christmas Medley 2017.mp3"))
    );
    let search = engine.file_search().unwrap();
    assert_eq!(search.folders(), [elsewhere.clone(), f.root.clone()]);
    let report = engine.use_found_files(search.run()).unwrap();
    assert_eq!(
        report.found[0].to,
        text(&f.root.join("Audio/Christmas Medley 2017.mp3"))
    );
    assert!(report.still_missing.is_empty());
}

#[test]
fn a_show_saved_in_the_home_folder_and_moved_keeps_its_files() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    touch(&home.join("Carol.fseq"));
    touch(&home.join("Music/Carol.mp3"));
    let open_engine = || Engine::new(dir.path().join("data")).with_home(Some(home.clone()));
    let mut engine = open_engine();
    let mut entry = SequenceEntry::new("Carol", text(&home.join("Carol.fseq")));
    entry.audio = Some(text(&home.join("Music/Carol.mp3")));
    engine
        .apply(vec![Edit::AddSequence {
            sequence: entry.clone(),
        }])
        .unwrap();
    engine.save_as(&home.join("xmas.pixelflow.json")).unwrap();
    fs::create_dir_all(home.join("Shows")).unwrap();
    let moved = home.join("Shows/xmas.pixelflow.json");
    fs::rename(home.join("xmas.pixelflow.json"), &moved).unwrap();

    let mut engine = open_engine();
    let snap = engine.open(&moved).unwrap();
    assert_eq!(snap.show.sequences[0], entry);
    assert!(snap.missing_files.is_empty());

    // The home folder it was saved in is a search folder too, but only looked at itself.
    fs::create_dir_all(home.join("Other")).unwrap();
    fs::rename(home.join("Music/Carol.mp3"), home.join("Other/Carol.mp3")).unwrap();
    engine.check_files();
    let search = engine.file_search().unwrap();
    assert_eq!(search.folders(), [home.join("Shows"), home.clone()]);
    assert!(search.run().found.is_empty(), "not the whole home folder");
    touch(&home.join("Carol.mp3"));
    assert_eq!(
        engine.file_search().unwrap().run().found[0].to,
        text(&home.join("Carol.mp3"))
    );
}

#[test]
fn a_search_gives_up_after_its_time_limit() {
    let f = folder();
    let mut engine = Engine::new(&f.data);
    build(&mut engine, &f);
    engine.save_as(&f.show).unwrap();
    fs::remove_file(f.root.join("photos/house.jpg")).unwrap();
    engine.check_files();
    let outcome = engine
        .file_search()
        .unwrap()
        .with_time_limit(Duration::ZERO)
        .run();
    assert!(outcome.gave_up);
    let report = engine.use_found_files(outcome).unwrap();
    assert!(report.gave_up);
    assert!(report.found.is_empty());
}

#[test]
fn finding_a_file_that_came_back_points_nowhere_new() {
    let f = folder();
    let mut engine = Engine::new(&f.data);
    let entry = build(&mut engine, &f);
    engine.save_as(&f.show).unwrap();
    fs::rename(f.root.join("photos/house.jpg"), f.root.join("house.tmp")).unwrap();
    engine.check_files();
    assert_eq!(engine.snapshot().missing_files.len(), 1);
    fs::rename(f.root.join("house.tmp"), f.root.join("photos/house.jpg")).unwrap();
    let report = engine
        .use_found_files(engine.file_search().unwrap().run())
        .unwrap();
    assert_eq!(report.found[0].to, report.found[0].from, "back where it was");
    assert!(report.still_missing.is_empty());
    assert_eq!(report.snapshot.show.sequences[0], entry);
    assert!(!report.snapshot.dirty, "nothing changed");
}
