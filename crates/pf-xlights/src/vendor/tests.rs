//! Mapping a made-up vendor's sequence onto a made-up house.

use super::package::tests::{LAYOUT, SEQUENCE, zip_of};
use super::*;
use crate::sequence::parse_xsq;
use pf_sequence::Target;

/// A house like the ones PixelFlow is used for, imported from xLights as users do.
const HOUSE: &str = r#"<?xml version="1.0"?>
<xrgb><models>
  <model name="Outline Left" DisplayAs="Single Line" parm1="1" parm2="100" StringType="RGB Nodes" StartChannel="1" X2="100" Y2="0"/>
  <model name="Outline Right" DisplayAs="Single Line" parm1="1" parm2="120" StringType="RGB Nodes" StartChannel="1" X2="100" Y2="0"/>
  <model name="Roof Top" DisplayAs="Poly Line" parm1="1" parm2="150" StringType="RGB Nodes" StartChannel="1" PointData="0,0,0,1,1,0"/>
  <model name="Window Dining Room" DisplayAs="Window Frame" parm1="10" parm2="20" parm3="10" StringType="RGB Nodes" StartChannel="1"/>
  <model name="Window Master" DisplayAs="Window Frame" parm1="10" parm2="20" parm3="10" StringType="RGB Nodes" StartChannel="1"/>
  <model name="Candy Canes 1-12" DisplayAs="Candy Canes" parm1="12" parm2="18" parm3="1" StringType="RGB Nodes" StartChannel="1"/>
  <model name="Tree" DisplayAs="Tree 360" parm1="16" parm2="50" parm3="1" StringType="RGB Nodes" StartChannel="1"/>
  <model name="Pillar Left" DisplayAs="Vert Matrix" parm1="4" parm2="30" parm3="1" StringType="RGB Nodes" StartChannel="1"/>
  <model name="Pillar Right" DisplayAs="Vert Matrix" parm1="4" parm2="30" parm3="1" StringType="RGB Nodes" StartChannel="1"/>
  <model name="Door Arch" DisplayAs="Arches" parm1="1" parm2="50" StringType="RGB Nodes" StartChannel="1">
    <subModel name="Left Half" layout="horizontal" type="ranges" line0="1-25"/>
  </model>
</models><modelGroups>
  <modelGroup name="House Outline" models="Outline Left,Outline Right,Roof Top"/>
  <modelGroup name="Windows" models="Window Dining Room,Window Master"/>
  <modelGroup name="Pillars" models="Pillar Left,Pillar Right"/>
  <modelGroup name="Whole House" models="Outline Left,Outline Right,Roof Top,Window Dining Room,Window Master,Candy Canes 1-12,Tree,Pillar Left,Pillar Right,Door Arch"/>
</modelGroups></xrgb>"#;

/// The vendor's layout: their names, their props.
const VENDOR: &str = r#"<?xml version="1.0"?>
<xrgb><models>
  <model name="Mega Tree" DisplayAs="Tree 180" parm1="16" parm2="50" StringType="RGB Nodes" StartChannel="1"/>
  <model name="Mini Tree 1" DisplayAs="Tree 360" parm1="1" parm2="50" StringType="RGB Nodes" StartChannel="1"/>
  <model name="Mini Tree 2" DisplayAs="Tree 360" parm1="1" parm2="50" StringType="RGB Nodes" StartChannel="1"/>
  <model name="Eave 1" DisplayAs="Single Line" parm1="1" parm2="100" StringType="RGB Nodes" StartChannel="1"/>
  <model name="Eave 2" DisplayAs="Single Line" parm1="1" parm2="120" StringType="RGB Nodes" StartChannel="1"/>
  <model name="Big Arch" DisplayAs="Arches" parm1="1" parm2="50" StringType="RGB Nodes" StartChannel="1">
    <subModel name="Left Half" layout="horizontal" type="ranges" line0="1-25"/>
  </model>
  <model name="P10 Panel" DisplayAs="Horiz Matrix" parm1="32" parm2="64" StringType="RGB Nodes" StartChannel="1"/>
  <model name="Canes" DisplayAs="Candy Canes" parm1="8" parm2="18" StringType="RGB Nodes" StartChannel="1"/>
  <model name="Front Window" DisplayAs="Window Frame" parm1="10" parm2="20" parm3="10" StringType="RGB Nodes" StartChannel="1"/>
  <model name="Flood 1" DisplayAs="Single Line" parm1="1" parm2="1" StringType="RGB Nodes" StartChannel="1"/>
  <model name="Spinner" DisplayAs="Spinner" parm1="6" parm2="20" StringType="RGB Nodes" StartChannel="1"/>
  <model name="Old Name Prop" DisplayAs="Single Line" parm1="1" parm2="10" StringType="RGB Nodes" StartChannel="1">
    <Aliases><alias name="oldname:Pillar Right"/></Aliases>
  </model>
