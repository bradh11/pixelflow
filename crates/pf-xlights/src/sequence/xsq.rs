//! Reading the parts of an `.xsq` file the import needs, including `<CompressedData>` blocks.

use crate::XlightsError;
use crate::xml::{self, MAX_XML_BYTES};
use roxmltree::Node;
use std::io::Read;

/// Largest `.xsq` file read, in bytes (the same limit as every xLights file).
pub const MAX_XSQ_BYTES: usize = MAX_XML_BYTES;
/// Effects and marks read across the whole file before the rest is skipped.
pub const MAX_ITEMS: usize = 2_000_000;

const FILE: &str = "The xLights sequence";

/// The `<head>` fields the import uses, as written (entity-decoded once by the XML parser).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct XsqHead {
    pub song: String,
    pub artist: String,
    pub media_file: String,
    pub sequence_type: String,
    /// `sequenceDuration`, as written (seconds).
    pub duration: String,
    /// `sequenceTiming`, as written (e.g. "25 ms").
    pub timing: String,
}

/// One timed item: a model effect (with its settings and palette references) or a timing mark.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct XsqEffect {
    /// The effect's name for model effects (`Bars`), the mark's label for timing tracks.
    pub name: String,
    /// Index into the effect settings (`EffectDB`), when given.
    pub settings_ref: Option<String>,
    /// Settings written inline (old files without `ref`).
    pub inline_settings: String,
    /// Index into the color palettes, when given.
    pub palette_ref: Option<String>,
    /// Start and end as written (milliseconds, or seconds in old files).
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct XsqLayer {
    pub effects: Vec<XsqEffect>,
}

/// What a sequence element is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementKind {
    Model,
    Timing,
    Other,
}

/// One layer of effects on a model's submodel (`<SubModelEffectLayer>`).
#[derive(Debug, Clone, PartialEq)]
pub struct XsqSubmodelLayer {
    /// The submodel's name (trimmed).
    pub name: String,
    /// The layer number (0 is xLights' top layer).
    pub layer: usize,
    pub effects: Vec<XsqEffect>,
}

/// One `<Element>` under `<ElementEffects>`.
#[derive(Debug, Clone, PartialEq)]
pub struct XsqElement {
    pub kind: ElementKind,
    /// As written (trimmed).
    pub name: String,
    /// `fixed="N"`: a timing track with a mark every N ms.
    pub fixed: Option<String>,
    pub layers: Vec<XsqLayer>,
    /// Layers of effects on the model's submodels, in file order.
    pub submodels: Vec<XsqSubmodelLayer>,
    /// Effects on the model's strands and single nodes.
    pub sub_effects: usize,
}

impl XsqElement {
    /// Effects on the model's submodels.
    pub fn submodel_effects(&self) -> usize {
        self.submodels.iter().map(|s| s.effects.len()).sum()
    }
}

/// The parts of an `.xsq` file the import uses.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct XsqFile {
    pub head: XsqHead,
    /// Times are whole milliseconds (`FixedPointTiming`), not seconds.
    pub fixed_point: bool,
    pub palettes: Vec<String>,
    pub effect_db: Vec<String>,
    pub elements: Vec<XsqElement>,
    /// Effects (on models) and timing marks not read because the file has more than
    /// [`MAX_ITEMS`] of them.
    pub effects_unread: usize,
    pub marks_unread: usize,
    /// Notes from reading the file (compressed blocks that couldn't be read, ...).
    pub notes: Vec<String>,
}

fn bad(reason: impl Into<String>) -> XlightsError {
    XlightsError::BadFile(FILE, reason.into())
}

