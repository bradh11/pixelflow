//! Recognizing a controller from its web home page.

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
}