</models><modelGroups>
  <modelGroup name="Group - Eaves" models="Eave 1,Eave 2"/>
  <modelGroup name="Everything" models="Mega Tree,Mini Tree 1,Mini Tree 2,Eave 1,Eave 2,Big Arch,P10 Panel,Canes,Front Window,Flood 1,Spinner"/>
</modelGroups></xrgb>"#;

fn effects(n: usize, from: usize) -> String {
    (0..n)
        .map(|i| {
            let start = (from + i) * 100;
            format!(
                r#"<Effect ref="0" name="On" startTime="{start}" endTime="{}" palette="0"/>"#,
                start + 100
            )
        })
        .collect()
}

/// The vendor's sequence: effects on most of their props.
fn vendor_sequence() -> String {
    let element = |name: &str, n: usize| {
        format!(
            r#"<Element type="model" name="{name}"><EffectLayer>{}</EffectLayer></Element>"#,
            effects(n, 0)
        )
    };
    format!(
        r#"<?xml version="1.0"?><xsequence FixedPointTiming="1">
<head><version>2024.01</version><song>Vendor Song</song><mediaFile>Vendor Song.mp3</mediaFile><sequenceType>Media</sequenceType>
<sequenceTiming>50 ms</sequenceTiming><sequenceDuration>60.000</sequenceDuration></head>
<ColorPalettes><ColorPalette>C_BUTTON_Palette1=#FF0000,C_CHECKBOX_Palette1=1</ColorPalette></ColorPalettes>
<EffectDB><Effect>E_SLIDER_Speed=10</Effect></EffectDB>
<ElementEffects>
<Element type="timing" name="Beats"><EffectLayer><Effect label="1" startTime="0" endTime="500"/></EffectLayer></Element>
{}{}{}{}{}{}{}{}{}{}{}{}{}
<Element type="model" name="Big Arch"><EffectLayer>{}</EffectLayer><SubModelEffectLayer layer="0" name="Left Half">{}</SubModelEffectLayer></Element>
<Element type="model" name="Unused"><EffectLayer/></Element>
</ElementEffects></xsequence>"#,
        element("Mega Tree", 40),
        element("Mini Tree 1", 10),
        element("Mini Tree 2", 9),
        element("Eave 1", 5),
        element("Eave 2", 5),
        element("Group - Eaves", 30),
        element("Everything", 50),
        element("P10 Panel", 20),
        element("Canes", 12),
        element("Front Window", 6),
        element("Flood 1", 4),
        element("Spinner", 7),
        element("Old Name Prop", 3),
        effects(8, 0),
        effects(2, 10),
    )
}

fn house() -> Show {
    let layout = crate::parse_layout(HOUSE).unwrap();
    crate::build_show("House", &[], &layout, crate::geometry).show
}

/// Inspects the vendor sequence against the house, as the app does.
fn inspected(saved: Option<Mapping>) -> Inspection {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Vendor Song.zip");
    std::fs::write(
        &path,
        zip_of(&[
            ("Vendor/xlights_rgbeffects.xml", VENDOR.as_bytes()),
            ("Vendor/Vendor Song.xsq", vendor_sequence().as_bytes()),
            ("Vendor/Music/Vendor Song.mp3", b"music"),
        ]),
    )
    .unwrap();
    let package = Package::open(&path).unwrap();
    inspect(&package, None, &house(), |_| saved).unwrap()
}

fn mapped<'a>(inspection: &'a Inspection, item: &str) -> Option<&'a [String]> {
    inspection.mapping.targets(item)
}

fn suggestion<'a>(inspection: &'a Inspection, item: &str) -> &'a Suggestion {
    inspection.suggestions.iter().find(|s| s.item == item).unwrap()
}

