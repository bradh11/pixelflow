//! Parsing xLights XML files safely: bounded size, node count, and nesting depth, and no DTD
//! that declares anything (so no entity expansion). Every xLights file the importer reads goes
//! through [`parse`].

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

/// Where a bare document type declaration (`<!DOCTYPE name>`: a name and nothing else, which
/// some xLights versions write as `<!DOCTYPE html>`) sits in `xml`'s prolog, as a byte range.
/// The prolog is walked as the XML parser walks it: a byte order mark, the XML declaration,
/// then comments, processing instructions and white space. Any other declaration (one with an
/// internal subset or an external identifier, or one after something else) isn't bare.
fn bare_doctype(xml: &str) -> Option<std::ops::Range<usize>> {
    let b = xml.as_bytes();
    let mut i = if xml.starts_with('\u{feff}') { 3 } else { 0 };
    let skip_to = |i: usize, end: &str| xml[i..].find(end).map(|at| i + at + end.len());
    loop {
        while b.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        let rest = &xml[i..];
        if rest.starts_with("<!--") {
            i = skip_to(i + 4, "-->")?;
        } else if rest.starts_with("<?") {
            i = skip_to(i + 2, "?>")?;
        } else {
            break;
        }
    }
    let start = i;
    i += "<!DOCTYPE".len();
    if !xml[start..].starts_with("<!DOCTYPE") || !b.get(i).is_some_and(u8::is_ascii_whitespace) {
        return None;
    }
    while b.get(i).is_some_and(u8::is_ascii_whitespace) {
        i += 1;
    }
    let name = b[i..]
        .iter()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b':' | b'-' | b'.'))
        .count();
    if name == 0 {
        return None;
    }
    i += name;
    while b.get(i).is_some_and(u8::is_ascii_whitespace) {
        i += 1;
    }
    (b.get(i) == Some(&b'>')).then_some(start..i + 1)
}

/// `xml` with a bare `<!DOCTYPE name>` in its prolog (see [`bare_doctype`]) turned into spaces,
/// so [`parse`] reads it; it declares nothing, and the spaces keep every position the same.
/// Any other document type declaration is left for [`parse`] to refuse.
pub fn without_bare_doctype(xml: &str) -> std::borrow::Cow<'_, str> {
    match bare_doctype(xml) {
        Some(range) => {
            let mut text = xml.to_string();
            text.replace_range(range.clone(), &" ".repeat(range.len()));
            std::borrow::Cow::Owned(text)
        }
        None => std::borrow::Cow::Borrowed(xml),
    }
}

/// Parses an xLights XML file, or explains (for "… isn't a valid xLights file: {reason}") why
/// it won't be read. Document type declarations are refused (so no entity is ever expanded);
/// pass the text through [`without_bare_doctype`] first to read the bare one xLights writes.
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
    Document::parse_with_options(xml, options).map_err(|e| match e {
        roxmltree::Error::DtdDetected => {
            "it declares its own document type (a DTD), which PixelFlow doesn't read".to_string()
        }
        e => e.to_string(),
    })
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

    /// Reads `xml` as the importers do: a bare doctype blanked out, then parsed.
    fn read(xml: &str) -> Result<usize, String> {
        let text = without_bare_doctype(xml);
        parse(&text).map(|d| d.descendants().count())
    }

    /// A file whose one entity, used 10,000 times at the top level, would expand to 2 GB.
    fn quadratic_blowup(before: &str, external_id: &str) -> String {
        let big = "A".repeat(100_000);
        format!(
            "{before}<!DOCTYPE x {external_id}[<!ENTITY a \"{big}\">]><x>{}</x>",
            "&a;".repeat(10_000)
        )
    }

    fn billion_laughs(before: &str) -> String {
        let mut dtd = String::from("<!ENTITY l0 \"lol\">");
        for i in 1..10 {
            dtd.push_str(&format!(
                "<!ENTITY l{i} \"{}\">",
                format!("&l{};", i - 1).repeat(10)
            ));
        }
        format!("{before}<!DOCTYPE x [{dtd}]><x>&l9;</x>")
    }

    #[test]
    fn only_a_bare_doctype_is_read_and_every_other_dtd_is_refused() {
        // What some xLights versions write at the top of their files.
        assert!(read("<?xml version=\"1.0\"?>\n<!DOCTYPE html>\n<xrgb/>").is_ok());
        assert!(
            read("\u{feff}<?xml version=\"1.0\"?><!-- xLights --><?pi x?>\n<!DOCTYPE  html >\n<xrgb/>")
                .is_ok()
        );
        assert!(read("<!DOCTYPE xrgb><xrgb/>").is_ok());
        let refused = [
            // An internal subset, where entities are declared.
            r#"<!DOCTYPE x [<!ENTITY a "aaaa">]><x>&a;</x>"#.to_string(),
            // External identifiers, and an external entity.
            r#"<!DOCTYPE x SYSTEM "file:///etc/passwd"><x/>"#.to_string(),
            r#"<!DOCTYPE x PUBLIC "-//x" "http://example.com/x.dtd"><x/>"#.to_string(),
            r#"<!DOCTYPE x [<!ENTITY e SYSTEM "file:///etc/passwd">]><x>&e;</x>"#.to_string(),
            // A comment hiding a bare doctype before the real one, and a `>` in a SYSTEM literal.
            quadratic_blowup("<!-- <!DOCTYPE z> -->", ""),
            quadratic_blowup("", "SYSTEM \"a>b\" "),
            quadratic_blowup("", ""),
            billion_laughs(""),
            billion_laughs("<!-- a comment first -->"),
        ];
        for xml in &refused {
            let started = std::time::Instant::now();
            let err = read(xml).unwrap_err();
            let head = &xml[..xml.len().min(60)];
            assert!(err.contains("document type"), "{err}: {head}");
            assert!(started.elapsed().as_millis() < 500, "{head}");
        }
        // A bare doctype anywhere but the prolog isn't blanked, and the file isn't read.
        assert!(read("<x/><!DOCTYPE html>").is_err());
        assert_eq!(
            without_bare_doctype("<x><!DOCTYPE html></x>"),
            "<x><!DOCTYPE html></x>"
        );
    }

    #[test]
    fn dtds_and_oversized_files_are_refused() {
        // Without the bare doctype blanked, even that one is refused.
        assert!(
            parse("<!DOCTYPE html><x/>")
                .unwrap_err()
                .contains("document type")
        );
        let huge = " ".repeat(MAX_XML_BYTES + 1);
        assert!(
            parse(&huge)
                .unwrap_err()
                .contains("PixelFlow reads xLights files up to 64 MB")
        );
    }
}
