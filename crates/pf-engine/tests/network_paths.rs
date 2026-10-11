//! Network and device paths a show or a sequence names are never looked at, unless the user
//! chose something in the same folder this session (see `pf_model::network_path_refusal`).
//!
//! To show a file isn't looked at, each test names a file that is really there by a spelling
//! the rule takes for a network or device path and that still reaches the file on this system:
//! a second leading `/` on Unix, the `\\?\` prefix on Windows. Looked at, the file would be
//! found; refused, it's reported with the refusal instead.

use pf_engine::{Edit, Engine, FileRole};
use pf_model::{Background, HouseModel, SequenceEntry, path_to_text};
use std::fs;
use std::path::{Path, PathBuf};

fn touch(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, b"x").unwrap();
}

/// `path` (an ordinary full path) spelled so its text is a network or device path.
fn spelled_as_network(path: &Path) -> String {
    let text = path_to_text(path);
    if cfg!(windows) {
        format!("\\\\?\\{text}")
    } else {
        format!("/{text}")
    }
}

/// What trusting the folder `file` is in does: the file dialogs do it for whatever is chosen.
fn choose_in_a_dialog(file: &str) {
    pf_model::trust_network_folder_of(&pf_model::path_from_text(file));
}

/// A saved show (in `Show`) whose sequence, music, photo, and house model are all there, in
/// folders of their own under `media`, and named by network spellings.
struct Saved {
    _dir: tempfile::TempDir,
    data: PathBuf,
    show: PathBuf,
    entry: SequenceEntry,
}

fn saved_show(media: &str) -> Saved {
    let dir = tempfile::tempdir().unwrap();
    let media = dir.path().join(media);
    let named = |folder: &str, name: &str| {
        let file = media.join(folder).join(name);
        touch(&file);
        spelled_as_network(&file)
    };
    let mut entry = SequenceEntry::new("Medley", named("Sequences", "Medley.fseq"));
    entry.audio = Some(named("Music", "Medley.mp3"));
    let data = dir.path().join("data");
    let show = dir.path().join("Show").join("show.pixelflow.json");
    let mut engine = Engine::new(&data);
    engine
        .apply(vec![
            Edit::AddSequence {
                sequence: entry.clone(),
            },
            Edit::SetBackground {
                background: Some(Background::new(named("Photos", "house.jpg"), 0.0, 0.0, 10.0)),
            },
            Edit::SetHouseModel {
                house_model: Some(HouseModel::new(named("Models", "house.glb"))),
            },
        ])
        .unwrap();
    engine.save_as(&show).unwrap();
    Saved {
        _dir: dir,
        data,
        show,
        entry,
    }
}

#[test]
fn a_show_naming_network_paths_opens_without_looking_at_them() {
    let saved = saved_show("Unchosen");
    let mut engine = Engine::new(&saved.data);
    let snap = engine.open(&saved.show).unwrap();
    assert_eq!(snap.show.sequences[0], saved.entry, "the paths stay as written");
    assert!(snap.files_checked);
    let missing = snap.missing_files;
    assert_eq!(missing.len(), 4, "{missing:?}");
    assert_eq!(missing[0].file, FileRole::Sequence { id: saved.entry.id });
    assert_eq!(missing[1].file, FileRole::Music { id: saved.entry.id });
    assert_eq!(missing[2].file, FileRole::Photo);
    assert_eq!(missing[3].file, FileRole::HouseModel);
    for (file, name) in missing
        .iter()
        .zip(["Medley.fseq", "Medley.mp3", "house.jpg", "house.glb"])
    {
        assert_eq!(file.name, name);
        assert!(
            file.message.starts_with(&format!("{name} is "))
                && file.message.contains("only after you choose"),
            "{}",
            file.message
        );
    }
    // Checking again doesn't look either.
    engine.check_files();
    assert_eq!(engine.snapshot().missing_files.len(), 4);
    // Nor does a search: the files are where the show says, and stay unfound.
    let found = engine.file_search().unwrap().run();
    assert!(found.found.is_empty(), "{:?}", found.found);

    // Playing the sequence is refused with the same sentence, before the file is opened.
    let error = engine.play_sequence(saved.entry.id, 0).unwrap_err().to_string();
    assert_eq!(error, missing[0].message);
}