#[test]
fn items_are_the_models_submodels_and_strands_with_effects() {
    let file = parse_xsq(&vendor_sequence()).unwrap();
    let items = items_of(&file);
    let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
    assert!(!names.contains(&"Unused") && !names.contains(&"Beats"));
    let arch = names.iter().position(|n| *n == "Big Arch").unwrap();
    assert_eq!(
        names[arch + 1],
        "Big Arch/Left Half",
        "submodels follow their model"
    );
    assert_eq!(items[arch + 1].kind, ItemKind::Submodel);
    assert_eq!(items[arch + 1].label, "Left Half");
    assert_eq!(items[arch + 1].effects, 2);
}

#[test]
fn auto_map_matches_like_with_like() {
    let x = inspected(None);
    assert!(x.has_layout);
    assert_eq!(x.song, "Vendor Song");
    assert_eq!(x.music.as_deref(), Some("Vendor Song.mp3"));
    assert!(!x.all_exact);
    let reason = |item: &str| suggestion(&x, item).reason;

    // The mega tree goes to the tree, by type (the only tree in the house).
    assert_eq!(mapped(&x, "Mega Tree"), Some(&["Tree".to_string()][..]));
    assert_eq!(reason("Mega Tree"), MatchReason::Type);
    // The other trees don't pile onto it: they're unmapped, with the tree as a hint.
    assert_eq!(mapped(&x, "Mini Tree 1"), None);
    let hint = suggestion(&x, "Mini Tree 1");
    assert!(hint.confidence < AUTO_MAP_CONFIDENCE && hint.targets == ["Tree"]);
    // A group of lines goes to the group of lines; the whole house to the whole house.
    assert_eq!(
        mapped(&x, "Group - Eaves"),
        Some(&["House Outline".to_string()][..])
    );
    assert_eq!(mapped(&x, "Everything"), Some(&["Whole House".to_string()][..]));
    // Lines to lines (the closest in size), arches to the arch, canes to canes, windows to windows.
    assert_eq!(mapped(&x, "Eave 2"), Some(&["Outline Right".to_string()][..]));
    assert_eq!(mapped(&x, "Eave 1"), Some(&["Outline Left".to_string()][..]));
    assert_eq!(mapped(&x, "Big Arch"), Some(&["Door Arch".to_string()][..]));
    assert_eq!(mapped(&x, "Canes"), Some(&["Candy Canes 1-12".to_string()][..]));
    assert_eq!(reason("Canes"), MatchReason::Type);
    assert!(mapped(&x, "Front Window").is_some_and(|t| t[0].starts_with("Window ")));
    // The submodel goes to the arch's submodel of the same name.
    assert_eq!(
        mapped(&x, "Big Arch/Left Half"),
        Some(&["Door Arch/Left Half".to_string()][..])
    );
    // An alias (xLights' "oldname:") names a prop.
    assert_eq!(
        mapped(&x, "Old Name Prop"),
        Some(&["Pillar Right".to_string()][..])
    );
    assert_eq!(reason("Old Name Prop"), MatchReason::Alias);
    // Nothing like a flood or a spinner in the house; a P10 panel is far bigger than a pillar.
    assert_eq!(mapped(&x, "Flood 1"), None);
    assert_eq!(mapped(&x, "Spinner"), None);
    assert_eq!(mapped(&x, "P10 Panel"), None);
    assert_eq!(suggestion(&x, "P10 Panel").targets, ["Pillar Left"]);

    // Types and sizes come from the vendor's layout; targets list the house's.
    let tree = x.items.iter().find(|i| i.name == "Mega Tree").unwrap();
    assert_eq!(
        (tree.ptype, tree.pixels, tree.display_as.as_deref()),
        (PropType::Tree, 800, Some("Tree 180"))
    );
    let everything = x.items.iter().find(|i| i.name == "Everything").unwrap();
    assert_eq!(everything.kind, ItemKind::Group);
    let canes = x.targets.iter().find(|t| t.name == "Candy Canes 1-12").unwrap();
    assert_eq!((canes.kind, canes.ptype), (ItemKind::Model, PropType::Canes));
    assert!(
        x.targets
            .iter()
            .any(|t| t.name == "Door Arch/Left Half" && t.kind == ItemKind::Submodel)
    );
}

