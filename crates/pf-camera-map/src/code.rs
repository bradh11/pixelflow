//! The coded flash sequence: what every pixel shows in every slot.
//!
//! A sequence is a list of equal-length slots. It opens with a sync preamble (dark and all-white
//! slots in a pattern that can't be mistaken for code), then shows each colour on every pixel as a
//! reference (which also reveals pixels whose colour order is wrong), then the pixel's number one
//! digit per slot, two check digits, and a dark tail. It loops, so a recording that misses the
//! start catches the next pass.
//!
//! Pixel numbers are sent as `index + 1`, so a code of all "off" digits never decodes.

use serde::{Deserialize, Serialize};

/// The default slot length: long enough for a phone at 30 fps to catch several clean frames per
/// slot, even with a little network and camera latency.
pub const DEFAULT_SLOT_SECONDS: f32 = 0.5;

/// How each digit is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Base {
    /// Off, red, green, or blue: half as many slots as binary. Needs a camera that tells the
    /// colours apart (any phone, in a dark room).
    #[default]
    Four,
    /// Off or white: for single-colour pixels or a camera that washes out colour.
    Two,
}

impl Base {
    pub fn radix(self) -> u32 {
        match self {
            Base::Four => 4,
            Base::Two => 2,
        }
    }
}

/// A colour one pixel shows during one slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Symbol {
    Off,
    Red,
    Green,
    Blue,
    White,
}

/// What a slot of the sequence is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "index", rename_all = "camelCase")]
pub enum Slot {
    /// Every pixel off (sync, and the background the decoder subtracts).
    Dark,
    /// Every pixel white (sync, and where pixels are found).
    White,
    /// Every pixel one colour: 0 red, 1 green, 2 blue.
    Reference(u8),
    /// Digit `i` of the pixel's number, least significant first.
    Digit(u8),
    /// Check digit `i`.
    Check(u8),
}

/// The sync preamble, as whether each slot is lit. Asymmetric, so it lines up only one way.
pub const PREAMBLE: [bool; 6] = [false, true, false, true, true, false];

/// How many check digits follow the number.
pub const CHECK_DIGITS: u8 = 2;

/// A coded sequence for `pixels` pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeSpec {
    pub pixels: u32,
    #[serde(default)]
    pub base: Base,
    #[serde(default = "default_slot_seconds")]
    pub slot_seconds: f32,
}

fn default_slot_seconds() -> f32 {
    DEFAULT_SLOT_SECONDS
}

impl CodeSpec {
    pub fn new(pixels: u32, base: Base) -> Self {
        Self {
            pixels,
            base,
            slot_seconds: DEFAULT_SLOT_SECONDS,
        }
    }

    /// Digits needed for every pixel's number (`index + 1`, up to `pixels`).
    pub fn digits(&self) -> u8 {
        let radix = u64::from(self.base.radix());
        let mut digits = 1u8;
        let mut reach = radix;
        while reach <= u64::from(self.pixels) {
            reach *= radix;
            digits += 1;
        }
        digits
    }

    /// The slots in order.
    pub fn slots(&self) -> Vec<Slot> {
        let mut slots: Vec<Slot> = PREAMBLE
            .iter()
            .map(|&lit| if lit { Slot::White } else { Slot::Dark })
            .collect();
        slots.extend((0..3).map(Slot::Reference));
        slots.extend((0..self.digits()).map(Slot::Digit));
        slots.extend((0..CHECK_DIGITS).map(Slot::Check));
        slots.push(Slot::Dark);
        slots
    }

    /// One pass of the sequence, in seconds.
    pub fn duration(&self) -> f32 {
        self.slots().len() as f32 * self.slot_seconds
    }

    /// The digits of pixel `index`'s code (number then check digits), least significant first.
    pub fn code(&self, index: u32) -> Vec<u8> {
        let radix = self.base.radix();
        let mut value = index + 1;
        let mut digits: Vec<u8> = (0..self.digits())
            .map(|_| {
                let d = (value % radix) as u8;
                value /= radix;
                d
            })
            .collect();
        let checks = check_digits(&digits, radix);
        digits.extend(checks);
        digits
    }

    /// The pixel index a code reads as (number then check digits), or `None` when the check
    /// digits don't match or the number is out of range.
    pub fn read(&self, code: &[u8]) -> Option<u32> {
        let n = usize::from(self.digits());
        if code.len() != n + usize::from(CHECK_DIGITS) {
            return None;
        }
        let radix = self.base.radix();
        let (number, checks) = code.split_at(n);
        if checks != check_digits(number, radix) {
            return None;
        }
        let value = number.iter().rev().try_fold(0u64, |v, &d| {
            (u32::from(d) < radix).then_some(v * u64::from(radix) + u64::from(d))
        })?;
        (1..=u64::from(self.pixels))
            .contains(&value)
            .then(|| (value - 1) as u32)
    }

    /// What pixel `index` shows during `slot`.
    pub fn symbol(&self, slot: Slot, index: u32) -> Symbol {
        match slot {
            Slot::Dark => Symbol::Off,
            Slot::White => Symbol::White,
            Slot::Reference(c) => [Symbol::Red, Symbol::Green, Symbol::Blue][usize::from(c % 3)],
            Slot::Digit(i) => self.digit_symbol(self.code(index)[usize::from(i)]),
            Slot::Check(i) => self.digit_symbol(self.code(index)[usize::from(self.digits() + i)]),
        }
    }

