//! The layout's background photo, from the `<settings>` of `xlights_rgbeffects.xml`.
//!
//! xLights stores the photo as an absolute path on the machine that made the file, so the
//! photo is looked for in the show folder too. Placement follows how xLights draws it: from the
//! bottom-left corner of the layout area (its preview), shrunk to fit unless "scale image" is on.

use pf_model::{Background, path_to_text};
use roxmltree::Node;
use std::io::Read;
use std::path::{Path, PathBuf};

/// PixelFlow units per xLights layout unit (the importer's `LAYOUT_SCALE`, in full precision).
const UNITS_PER_XLIGHTS: f64 = 0.01;

/// The photo settings as xLights wrote them.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct XBackground {
    /// `backgroundImage`: a path as written, on any machine.
    pub image: String,
    /// `backgroundBrightness`, percent (xLights' default is 100).
    pub brightness: i64,
    /// `backgroundAlpha`, percent opaque (xLights' default is 100).
    pub alpha: i64,
    /// `scaleImage`: stretch the photo over the whole layout area.
    pub stretch: bool,
    /// The layout area in xLights units (`previewWidth`, `previewHeight`).
    pub width: f64,
    pub height: f64,
    /// `Display2DCenter0`: the layout's origin is in the middle of its width.
    pub centered: bool,
}

/// Reads the photo settings. `None` when the file names no photo (or isn't readable XML, which
/// the layout import reports).
pub(crate) fn parse(xml: &str) -> Option<XBackground> {
    let xml = crate::xml::without_bare_doctype(xml);
    let doc = crate::xml::parse(&xml).ok()?;
    let settings = doc
        .root_element()
        .children()
        .find(|c| c.is_element() && c.tag_name().name() == "settings")?;
    let value = |name: &str| -> Option<&str> {
        settings
            .children()
            .filter(|c: &Node| c.is_element() && c.tag_name().name() == name)
            .find_map(|c| c.attribute("value"))
    };
    let number = |name: &str, default: i64| {
        value(name)
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .map_or(default, |v| v as i64)
    };
    let image = value("backgroundImage")?.trim().to_string();
    if image.is_empty() {
        return None;
    }
    Some(XBackground {
        image,
        brightness: number("backgroundBrightness", 100),
        alpha: number("backgroundAlpha", 100),
        stretch: number("scaleImage", 0) != 0,
        width: number("previewWidth", 1280).max(1) as f64,
        height: number("previewHeight", 720).max(1) as f64,
        centered: number("Display2DCenter0", 0) != 0,
    })
}

/// The path split into its folder and file names, whichever slashes it uses. A drive (`C:`) and
/// `.`/`..` parts are dropped, so joining the result onto a folder never leaves it.
fn parts(path: &str) -> Vec<&str> {
    path.split(['/', '\\'])
        .filter(|p| !p.is_empty() && *p != "." && *p != ".." && !p.ends_with(':'))
        .collect()
}

/// Where the photo is on this computer: as written, else by name in `folder`, else under
/// `folder` with the old show folder's part of the path removed.
fn find(written: &str, folder: &Path) -> Option<PathBuf> {
    let as_written = Path::new(written);
    if as_written.is_absolute() && as_written.is_file() {
        return Some(as_written.to_path_buf());
    }
    let parts = parts(written);
    let name = parts.last()?;
    let by_name = folder.join(name);
    if by_name.is_file() {
        return Some(by_name);
    }
    // `/Users/me/Documents/xlights/Assets/a.jpg` -> `<folder>/Assets/a.jpg`: the longest tail
    // that exists wins, so nothing is assumed about how deep the old show folder was.
    (2..parts.len()).rev().find_map(|keep| {
        let mut tail = folder.to_path_buf();
        tail.extend(&parts[parts.len() - keep..]);
        tail.is_file().then_some(tail)
    })
}