/// Standard base64 (whitespace ignored, padding optional). `None` when it isn't base64.
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    fn value(c: u8) -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        } as u32)
    }
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    let mut padding = false;
    for &c in text.as_bytes() {
        if c.is_ascii_whitespace() {
            continue;
        }
        if c == b'=' {
            padding = true;
            continue;
        }
        if padding {
            return None;
        }
        acc = (acc << 6) | value(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Decompresses one `<CompressedData>` block: base64 of zstd-compressed XML, at most `limit`
/// bytes once decompressed.
fn decompress_block(text: &str, limit: usize) -> Result<String, String> {
    let compressed = base64_decode(text).ok_or("it isn't valid base64")?;
    let mut decoder =
        zstd::stream::read::Decoder::new(compressed.as_slice()).map_err(|e| format!("zstd: {e}"))?;
    let mut out = Vec::new();
    (&mut decoder)
        .take(limit as u64 + 1)
        .read_to_end(&mut out)
        .map_err(|_| "it isn't valid zstd data".to_string())?;
    if out.len() > limit {
        return Err(format!(
            "it holds more than {} MB of data",
            MAX_XSQ_BYTES / (1024 * 1024)
        ));
    }
    String::from_utf8(out).map_err(|_| "it isn't text".to_string())
}

fn children<'a, 'i>(node: Node<'a, 'i>, name: &'static str) -> impl Iterator<Item = Node<'a, 'i>> {
    node.children()
        .filter(move |c| c.is_element() && c.tag_name().name() == name)
}

fn text(node: Node<'_, '_>) -> String {
    node.text().unwrap_or("").to_string()
}

fn attr(node: Node<'_, '_>, name: &str) -> Option<String> {
    node.attribute(name).map(str::to_string)
}

/// xLights trims spaces and tabs from element names.
fn trim_name(name: &str) -> String {
    name.trim_matches([' ', '\t']).to_string()
}

/// The sections of one document (the file, or a decompressed block) read into `out`.
struct Reader<'o> {
    out: &'o mut XsqFile,
    items: usize,
}

impl Reader<'_> {
    fn section(&mut self, node: Node<'_, '_>) {
        match node.tag_name().name() {
            "head" => self.head(node),
            "ColorPalettes" => self.out.palettes.extend(children(node, "ColorPalette").map(text)),
            "EffectDB" => self.out.effect_db.extend(children(node, "Effect").map(text)),
            "ElementEffects" => {
                for element in children(node, "Element") {
                    self.element(element);
                }
            }
            _ => {}
        }
    }

    fn head(&mut self, head: Node<'_, '_>) {
        let h = &mut self.out.head;
        for field in head.children().filter(Node::is_element) {
            let value = text(field);
            match field.tag_name().name() {
                "song" => h.song = value,
                "artist" => h.artist = value,
                "mediaFile" => h.media_file = value,
                "sequenceType" => h.sequence_type = value,
                "sequenceDuration" => h.duration = value,
                "sequenceTiming" => h.timing = value,
                _ => {}
            }
        }
    }

    fn effect(&mut self, node: Node<'_, '_>, kind: ElementKind) -> Option<XsqEffect> {
        if self.items >= MAX_ITEMS {
            match kind {
                ElementKind::Timing => self.out.marks_unread += 1,
                _ => self.out.effects_unread += 1,
            }
            return None;
        }
        self.items += 1;
        let name = match kind {
            ElementKind::Timing => attr(node, "label"),
            _ => attr(node, "name"),
        };
        Some(XsqEffect {
            name: name.unwrap_or_default(),
            settings_ref: attr(node, "ref"),
            inline_settings: if node.attribute("ref").is_none() {
                text(node)
            } else {
                String::new()
            },
            palette_ref: attr(node, "palette"),
            start: attr(node, "startTime").unwrap_or_default(),
            end: attr(node, "endTime").unwrap_or_default(),
        })
    }

    fn element(&mut self, node: Node<'_, '_>) {
        let kind = match node.attribute("type") {
            Some("model") => ElementKind::Model,
            Some("timing") => ElementKind::Timing,
            _ => ElementKind::Other,
        };
        let mut element = XsqElement {
            kind,
            name: trim_name(node.attribute("name").unwrap_or("")),
            fixed: attr(node, "fixed"),
            layers: Vec::new(),
            submodels: Vec::new(),
            sub_effects: 0,
        };
        for layer in node.children().filter(Node::is_element) {
            match layer.tag_name().name() {
                "EffectLayer" => {
                    let effects = children(layer, "Effect")
                        .filter_map(|e| self.effect(e, kind))
                        .collect();
                    element.layers.push(XsqLayer { effects });
                }
                "SubModelEffectLayer" => {
                    let effects = children(layer, "Effect")
                        .filter_map(|e| self.effect(e, kind))
                        .collect();
                    element.submodels.push(XsqSubmodelLayer {
                        name: trim_name(layer.attribute("name").unwrap_or("")),
                        layer: layer
                            .attribute("layer")
                            .and_then(|l| l.trim().parse::<usize>().ok())
                            .unwrap_or(0),
                        effects,
                    });
                }
                "Strand" => {
                    // Strands hold node layers one level down.
                    element.sub_effects += layer
                        .descendants()
                        .filter(|d| d.is_element() && d.tag_name().name() == "Effect")
                        .count();
                }
                _ => {}
            }
        }
        self.out.elements.push(element);
    }
}

