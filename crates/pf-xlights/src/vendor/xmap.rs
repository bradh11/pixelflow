//! xLights mapping files (`.xmap`), as xLights' Import Effects dialog saves and loads them, so a
//! mapping made in xLights can be used here and the other way round.
//!
//! The format, as `xLightsImportChannelMapDialog` writes it: a first line xLights ignores
//! ("false"), the number of the show's models that are mapped and their names (one a line),
//! then one line per mapping, its fields separated by tabs: the show's model, its submodel or
//! strand (blank for the model itself), a node (blank), the sequence item it takes effects from
//! ("Model", "Model/Submodel", "Model/Strand 1"), and a color. Several lines for one model
//! stack their effects on it; one item on several lines is copied to each.

use super::Mapping;
use crate::XlightsError;
use std::collections::BTreeMap;

const FILE: &str = "The mapping file";

/// The largest mapping file read.
pub const MAX_XMAP_BYTES: usize = 8 * 1024 * 1024;

/// A mapping read from an `.xmap` file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct XmapRead {
    pub mapping: Mapping,
    /// Lines mapping single nodes, which PixelFlow doesn't import.
    pub nodes_skipped: usize,
}

/// Reads an `.xmap` file's text.
pub fn read_xmap(text: &str) -> Result<XmapRead, XlightsError> {
    if text.len() > MAX_XMAP_BYTES {
        return Err(XlightsError::BadFile(FILE, "it's too large".into()));
    }
    let mut lines = text.lines().map(|l| l.trim_end_matches('\r'));
    let first = lines.next().unwrap_or("");
    if first.contains('{') {
        return Err(XlightsError::BadFile(
            FILE,
            "it's xLights' newer JSON mapping (.xjmap); save the mapping as .xmap in xLights".into(),
        ));
    }
    let count = lines
        .next()
        .and_then(|l| l.trim().parse::<usize>().ok())
        .ok_or_else(|| XlightsError::BadFile(FILE, "its second line should be a number of models".into()))?;
    // The mapped models' names (only a header: the lines below say everything).
    for _ in 0..count {
        if lines.next().is_none() {
            return Err(XlightsError::BadFile(
                FILE,
                "it ends before its list of models does".into(),
            ));
        }
    }
    let mut out = XmapRead::default();
    for line in lines {
        // xLights stops at the first blank line.
        if line.is_empty() {
            break;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 4 {
            continue;
        }
        let (model, strand, node, from) = (fields[0], fields[1], fields[2], fields[3]);
        if from.trim().is_empty() || model.trim().is_empty() {
            continue;
        }
        if !node.trim().is_empty() {
            out.nodes_skipped += 1;
            continue;
        }
        let target = if strand.trim().is_empty() {
            model.to_string()
        } else {
            format!("{model}/{strand}")
        };
        out.mapping.add(from, &target);
    }
    Ok(out)
}

/// Writes `mapping` as an `.xmap` file xLights can load.
pub fn write_xmap(mapping: &Mapping) -> String {
    // By the show's model, then its submodel ("" for the model itself), in name order: the
    // sequence items mapped there, in the mapping's order.
    let mut by_model: BTreeMap<&str, BTreeMap<&str, Vec<&str>>> = BTreeMap::new();
    for (item, targets) in &mapping.items {
        for target in targets {
            let (model, sub) = split_target(target);
            by_model
                .entry(model)
                .or_default()
                .entry(sub)
                .or_default()
                .push(item);
        }
    }
    let mut out = String::from("false\n");
    out.push_str(&format!("{}\n", by_model.len()));
    for model in by_model.keys() {
        out.push_str(&format!("{model}\n"));
    }
    for (model, subs) in &by_model {
        // xLights writes the model's own line first, even when only its submodels are mapped.
        if !subs.contains_key("") {
            out.push_str(&format!("{model}\t\t\t\twhite\n"));
        }
        for (sub, items) in subs {
            for item in items {
                out.push_str(&format!("{model}\t{sub}\t\t{item}\twhite\n"));
            }
        }
    }
    out
}

/// A target's model and submodel: "Prop/Sub" is ("Prop", "Sub"), "Prop" is ("Prop", "").
fn split_target(target: &str) -> (&str, &str) {
    target.split_once('/').unwrap_or((target, ""))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapping(pairs: &[(&str, &[&str])]) -> Mapping {
        let mut m = Mapping::default();
        for (item, targets) in pairs {
            m.items
                .insert(item.to_string(), targets.iter().map(|t| t.to_string()).collect());
        }
        m
    }

    #[test]
    fn reads_what_xlights_writes() {
        // As xLights saves it: models with a color, a stacked second source, a submodel, a node,
        // and a model mapped to nothing.
        let text = "false\n3\nMega Tree\nRoof\nArch\n\
            Mega Tree\t\t\tTree 1\trgb(255, 255, 0)\n\
            Mega Tree\t\t\tTree 2\twhite\n\
            Roof\t\t\tOutline\twhite\n\
            Roof\tLeft\t\tOutline/Left\twhite\n\
            Roof\tLeft\tNode 1\tOutline/Left\twhite\n\
            Arch\t\t\t\twhite\n\
            Window\t\t\tOutline\n\
            \n\
            Ignored\t\t\tAfter blank\twhite\n";
        let read = read_xmap(text).unwrap();
        assert_eq!(
            read.mapping,
            mapping(&[
                ("Tree 1", &["Mega Tree"]),
                ("Tree 2", &["Mega Tree"]),
                ("Outline", &["Roof", "Window"]),
                ("Outline/Left", &["Roof/Left"]),
            ])
        );
        assert_eq!(read.nodes_skipped, 1);
    }

    #[test]
    fn round_trips() {
        let original = mapping(&[
            ("Mega Tree", &["Tree"]),
            ("Outline", &["House Outline", "Roof Left"]),
            ("Arch 1", &["Door Arch"]),
            ("Arch 2", &["Door Arch"]),
            ("Matrix/Top", &["Pillar Left/Top"]),
            ("Floods", &[]),
        ]);
        let text = write_xmap(&original);
        assert!(text.starts_with("false\n5\nDoor Arch\nHouse Outline\nPillar Left\nRoof Left\nTree\n"));
        assert!(
            text.contains("Pillar Left\t\t\t\twhite\n"),
            "the model's own (empty) line"
        );
        assert!(text.contains("Door Arch\t\t\tArch 1\twhite\nDoor Arch\t\t\tArch 2\twhite\n"));
        let mut back = read_xmap(&text).unwrap().mapping;
        // Items mapped to nothing aren't written (xLights has no line for them).
        let mut expected = original.clone();
        expected.items.remove("Floods");
        for targets in back.items.values_mut().chain(expected.items.values_mut()) {
            targets.sort();
        }
        assert_eq!(back, expected);
    }

    #[test]
    fn other_files_are_refused_plainly() {
        assert!(
            read_xmap("{\"mappings\": []}")
                .unwrap_err()
                .to_string()
                .contains(".xjmap")
        );
        assert!(read_xmap("false\nnot a number\n").is_err());
        assert!(read_xmap("false\n3\nOnly one\n").is_err());
        assert_eq!(read_xmap("false\n0\n").unwrap(), XmapRead::default());
    }
}