#[test]
fn a_saved_mapping_comes_first_and_keeps_skips() {
    let first = inspected(None);
    let mut saved = Mapping::default();
    saved.items.insert(
        "Mega Tree".into(),
        vec!["Pillar Left".into(), "Pillar Right".into()],
    );
    saved.items.insert("Canes".into(), vec![]);
    saved.items.insert("Spinner".into(), vec!["Gone Prop".into()]);
    let x = inspected(Some(saved));
    assert_eq!(x.key, first.key, "the vendor's layout names the mapping");
    assert!(x.key.starts_with("layout:"));
    assert_eq!(
        mapped(&x, "Mega Tree"),
        Some(&["Pillar Left".to_string(), "Pillar Right".to_string()][..])
    );
    assert_eq!(suggestion(&x, "Mega Tree").reason, MatchReason::Saved);
    assert_eq!(
        mapped(&x, "Canes"),
        Some(&[][..]),
        "skipped last time, skipped again"
    );
    // A saved target no longer in the show falls back to matching.
    assert_ne!(suggestion(&x, "Spinner").reason, MatchReason::Saved);
}

#[test]
fn the_users_own_sequence_maps_by_name_with_nothing_to_ask() {
    let show = house();
    let xsq = format!(
        r#"<xsequence FixedPointTiming="1"><head><sequenceTiming>50 ms</sequenceTiming><sequenceDuration>5</sequenceDuration></head>
<ColorPalettes/><EffectDB><Effect>E_SLIDER_Speed=10</Effect></EffectDB><ElementEffects>
<Element type="model" name="Tree"><EffectLayer>{}</EffectLayer></Element>
<Element type="model" name="Whole House"><EffectLayer>{}</EffectLayer></Element>
</ElementEffects></xsequence>"#,
        effects(3, 0),
        effects(2, 0)
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Mine.xsq");
    std::fs::write(&path, xsq).unwrap();
    let package = Package::open(&path).unwrap();
    let x = inspect(&package, None, &show, |_| None).unwrap();
    assert!(x.all_exact);
    assert!(!x.has_layout);
    assert!(x.key.starts_with("sequence:"));
    assert_eq!(x.mapping.items.len(), 2);
}

/// A tiny sequence: effects on "A" (two layers) and "B" (one), each its own effect name.
fn two_models() -> XsqFile {
    parse_xsq(
        r#"<xsequence FixedPointTiming="1"><head><sequenceTiming>50 ms</sequenceTiming><sequenceDuration>5</sequenceDuration></head>
<ColorPalettes/><EffectDB><Effect>E_SLIDER_Speed=10</Effect></EffectDB><ElementEffects>
<Element type="model" name="A"><EffectLayer><Effect ref="0" name="On" startTime="0" endTime="100"/></EffectLayer>
  <EffectLayer><Effect ref="0" name="Twinkle" startTime="0" endTime="100"/></EffectLayer>
  <Strand index="1"><Effect ref="0" name="Bars" startTime="0" endTime="100"/><Node index="0"><Effect ref="0" name="On" startTime="0" endTime="100"/></Node></Strand></Element>
<Element type="model" name="B"><EffectLayer><Effect ref="0" name="Shimmer" startTime="0" endTime="100"/></EffectLayer></Element>
</ElementEffects></xsequence>"#,
    )
    .unwrap()
}

fn row_effects(import: &SequenceImport, show: &Show, prop: &str) -> Vec<Vec<String>> {
    let id = show.props.iter().find(|p| p.name == prop).unwrap().id;
    let row = import
        .sequence
        .rows
        .iter()
        .find(|r| r.target == Target::Prop(id))
        .unwrap();
    row.layers
        .iter()
        .map(|l| {
            l.effects
                .iter()
                .map(|e| {
                    format!("{:?}", e.params)
                        .split(['(', ' ', '{'])
                        .next()
                        .unwrap()
                        .to_string()
                })
                .collect()
        })
        .collect()
}

#[test]
fn one_item_copies_to_several_targets_and_several_layer_onto_one() {
    let show = house();
    let file = two_models();
    let mut mapping = Mapping::default();
    mapping.add("A", "Tree");
    mapping.add("A", "Door Arch");
    mapping.add("B", "Tree");
    mapping.add("A/Strand 2", "Outline Left");
    let import = build_sequence_mapped(&file, &show, "Test", &mapping);
    // A goes to both; on the tree, B's layer goes beneath A's (A is first in the sequence, so
    // it stays on top, as when xLights maps two models to one). PixelFlow draws its last layer
    // on top: A's top layer (On) is last.
    assert_eq!(
        row_effects(&import, &show, "Tree"),
        vec![vec!["Shimmer"], vec!["Twinkle"], vec!["On"]]
    );
    assert_eq!(
        row_effects(&import, &show, "Door Arch"),
        vec![vec!["Twinkle"], vec!["On"]]
    );
    // The strand's own effects go where it's mapped; its node's don't come in.
    assert_eq!(row_effects(&import, &show, "Outline Left"), vec![vec!["Bars"]]);
    assert_eq!(import.summary.effects, 6);
    assert_eq!(import.summary.skipped, 1);
    assert!(
        import
            .notes
            .iter()
            .any(|n| n.contains("single nodes") && n.contains("A (1 effect)")),
        "{:?}",
        import.notes
    );
    // Every copy is its own effect.
    let ids: HashSet<_> = import
        .sequence
        .rows
        .iter()
        .flat_map(|r| &r.layers)
        .flat_map(|l| &l.effects)
        .map(|e| e.id)
        .collect();
    assert_eq!(ids.len(), 6);

    // The same mapping twice gives the same sequence, layer for layer.
    let again = build_sequence_mapped(&file, &show, "Test", &mapping);
    assert_eq!(
        row_effects(&again, &show, "Tree"),
        row_effects(&import, &show, "Tree")
    );
}

#[test]
fn unmapped_items_and_missing_props_are_reported() {
    let show = house();
    let mut mapping = Mapping::default();
    mapping.items.insert("A".into(), vec![]);
    mapping.add("B", "Not In The House");
    let import = build_sequence_mapped(&two_models(), &show, "Test", &mapping);
    assert!(import.sequence.rows.is_empty());
    assert_eq!(import.summary.effects, 0);
    let notes = import.notes.join("\n");
    assert!(
        notes.contains(
            "weren't mapped to anything, so their effects weren't imported: A (2 effects), B (1 effect)"
        ),
        "{notes}"
    );
    assert!(
        notes.contains(
            "The mapping names props that aren't in the show, so nothing went to them: Not In The House."
        ),
        "{notes}"
    );
    assert!(notes.contains("A/Strand 2 (1 effect)"), "{notes}");
}

#[test]
fn a_zip_package_imports_with_its_music_copied_next_to_the_show() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Made Up Song.xsqz");
    std::fs::write(
        &path,
        zip_of(&[
            ("xlights_rgbeffects.xml", LAYOUT.as_bytes()),
            ("Made Up Song.xsq", SEQUENCE.as_bytes()),
            ("Made Up Song.mp3", b"music"),
        ]),
    )
    .unwrap();
    let show = house();
    let package = Package::open(&path).unwrap();
    let x = inspect(&package, None, &show, |_| None).unwrap();
    assert_eq!(
        x.mapping.targets("MegaTree 16x50"),
        Some(&["Tree".to_string()][..])
    );
    assert_eq!(x.music.as_deref(), Some("Made Up Song.mp3"));
    let music = dir.path().join("show/music");
    let no_audio =
        |_: &Path, _: Option<&str>| -> Option<PathBuf> { panic!("a zip's music isn't looked for on disk") };
    let import = import(&package, None, &show, &x.mapping, Some(&music), no_audio).unwrap();
    assert_eq!(import.sequence.name, "Made Up Song");
    assert_eq!(
        import.summary.effects,
        2 + 1,
        "the tree's two and the roofline's one"
    );
    assert_eq!(
        import.sequence.audio.as_deref(),
        Some(pf_model::path_to_text(&music.join("Made Up Song.mp3")).as_str())
    );
    // Without a folder for it, the music is left out, with a note.
    let unsaved = super::import(&package, None, &show, &x.mapping, None, no_audio).unwrap();
    assert!(unsaved.sequence.audio.is_none());
    assert!(
        unsaved.notes[0].contains("Made Up Song.mp3"),
        "{:?}",
        unsaved.notes
    );
}
