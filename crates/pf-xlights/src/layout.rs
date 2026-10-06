//! Models and groups from `xlights_rgbeffects.xml`.

use crate::error::XlightsError;
use crate::model::XmlModel;
use roxmltree::Node;
use std::collections::BTreeMap;

/// Legacy element names accepted in place of `<model DisplayAs=…>` in old files.
const LEGACY_ELEMENTS: [(&str, &str); 13] = [
    ("archesmodel", "Arches"),
    ("circlemodel", "Circle"),
    ("Cubemodel", "Cube"),
    ("custommodel", "Custom"),
    ("dmxgeneral", "DmxGeneral"),
    ("dmxservo", "DmxServo"),
    ("iciclemodel", "Icicles"),
    ("matrixmodel", "Matrix"),
    ("multipointmodel", "MultiPoint"),
    ("polylinemodel", "Poly Line"),
    ("spheremodel", "Sphere"),
    ("starmodel", "Star"),
    ("treemodel", "Tree"),
];

/// A model group and its members (model, submodel, or other group names, as written).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XGroup {
    pub name: String,
    pub members: Vec<String>,
}

/// Everything the importer reads from the layout file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct XLayout {
    pub models: Vec<XmlModel>,
    pub groups: Vec<XGroup>,
}

fn model_from(node: Node<'_, '_>) -> Option<XmlModel> {
    let tag = node.tag_name().name();
    let display_as = match node.attribute("DisplayAs") {
        Some(d) => d.trim().to_string(),
        None => LEGACY_ELEMENTS.iter().find(|(t, _)| *t == tag)?.1.to_string(),
    };
    if display_as == "ModelGroup" {
        return None;
    }
    let attrs_of = |n: Node<'_, '_>| -> BTreeMap<String, String> {
        n.attributes()
            .map(|a| (a.name().to_string(), a.value().to_string()))
            .collect()
    };
    let children = |name: &'static str| {
        node.children()
            .filter(move |c| c.is_element() && c.tag_name().name() == name)
    };
    Some(XmlModel {
        name: node.attribute("name").unwrap_or("").trim().to_string(),
        display_as,
        attrs: attrs_of(node),
        connection: children("ControllerConnection")
            .next()
            .map(attrs_of)
            .unwrap_or_default(),
        submodels: children("subModel").map(attrs_of).collect(),
        faces: children("faceInfo").map(attrs_of).collect(),
        states: children("stateInfo")
            .map(|s| {
                let name = s.attribute("Name").unwrap_or("").trim();
                if name.is_empty() {
                    s.attribute("Type").unwrap_or("SingleNode").to_string()
                } else {
                    name.to_string()
                }
            })
            .collect(),
    })
}

/// Reads the models and groups.
pub fn parse_layout(xml: &str) -> Result<XLayout, XlightsError> {
    let doc = crate::xml::parse(xml).map_err(|e| XlightsError::BadFile("xlights_rgbeffects.xml", e))?;
    let root = doc.root_element();
    if root.tag_name().name() != "xrgb" {
        return Err(XlightsError::BadFile(
            "xlights_rgbeffects.xml",
            format!("expected an <xrgb> document, found <{}>", root.tag_name().name()),
        ));
    }
    let section = |name: &str| {
        root.children()
            .find(|c| c.is_element() && c.tag_name().name() == name)
    };
    let models = section("models")
        .into_iter()
        .flat_map(|s| s.children().filter(Node::is_element))
        .filter_map(model_from)
        .filter(|m| !m.name.is_empty())
        .collect();
    let groups = section("modelGroups")
        .into_iter()
        .flat_map(|s| {
            s.children()
                .filter(|c| c.is_element() && c.tag_name().name() == "modelGroup")
        })
        .map(|g| XGroup {
            name: g.attribute("name").unwrap_or("").trim().to_string(),
            members: g
                .attribute("models")
                .unwrap_or("")
                .split(',')
                .map(str::trim)
                .filter(|m| !m.is_empty())
                .map(String::from)
                .collect(),
        })
        .filter(|g| !g.name.is_empty())
        .collect();
    Ok(XLayout { models, groups })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_models_connections_and_groups() {
        let xml = r#"<?xml version="1.0"?>
<xrgb>
  <models type="rgb_effects">
    <model name="Roof" DisplayAs="Single Line" StringType="RGB Nodes" parm1="1" parm2="100" StartChannel="!Falcon:1"
           WorldPosX="100" WorldPosY="400" X2="800" Y2="0">
      <ControllerConnection Port="1" Protocol="ws2811"/>
      <subModel name="Left" layout="horizontal" type="ranges" line0="1-50"/>
      <faceInfo Name="Face" Type="NodeRange" Mouth-O="1-3"/>
      <stateInfo Name="Lights" Type="NodeRange" s1="1-10"/>
      <stateInfo Type="SingleNode"/>
    </model>
    <treemodel name="Old Tree" parm1="16"/>
    <model name="Everything" DisplayAs="ModelGroup"/>
    <model DisplayAs="Arches" name=""/>
  </models>
  <modelGroups type="rgb_effects">
    <modelGroup name="Outline" models="Roof, Old Tree,Tree/Star ,"/>
  </modelGroups>
</xrgb>"#;
        let layout = parse_layout(xml).unwrap();
        let names: Vec<_> = layout
            .models
            .iter()
            .map(|m| (m.name.as_str(), m.display_as.as_str()))
            .collect();
        assert_eq!(names, vec![("Roof", "Single Line"), ("Old Tree", "Tree")]);
        assert_eq!(
            layout.models[0].connection.get("Port").map(String::as_str),
            Some("1")
        );
        assert_eq!(layout.models[0].attr("StartChannel"), Some("!Falcon:1"));
        let roof = &layout.models[0];
        assert_eq!(roof.submodels.len(), 1);
        assert_eq!(roof.submodels[0]["line0"], "1-50");
        assert_eq!(roof.faces[0]["Mouth-O"], "1-3");
        assert_eq!(roof.states, vec!["Lights", "SingleNode"]);
        assert!(layout.models[1].submodels.is_empty());
        assert_eq!(
            layout.groups,
            vec![XGroup {
                name: "Outline".into(),
                members: vec!["Roof".into(), "Old Tree".into(), "Tree/Star".into()],
            }]
        );
    }

    #[test]
    fn other_documents_are_rejected() {
        assert!(
            parse_layout("<Networks/>")
                .unwrap_err()
                .to_string()
                .contains("<xrgb>")
        );
        assert!(parse_layout("<xrgb").is_err());
    }

    #[test]
    fn deeply_nested_and_dtd_files_are_refused_without_crashing() {
        let deep = format!(
            "<xrgb><models>{}{}</models></xrgb>",
            "<model>".repeat(200_000),
            "</model>".repeat(200_000)
        );
        assert_eq!(
            parse_layout(&deep).unwrap_err().to_string(),
            "xlights_rgbeffects.xml isn't a valid xLights file: its elements are nested more than 64 deep"
        );
        let dtd = r#"<!DOCTYPE xrgb [<!ENTITY a "aaaa">]><xrgb>&a;</xrgb>"#;
        assert!(parse_layout(dtd).is_err());
    }
}