/// The photo as a PixelFlow background, with a note for anything that didn't come across.
pub(crate) fn build(x: &XBackground, folder: &Path, notes: &mut Vec<String>) -> Option<Background> {
    let name = parts(&x.image).last().copied().unwrap_or(&x.image);
    let Some(found) = find(&x.image, folder) else {
        notes.push(format!(
            "The layout photo {name} wasn't found: xLights has it somewhere on another computer and it \
             isn't in the xLights folder. Copy it into that folder and import again, or choose it on the \
             Layout screen."
        ));
        return None;
    };
    let found = std::path::absolute(&found).unwrap_or(found);

    // xLights draws the photo from the layout area's bottom-left corner (its middle, for a
    // centered layout), shrunk to fit unless it's stretched to fill.
    let (area_w, area_h) = (x.width * UNITS_PER_XLIGHTS, x.height * UNITS_PER_XLIGHTS);
    let left = if x.centered { -area_w / 2.0 } else { 0.0 };
    let shape = image_size(&found);
    let (width, top) = match shape {
        Some((w, h)) => {
            let (scale_w, scale_h) = (w / x.width, h / x.height);
            let width = if x.stretch || scale_w >= scale_h {
                area_w
            } else {
                area_w * scale_w / scale_h
            };
            if x.stretch && ((h / w) - (x.height / x.width)).abs() > 0.02 {
                notes.push(format!(
                    "xLights stretches the layout photo {name} to fill the layout; PixelFlow keeps its shape, \
                     so it may not fill the same area."
                ));
            }
            (width, width * h / w)
        }
        None => {
            notes.push(format!(
                "PixelFlow couldn't read the size of the layout photo {name}, so it's spread across the \
                 layout area; move or resize it on the Layout screen if it doesn't line up."
            ));
            (area_w, area_h)
        }
    };

    // xLights dims the photo (brightness) and fades it (alpha, percent opaque); PixelFlow has one
    // strength. A photo xLights hides completely (alpha 0) would arrive invisible, so then only
    // the brightness counts.
    let brightness = x.brightness.clamp(0, 100) as f32 / 100.0;
    let alpha = x.alpha.clamp(0, 100) as f32 / 100.0;
    let opacity = if alpha == 0.0 {
        notes.push(format!(
            "xLights has the layout photo {name} fully transparent, so PixelFlow shows it at its \
             brightness setting instead ({}%). Change its strength on the Layout screen.",
            x.brightness.clamp(0, 100)
        ));
        brightness
    } else {
        brightness * alpha
    };
    let mut background = Background::new(path_to_text(&found), left as f32, top as f32, width as f32);
    background.opacity = (opacity * 100.0).round() / 100.0;
    Some(background)
}

