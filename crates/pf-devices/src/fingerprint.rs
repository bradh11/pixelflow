//! Recognizing a controller from its web home page, or a Falcon from its `/status.xml`.

use crate::device::DeviceKind;

/// The kind of controller whose home page this is, if any.
///
/// Falcon markers follow xLights' detection (`pixelcontroller.com`, `falcon.css`, `f16v2.js`,
/// `js/cntrlr_`); FPP's pages are titled "Falcon Player"; WLED's are titled "WLED".
pub fn classify_home_page(body: &str) -> Option<DeviceKind> {
    let lower = body.to_ascii_lowercase();
    let has = |needle: &str| lower.contains(needle);
    if has("pixelcontroller.com") || has("falcon.css") || has("f16v2.js") || has("js/cntrlr_") {
        Some(DeviceKind::Falcon)
    } else if has("falcon player") {
        Some(DeviceKind::Fpp)
    } else if has("<title>wled") {
        Some(DeviceKind::Wled)
    } else {
        None
    }
}

/// True when `body` is a Falcon's `/status.xml`: a `<response>` carrying a numeric product code
/// (`<p>`). An F16V5 on firmware Bld 32 answers its home page `/` with a 404, so this is how such
/// a board is recognized.
pub fn is_falcon_status(body: &str) -> bool {
    roxmltree::Document::parse(body).is_ok_and(|doc| {
        let root = doc.root_element();
        root.has_tag_name("response")
            && root
                .children()
                .find(|n| n.has_tag_name("p"))
                .and_then(|n| n.text())
                .is_some_and(|t| t.trim().parse::<u32>().is_ok())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_each_kind_and_ignores_other_devices() {
        assert_eq!(
            classify_home_page("<link href=\"css/falcon.css\">"),
            Some(DeviceKind::Falcon)
        );
        assert_eq!(
            classify_home_page("<title>Falcon Player - FPP</title>"),
            Some(DeviceKind::Fpp)
        );
        assert_eq!(classify_home_page("<title>WLED</title>"), Some(DeviceKind::Wled));
        assert_eq!(classify_home_page("<title>HP Officejet</title>"), None);
    }

    #[test]
    fn recognizes_a_falcon_status_page() {
        assert!(is_falcon_status(include_str!("../fixtures/falcon/status.xml")));
        assert!(!is_falcon_status("<response><p>x</p></response>"));
        assert!(!is_falcon_status("<status><p>130</p></status>"));
        assert!(!is_falcon_status("<html><body>Error 404</body></html>"));
        assert!(!is_falcon_status("not xml"));
    }
}
