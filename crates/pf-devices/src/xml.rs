//! Reading the small XML a controller serves (a Falcon's `/status.xml`), with limits. During a
//! scan, whatever answers on the network is asked for that page and its answer parsed, so its
//! size, nesting, and node count are bounded before the parser sees it.

use roxmltree::{Document, ParsingOptions};

/// Largest status page read (a Falcon's is a few hundred bytes).
const MAX_BYTES: usize = 256 * 1024;
/// Deepest element nesting read (a Falcon's status nests two deep). The XML parser recurses per
/// level, so deeper XML could otherwise exhaust the stack, which ends the whole app.
const MAX_DEPTH: usize = 32;
/// Most XML nodes read.
const MAX_NODES: u32 = 50_000;

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

/// Parses a controller's status page, or says why it isn't read.
pub(crate) fn parse(xml: &str) -> Result<Document<'_>, String> {
    if xml.len() > MAX_BYTES {
        return Err("it is too large to be a controller's status".to_string());
    }
    if too_deep(xml) {
        return Err(format!("its elements are nested more than {MAX_DEPTH} deep"));
    }
    let options = ParsingOptions {
        nodes_limit: MAX_NODES,
        ..ParsingOptions::default()
    };
    Document::parse_with_options(xml, options).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_falcon_status_page_is_read() {
        let doc = parse(include_str!("../fixtures/falcon/status.xml")).unwrap();
        assert!(doc.root_element().has_tag_name("response"));
        let busy = format!(
            "<response><!-- <a><a> --><y a='>'/>{}<![CDATA[<a><a>]]><p>130</p></response>",
            "<b/><c></c>".repeat(1000)
        );
        assert!(parse(&busy).is_ok());
    }

    #[test]
    fn deep_nesting_is_refused_before_parsing() {
        let deep = format!("<x>{}{}</x>", "<a>".repeat(40), "</a>".repeat(40));
        assert!(parse(&deep).unwrap_err().contains("nested more than 32"));
        // What 256 KiB can hold: far deeper than the stack could take if it reached the parser.
        let abyss = "<a>".repeat(MAX_BYTES / 3);
        assert!(parse(&abyss).unwrap_err().contains("nested more than 32"));
        // Closing tags hidden in a comment don't lower the count.
        let hidden = "<a><!-- </a> -->".repeat(10_000);
        assert!(parse(&hidden).unwrap_err().contains("nested more than 32"));
    }

    #[test]
    fn a_page_too_large_or_with_too_many_nodes_is_refused() {
        let large = format!("<x>{}</x>", " ".repeat(MAX_BYTES));
        assert!(parse(&large).unwrap_err().contains("too large"));
        let many = format!("<x>{}</x>", "<a/>".repeat(60_000));
        assert!(parse(&many).is_err());
    }
}