/// Reads an `.xsq` file's text. Sections are read wherever they are in the file (xLights writes
/// palettes and effect settings first). `<CompressedData>` blocks are decompressed and read as
/// if their contents were written in their place at the end of the file, as xLights does.
pub fn parse_xsq(input: &str) -> Result<XsqFile, XlightsError> {
    let doc = xml::parse(input).map_err(bad)?;
    let root = doc.root_element();
    if root.tag_name().name() != "xsequence" {
        return Err(bad(format!(
            "expected an <xsequence> document, found <{}>",
            root.tag_name().name()
        )));
    }
    let mut out = XsqFile {
        fixed_point: root.attribute("FixedPointTiming").is_some(),
        ..XsqFile::default()
    };
    let mut reader = Reader {
        out: &mut out,
        items: 0,
    };
    let mut blocks = Vec::new();
    for section in root.children().filter(Node::is_element) {
        if section.tag_name().name() == "CompressedData" {
            blocks.push(text(section));
        } else {
            reader.section(section);
        }
    }
    // Decompressed data shares the file's size budget.
    let mut budget = MAX_XSQ_BYTES.saturating_sub(input.len() + 64);
    let mut failed = 0;
    for block in blocks {
        let parsed = decompress_block(&block, budget).and_then(|inner| {
            budget = budget.saturating_sub(inner.len());
            // The block holds top-level elements without a single root; wrap them.
            let wrapped = format!("<CompressedData>{inner}</CompressedData>");
            let doc = xml::parse(&wrapped)?;
            for section in doc.root_element().children().filter(Node::is_element) {
                reader.section(section);
            }
            Ok(())
        });
        if parsed.is_err() {
            failed += 1;
        }
    }
    if failed > 0 {
        out.notes.push(format!(
            "{failed} compressed part{} of the sequence couldn't be read, so {} effects weren't imported.",
            if failed == 1 { "" } else { "s" },
            if failed == 1 { "its" } else { "their" }
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_decodes_with_and_without_padding() {
        assert_eq!(base64_decode("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64_decode("aGVs\nbG8").unwrap(), b"hello");
        assert_eq!(base64_decode("").unwrap(), b"");
        assert!(base64_decode("aGV*").is_none());
        assert!(base64_decode("aG=Vs").is_none());
    }

    #[test]
    fn reads_sections_in_any_order() {
        let xml = r#"<xsequence FixedPointTiming="1">
  <ElementEffects>
    <Element type="model" name=" Roof	">
      <EffectLayer><Effect ref="0" name="On" palette="0" startTime="0" endTime="1000"/></EffectLayer>
      <EffectLayer/>
      <SubModelEffectLayer name=" Left "><Effect name="On" startTime="0" endTime="10"/></SubModelEffectLayer>
      <SubModelEffectLayer name="Left" layer="2"><Effect name="Off" startTime="0" endTime="10"/></SubModelEffectLayer>
      <Strand index="0"><Effect name="On" startTime="0" endTime="10"/><Node index="2"><Effect name="On" startTime="0" endTime="10"/></Node></Strand>
    </Element>
    <Element type="timing" name="Beats" fixed="500"><EffectLayer/></Element>
    <Element type="timing" name="Lyrics"><EffectLayer><Effect label="Hi" startTime="0" endTime="500"/></EffectLayer></Element>
  </ElementEffects>
  <EffectDB><Effect>E_X=1</Effect></EffectDB>
  <ColorPalettes><ColorPalette>C_BUTTON_Palette1=#FF0000,C_CHECKBOX_Palette1=1</ColorPalette></ColorPalettes>
  <head><song>Song</song><sequenceTiming>50 ms</sequenceTiming><sequenceDuration>12.5</sequenceDuration></head>
</xsequence>"#;
        let f = parse_xsq(xml).unwrap();
        assert!(f.fixed_point);
        assert_eq!(f.head.song, "Song");
        assert_eq!(f.head.timing, "50 ms");
        assert_eq!(f.effect_db, vec!["E_X=1"]);
        assert_eq!(f.palettes.len(), 1);
        let roof = &f.elements[0];
        assert_eq!((roof.kind, roof.name.as_str()), (ElementKind::Model, "Roof"));
        assert_eq!(roof.layers.len(), 2);
        assert_eq!(roof.sub_effects, 2, "strand and node effects");
        assert_eq!(roof.submodel_effects(), 2);
        assert_eq!(
            roof.submodels
                .iter()
                .map(|s| (s.name.as_str(), s.layer, s.effects[0].name.as_str()))
                .collect::<Vec<_>>(),
            vec![("Left", 0, "On"), ("Left", 2, "Off")]
        );
        assert_eq!(roof.layers[0].effects[0].settings_ref.as_deref(), Some("0"));
        assert_eq!(f.elements[1].fixed.as_deref(), Some("500"));
        assert_eq!(f.elements[2].layers[0].effects[0].name, "Hi");
    }

    #[test]
    fn other_documents_and_oversized_files_are_rejected() {
        assert!(
            parse_xsq("<xrgb/>")
                .unwrap_err()
                .to_string()
                .contains("<xsequence>")
        );
        assert!(parse_xsq("<xsequence").is_err());
        let dtd = r#"<!DOCTYPE x [<!ENTITY a "aaaa">]><xsequence>&a;</xsequence>"#;
        assert!(parse_xsq(dtd).is_err(), "DTDs (entity expansion) are refused");
    }

    #[test]
    fn bad_compressed_blocks_are_reported_not_fatal() {
        let xml = r#"<xsequence FixedPointTiming="1"><CompressedData size="10">!!!</CompressedData><CompressedData size="5">aGVsbG8=</CompressedData></xsequence>"#;
        let f = parse_xsq(xml).unwrap();
        assert_eq!(f.notes.len(), 1);
        assert!(f.notes[0].starts_with("2 compressed parts"), "{:?}", f.notes);
    }

    #[test]
    fn compressed_blocks_stay_within_the_size_limit() {
        // 1 MB of zeros compresses to a few bytes; with a smaller budget it's refused.
        let bomb = zstd::bulk::compress(&vec![b' '; 1 << 20], 3).unwrap();
        let b64 = base64_encode(&bomb);
        assert!(decompress_block(&b64, 1000).unwrap_err().contains("more than"));
        assert_eq!(decompress_block(&b64, 2 << 20).unwrap().len(), 1 << 20);
    }

    /// Test helper: standard base64 with padding.
    pub(crate) fn base64_encode(bytes: &[u8]) -> String {
        const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let n = chunk
                .iter()
                .enumerate()
                .fold(0u32, |n, (i, &b)| n | (u32::from(b) << (16 - 8 * i)));
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(A[(n >> (18 - 6 * i)) as usize & 63] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    #[test]
    fn deep_nesting_is_refused_before_parsing() {
        let deep = format!(
            "<xsequence>{}{}</xsequence>",
            "<a>".repeat(100),
            "</a>".repeat(100)
        );
        assert!(
            parse_xsq(&deep)
                .unwrap_err()
                .to_string()
                .contains("nested more than 64")
        );
    }

    #[test]
    fn base64_round_trips() {
        for len in 0..10 {
            let bytes: Vec<u8> = (0..len).map(|i| (i * 37) as u8).collect();
            assert_eq!(base64_decode(&base64_encode(&bytes)).unwrap(), bytes);
        }
    }
}