    fn digit_symbol(&self, digit: u8) -> Symbol {
        match (self.base, digit) {
            (_, 0) => Symbol::Off,
            (Base::Two, _) => Symbol::White,
            (Base::Four, 1) => Symbol::Red,
            (Base::Four, 2) => Symbol::Green,
            (Base::Four, _) => Symbol::Blue,
        }
    }

    /// What pixel `index` shows `t` seconds after the sequence started (it loops).
    pub fn symbol_at(&self, t: f32, index: u32) -> Symbol {
        let slots = self.slots();
        let slot = (t.max(0.0) / self.slot_seconds.max(0.01)) as usize % slots.len();
        self.symbol(slots[slot], index)
    }
}

/// The check digits of `number`: its digit sum, and a position-weighted sum, both mod `radix`.
fn check_digits(number: &[u8], radix: u32) -> [u8; CHECK_DIGITS as usize] {
    let (plain, weighted) = number.iter().enumerate().fold((0u32, 0u32), |(p, w), (i, &d)| {
        (p + u32::from(d), w + (i as u32 + 1) * u32::from(d))
    });
    [(plain % radix) as u8, (weighted % radix) as u8]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_cover_every_pixel_number() {
        assert_eq!(CodeSpec::new(1, Base::Four).digits(), 1);
        assert_eq!(CodeSpec::new(3, Base::Four).digits(), 1);
        assert_eq!(CodeSpec::new(4, Base::Four).digits(), 2);
        assert_eq!(CodeSpec::new(1000, Base::Four).digits(), 5);
        assert_eq!(CodeSpec::new(1023, Base::Four).digits(), 5);
        assert_eq!(CodeSpec::new(1024, Base::Four).digits(), 6);
        assert_eq!(CodeSpec::new(1000, Base::Two).digits(), 10);
    }

    #[test]
    fn every_code_reads_back_to_its_pixel() {
        for base in [Base::Four, Base::Two] {
            let spec = CodeSpec::new(700, base);
            for i in 0..700 {
                assert_eq!(spec.read(&spec.code(i)), Some(i), "{base:?} {i}");
            }
        }
    }

    #[test]
    fn a_single_wrong_digit_fails_the_check() {
        let spec = CodeSpec::new(500, Base::Four);
        for i in [0, 7, 255, 499] {
            let code = spec.code(i);
            for at in 0..code.len() {
                for wrong in 0..4 {
                    if wrong == code[at] {
                        continue;
                    }
                    let mut bad = code.clone();
                    bad[at] = wrong;
                    assert_eq!(spec.read(&bad), None, "pixel {i}, digit {at} -> {wrong}");
                }
            }
        }
    }

    #[test]
    fn all_off_and_out_of_range_codes_never_read() {
        let spec = CodeSpec::new(10, Base::Four);
        let n = usize::from(spec.digits() + CHECK_DIGITS);
        assert_eq!(spec.read(&vec![0; n]), None);
        // 11 is past the last pixel's number (10).
        let mut eleven = vec![3, 2];
        eleven.extend(check_digits(&eleven, 4));
        assert_eq!(spec.read(&eleven), None);
        assert_eq!(spec.read(&[1]), None);
    }

    #[test]
    fn slots_start_with_the_preamble_and_end_dark() {
        let spec = CodeSpec::new(100, Base::Four);
        let slots = spec.slots();
        assert_eq!(
            &slots[..6],
            &[
                Slot::Dark,
                Slot::White,
                Slot::Dark,
                Slot::White,
                Slot::White,
                Slot::Dark
            ]
        );
        assert_eq!(
            &slots[6..9],
            &[Slot::Reference(0), Slot::Reference(1), Slot::Reference(2)]
        );
        assert_eq!(slots.len(), 6 + 3 + 4 + 2 + 1);
        assert_eq!(slots.last(), Some(&Slot::Dark));
        assert!((spec.duration() - 8.0).abs() < 1e-6);
    }

    #[test]
    fn symbols_follow_the_slots_and_loop() {
        let spec = CodeSpec::new(100, Base::Four);
        assert_eq!(spec.symbol_at(0.1, 5), Symbol::Off);
        assert_eq!(spec.symbol_at(0.6, 5), Symbol::White);
        assert_eq!(spec.symbol_at(3.1, 5), Symbol::Red);
        assert_eq!(spec.symbol_at(3.6, 5), Symbol::Green);
        assert_eq!(spec.symbol_at(4.1, 5), Symbol::Blue);
        // Pixel 5 is number 6 = digits 2, 1, 0, 0 (least first): green, red, off, off.
        assert_eq!(spec.symbol_at(4.6, 5), Symbol::Green);
        assert_eq!(spec.symbol_at(5.1, 5), Symbol::Red);
        assert_eq!(spec.symbol_at(5.6, 5), Symbol::Off);
        assert_eq!(spec.symbol_at(8.6, 5), spec.symbol_at(0.6, 5));
        let binary = CodeSpec::new(100, Base::Two);
        let lit: Vec<Symbol> = (0..binary.digits())
            .map(|i| binary.symbol(Slot::Digit(i), 5))
            .collect();
        assert_eq!(&lit[..3], &[Symbol::Off, Symbol::White, Symbol::White]);
    }

    #[test]
    fn spec_json_defaults_base_and_slot_length() {
        let spec: CodeSpec = serde_json::from_str(r#"{ "pixels": 12 }"#).unwrap();
        assert_eq!(spec, CodeSpec::new(12, Base::Four));
    }
}