/// The photo's size as it's shown (JPEG orientation applied), for PNG, JPEG and GIF files.
fn image_size(path: &Path) -> Option<(f64, f64)> {
    // A header is all that's needed; JPEG's can sit behind a few hundred KB of thumbnail.
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(8 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .ok()?;
    let (w, h) = if bytes.starts_with(b"\x89PNG") && bytes.len() >= 24 {
        (be32(&bytes[16..]), be32(&bytes[20..]))
    } else if bytes.starts_with(b"GIF8") && bytes.len() >= 10 {
        (le16(&bytes[6..]), le16(&bytes[8..]))
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        return jpeg_size(&bytes);
    } else {
        return None;
    };
    (w > 0 && h > 0).then_some((f64::from(w), f64::from(h)))
}

fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

fn le16(b: &[u8]) -> u32 {
    u32::from(u16::from_le_bytes([b[0], b[1]]))
}

fn be16(b: &[u8]) -> u32 {
    u32::from(u16::from_be_bytes([b[0], b[1]]))
}

/// Walks a JPEG's segments for its frame size and EXIF orientation (5-8 turn it sideways).
fn jpeg_size(b: &[u8]) -> Option<(f64, f64)> {
    let mut i = 2;
    let mut sideways = false;
    while i + 4 <= b.len() {
        if b[i] != 0xff {
            return None;
        }
        let marker = b[i + 1];
        if marker == 0xff {
            i += 1;
            continue;
        }
        let len = be16(&b[i + 2..]) as usize;
        let body = b.get(i + 4..i + 2 + len)?;
        match marker {
            0xe1 if body.starts_with(b"Exif\0\0") => sideways = exif_sideways(&body[6..]),
            0xc0..=0xcf if !matches!(marker, 0xc4 | 0xc8 | 0xcc) && body.len() >= 5 => {
                let (h, w) = (be16(&body[1..]), be16(&body[3..]));
                if w == 0 || h == 0 {
                    return None;
                }
                let (w, h) = (f64::from(w), f64::from(h));
                return Some(if sideways { (h, w) } else { (w, h) });
            }
            _ => {}
        }
        i += 2 + len;
    }
    None
}

/// Whether the EXIF block (a TIFF structure) says to turn the photo a quarter turn.
fn exif_sideways(tiff: &[u8]) -> bool {
    let little = match tiff.get(..2) {
        Some(b"II") => true,
        Some(b"MM") => false,
        _ => return false,
    };
    let u16_at = |at: usize| {
        tiff.get(at..at + 2).map(|b| {
            if little {
                u16::from_le_bytes([b[0], b[1]])
            } else {
                u16::from_be_bytes([b[0], b[1]])
            }
        })
    };
    let u32_at = |at: usize| {
        tiff.get(at..at + 4).map(|b| {
            let b = [b[0], b[1], b[2], b[3]];
            if little {
                u32::from_le_bytes(b)
            } else {
                u32::from_be_bytes(b)
            }
        })
    };
    let Some(first) = u32_at(4) else { return false };
    let first = first as usize;
    let Some(count) = u16_at(first) else { return false };
    (0..usize::from(count)).any(|n| {
        let entry = first + 2 + n * 12;
        u16_at(entry) == Some(0x0112) && u16_at(entry + 8).is_some_and(|o| (5..=8).contains(&o))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pf-xlights-bg-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A PNG header for a `w` x `h` image (enough for its size).
    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        b.extend(w.to_be_bytes());
        b.extend(h.to_be_bytes());
        b
    }

    /// A JPEG header: optional EXIF orientation, then a frame of `w` x `h`.
    fn jpeg(w: u16, h: u16, orientation: Option<u16>) -> Vec<u8> {
        let mut b = vec![0xff, 0xd8];
        if let Some(o) = orientation {
            let mut exif = b"Exif\0\0MM\0\x2a\0\0\0\x08\0\x01\x01\x12\0\x03\0\0\0\x01".to_vec();
            exif.extend(o.to_be_bytes());
            exif.extend([0, 0, 0, 0, 0, 0]);
            b.extend([0xff, 0xe1]);
            b.extend((exif.len() as u16 + 2).to_be_bytes());
            b.extend(exif);
        }
        b.extend([0xff, 0xc0, 0, 11, 8]);
        b.extend(h.to_be_bytes());
        b.extend(w.to_be_bytes());
        b.extend([1, 1, 0x11, 0]);
        b
    }

    fn settings(image: &str) -> XBackground {
        XBackground {
            image: image.into(),
            brightness: 100,
            alpha: 100,
            stretch: false,
            width: 1000.0,
            height: 500.0,
            centered: false,
        }
    }

    #[test]
    fn the_photo_is_found_as_written_then_by_name_then_under_the_old_show_folder() {
        let dir = scratch("find");
        fs::create_dir_all(dir.join("Assets")).unwrap();
        fs::write(dir.join("house.png"), png(10, 10)).unwrap();
        fs::write(dir.join("Assets/a.png"), png(10, 10)).unwrap();
        fs::write(dir.join("Assets/house.png"), png(10, 10)).unwrap();
        let elsewhere = dir.join("Assets/a.png");
        // As written, even though the folder holds a file of the same name.
        assert_eq!(find(&elsewhere.to_string_lossy(), &dir), Some(elsewhere.clone()));
        // By name: the folder's own copy wins over the deeper one (and Windows paths work).
        assert_eq!(
            find("/Users/x/Documents/xlights/house.png", &dir),
            Some(dir.join("house.png"))
        );
        assert_eq!(
            find("C:\\Users\\x\\xlights\\house.png", &dir),
            Some(dir.join("house.png"))
        );
        // Under the old show folder.
        assert_eq!(
            find("/Users/x/Documents/xlights/Assets/a.png", &dir),
            Some(dir.join("Assets/a.png"))
        );
        assert_eq!(find("/Users/x/Documents/xlights/Assets/missing.png", &dir), None);
        assert_eq!(find("../../etc/house.png", &dir), Some(dir.join("house.png")));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_photo_is_left_out_with_its_name_in_the_note() {
        let dir = scratch("missing");
        let mut notes = Vec::new();
        assert_eq!(
            build(&settings("/Users/x/xlights/gone.jpg"), &dir, &mut notes),
            None
        );
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("gone.jpg wasn't found"), "{notes:?}");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_photo_is_placed_like_xlights_draws_it() {
        let dir = scratch("place");
        let mut notes = Vec::new();
        // Twice as wide as the 1000 x 500 layout area is tall: fills the width, sits on the bottom edge.
        fs::write(dir.join("wide.png"), png(2000, 500)).unwrap();
        let wide = build(&settings("wide.png"), &dir, &mut notes).unwrap();
        assert_eq!((wide.x, wide.y, wide.width), (0.0, 2.5, 10.0));
        // A tall photo is shrunk to the layout's height: 250 x 500 in 1000 x 500 is a quarter as wide.
        fs::write(dir.join("tall.png"), png(250, 500)).unwrap();
        let mut centered = settings("tall.png");
        centered.centered = true;
        let tall = build(&centered, &dir, &mut notes).unwrap();
        assert_eq!((tall.x, tall.y, tall.width), (-5.0, 5.0, 2.5));
        assert!(notes.is_empty(), "{notes:?}");
        assert!(wide.path.ends_with("wide.png") && std::path::Path::new(&wide.path).is_absolute());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn brightness_and_alpha_become_the_photos_strength() {
        let dir = scratch("strength");
        fs::write(dir.join("p.png"), png(1000, 500)).unwrap();
        let mut notes = Vec::new();
        let mut x = settings("p.png");
        x.brightness = 50;
        x.alpha = 80;
        assert_eq!(build(&x, &dir, &mut notes).unwrap().opacity, 0.4);
        assert!(notes.is_empty());
        // Fully transparent in xLights: shown at its brightness instead of vanishing, with a note.
        x.alpha = 0;
        assert_eq!(build(&x, &dir, &mut notes).unwrap().opacity, 0.5);
        assert!(notes[0].contains("fully transparent"), "{notes:?}");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn photo_size_comes_from_png_gif_and_jpeg_headers_with_orientation() {
        let dir = scratch("sizes");
        let write = |name: &str, bytes: Vec<u8>| {
            fs::write(dir.join(name), bytes).unwrap();
            image_size(&dir.join(name))
        };
        assert_eq!(write("a.png", png(30, 20)), Some((30.0, 20.0)));
        assert_eq!(write("a.gif", b"GIF89a\x1e\0\x14\0".to_vec()), Some((30.0, 20.0)));
        assert_eq!(write("a.jpg", jpeg(30, 20, None)), Some((30.0, 20.0)));
        assert_eq!(write("b.jpg", jpeg(30, 20, Some(1))), Some((30.0, 20.0)));
        assert_eq!(write("c.jpg", jpeg(30, 20, Some(6))), Some((20.0, 30.0)));
        assert_eq!(write("a.txt", b"not an image".to_vec()), None);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn settings_are_read_with_xlights_defaults() {
        let xml = r#"<xrgb><settings>
            <backgroundImage value=" /a/b/IMG.jpeg "/><backgroundBrightness value="50"/>
            <scaleImage value="1"/><Display2DCenter0 value="1"/><previewWidth value="1900"/>
        </settings></xrgb>"#;
        let x = parse(xml).unwrap();
        assert_eq!(
            x,
            XBackground {
                image: "/a/b/IMG.jpeg".into(),
                brightness: 50,
                alpha: 100,
                stretch: true,
                width: 1900.0,
                height: 720.0,
                centered: true,
            }
        );
        assert_eq!(
            parse("<xrgb><settings><backgroundImage value=\"\"/></settings></xrgb>"),
            None
        );
        assert_eq!(parse("<xrgb/>"), None);
    }
}
