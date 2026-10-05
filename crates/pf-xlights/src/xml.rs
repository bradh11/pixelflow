//! Parsing xLights XML files safely: bounded size, node count, and nesting depth, and no DTDs
//! (so no entity expansion). Every xLights file the importer reads goes through [`parse`].

use roxmltree::{Document, ParsingOptions};

/// Largest xLights file read, in bytes (the same limit as a PixelFlow sequence file).
pub const MAX_XML_BYTES: usize = pf_sequence::MAX_SEQUENCE_BYTES;
/// Most XML nodes in one file (a 64 MB file has a few million).
const MAX_NODES: u32 = 20_000_000;
/// Deepest element nesting accepted (xLights files nest under 10 deep). The XML parser recurses
/// per level, so a hostile file could otherwise exhaust the stack.
pub const MAX_DEPTH: usize = 64;

/// True when `xml` nests elements deeper than [`MAX_DEPTH`]. A quick scan, not a parse: it
/// skips comments, CDATA, declarations, and quoted attribute values.
fn too_deep(xml: &str) -> bool {
    let b = xml.as_bytes();
    let mut depth = 0usize;
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &xml[i..];
        let skip_to = |end: &str| rest.find(end).map_or(b.len(), |at| i + at + end.len());
        if rest.starts_with("<!--") {
            i = skip_to("-->");
        } else if rest.starts_with("<![CDATA[") {
            i = skip_to("]]>");
        } else if rest.starts_with("<?") {
            i = skip_to("?>");
        } else if rest.starts_with("<!") {
            i = skip_to(">");
        } else if rest.starts_with("</") {
            depth = depth.saturating_sub(1);
            i = skip_to(">");
        } else {
            // A start tag: find its end outside quotes; `/>` closes it at once.
            let mut j = i + 1;
            let mut quote = None;
            while j < b.len() {
                match (quote, b[j]) {
                    (None, b'"' | b'\'') => quote = Some(b[j]),
                    (Some(q), c) if c == q => quote = None,
                    (None, b'>') => break,
                    _ => {}
                }
                j += 1;
            }
            if j >= b.len() || b[j - 1] != b'/' {
                depth += 1;
                if depth > MAX_DEPTH {
                    return true;
                }
            }
            i = j + 1;
        }
    }
    false
}

/// Parses an xLights XML file, or explains (for "… isn't a valid xLights file: {reason}") why
/// it won't be read.
pub fn parse(xml: &str) -> Result<Document<'_>, String> {
    if xml.len() > MAX_XML_BYTES {
        return Err(format!(
            "it is {} MB; PixelFlow reads xLights files up to {} MB",
            xml.len() / (1024 * 1024),
            MAX_XML_BYTES / (1024 * 1024)
        ));
    }
    if too_deep(xml) {
        return Err(format!("its elements are nested more than {MAX_DEPTH} deep"));
    }
    let options = ParsingOptions {
        allow_dtd: false,
        nodes_limit: MAX_NODES,
        ..ParsingOptions::default()
    };
    Document::parse_with_options(xml, options).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_nesting_is_refused_before_parsing() {
        let deep = format!("<x>{}{}</x>", "<a>".repeat(100), "</a>".repeat(100));
        assert!(parse(&deep).unwrap_err().contains("nested more than 64"));
        // Far deeper than the stack could take if it reached the parser.
        let abyss = format!("<x>{}{}</x>", "<a>".repeat(200_000), "</a>".repeat(200_000));
        assert!(parse(&abyss).is_err());
        let flat = format!(
            "<x><!-- <a><a> --><y a='>' b=\"<\"/>{}<![CDATA[<a><a>]]></x>",
            "<b/><c></c>".repeat(1000)
        );
        assert!(!too_deep(&flat));
        assert!(parse(&flat.replace("b=\"<\"", "")).is_ok());
    }

    #[test]
    fn dtds_and_oversized_files_are_refused() {
        let dtd = r#"<!DOCTYPE x [<!ENTITY a "aaaa">]><x>&a;</x>"#;
        assert!(parse(dtd).is_err());
        let huge = " ".repeat(MAX_XML_BYTES + 1);
        assert!(
            parse(&huge)
                .unwrap_err()
                .contains("PixelFlow reads xLights files up to 64 MB")
        );
    }
}