#[test]
fn files_in_a_network_folder_the_user_chose_from_are_looked_at() {
    let saved = saved_show("Chosen");
    let mut engine = Engine::new(&saved.data);
    engine.open(&saved.show).unwrap();
    assert_eq!(engine.snapshot().missing_files.len(), 4);

    // The user locates the music in a dialog: its folder is trusted, the other three aren't.
    choose_in_a_dialog(saved.entry.audio.as_deref().unwrap());
    engine.check_files();
    let missing = engine.snapshot().missing_files;
    assert_eq!(missing.len(), 3, "{missing:?}");
    assert!(
        missing
            .iter()
            .all(|m| m.file != FileRole::Music { id: saved.entry.id })
    );

    choose_in_a_dialog(&saved.entry.path);
    engine.check_files();
    assert_eq!(engine.snapshot().missing_files.len(), 2);
    // Opened afresh in the same session, those two are found without asking again.
    let mut engine = Engine::new(&saved.data);
    let snap = engine.open(&saved.show).unwrap();
    assert_eq!(snap.missing_files.len(), 2, "{:?}", snap.missing_files);
}

#[test]
fn a_network_folder_a_show_says_it_was_saved_in_is_not_looked_in() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("Saved In");
    touch(&home.join("Music").join("Carol.mp3"));
    touch(&home.join("Carol.fseq"));
    let show = home.join("show.pixelflow.json");
    let mut entry = SequenceEntry::new("Carol", path_to_text(&home.join("Carol.fseq")));
    entry.audio = Some(path_to_text(&home.join("Music").join("Carol.mp3")));
    let mut engine = Engine::new(dir.path().join("data"));
    engine
        .apply(vec![Edit::AddSequence {
            sequence: entry.clone(),
        }])
        .unwrap();
    engine.save_as(&show).unwrap();

    // The show file moves on its own, and says it was saved in a network folder.
    let alone = dir.path().join("Alone").join("show.pixelflow.json");
    fs::create_dir_all(alone.parent().unwrap()).unwrap();
    let mut json: serde_json::Value = serde_json::from_str(&fs::read_to_string(&show).unwrap()).unwrap();
    assert_eq!(json["savedIn"], path_to_text(&home), "files are found from here");
    let on_network = spelled_as_network(&home);
    json["savedIn"] = on_network.clone().into();
    fs::write(&alone, serde_json::to_string(&json).unwrap()).unwrap();

    let mut engine = Engine::new(dir.path().join("data"));
    let snap = engine.open(&alone).unwrap();
    assert_eq!(snap.missing_files.len(), 2, "{:?}", snap.missing_files);
    for missing in &snap.missing_files {
        assert_eq!(missing.was_at, missing.path, "nowhere else was looked in");
    }
    assert_eq!(
        engine.file_search().unwrap().folders(),
        [alone.parent().unwrap().to_path_buf()],
        "and it isn't searched"
    );

    // Once the user has chosen something there, the same file finds its files.
    pf_model::trust_network_folder(&pf_model::path_from_text(&on_network));
    let mut engine = Engine::new(dir.path().join("data"));
    let snap = engine.open(&alone).unwrap();
    assert!(snap.missing_files.is_empty(), "{:?}", snap.missing_files);
}

#[test]
fn a_sequences_music_on_a_network_share_is_named_but_never_opened() {
    let dir = tempfile::tempdir().unwrap();
    let song = dir.path().join("Songs").join("Carol.mp3");
    touch(&song);
    let named = spelled_as_network(&song);
    let mut engine = Engine::new(dir.path().join("data"));
    engine.new_sequence_doc("Carol", 5_000, Some(&named)).unwrap();

    assert_eq!(engine.sequence_music(), None, "nothing to open");
    assert_eq!(
        engine.sequence_music_named(),
        Some(pf_model::path_from_text(&named))
    );
    let missing = engine.sequence_music_missing().expect("refused");
    assert_eq!(missing.file, FileRole::SequenceDocMusic);
    assert_eq!(missing.name, "Carol.mp3");
    assert_eq!(missing.owner, "Music for Carol");
    assert_eq!(
        Some(missing.message),
        pf_model::network_path_refusal(&named),
        "says why, not that it's gone"
    );

    choose_in_a_dialog(&named);
    assert_eq!(engine.sequence_music(), Some(pf_model::path_from_text(&named)));
    assert_eq!(engine.sequence_music_missing(), None);
}
