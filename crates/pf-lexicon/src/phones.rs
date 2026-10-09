//! ARPAbet phones, as the CMU Pronouncing Dictionary writes them (`G OW1 S T`), and the mouth
//! shape each makes.

use std::fmt;

/// The 39 ARPAbet phones, vowels first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Arpa {
    Aa,
    Ae,
    Ah,
    Ao,
    Aw,
    Ay,
    Eh,
    Er,
    Ey,
    Ih,
    Iy,
    Ow,
    Oy,
    Uh,
    Uw,
    B,
    Ch,
    D,
    Dh,
    F,
    G,
    Hh,
    Jh,
    K,
    L,
    M,
    N,
    Ng,
    P,
    R,
    S,
    Sh,
    T,
    Th,
    V,
    W,
    Y,
    Z,
    Zh,
}

impl Arpa {
    pub const ALL: [Arpa; 39] = [
        Arpa::Aa,
        Arpa::Ae,
        Arpa::Ah,
        Arpa::Ao,
        Arpa::Aw,
        Arpa::Ay,
        Arpa::Eh,
        Arpa::Er,
        Arpa::Ey,
        Arpa::Ih,
        Arpa::Iy,
        Arpa::Ow,
        Arpa::Oy,
        Arpa::Uh,
        Arpa::Uw,
        Arpa::B,
        Arpa::Ch,
        Arpa::D,
        Arpa::Dh,
        Arpa::F,
        Arpa::G,
        Arpa::Hh,
        Arpa::Jh,
        Arpa::K,
        Arpa::L,
        Arpa::M,
        Arpa::N,
        Arpa::Ng,
        Arpa::P,
        Arpa::R,
        Arpa::S,
        Arpa::Sh,
        Arpa::T,
        Arpa::Th,
        Arpa::V,
        Arpa::W,
        Arpa::Y,
        Arpa::Z,
        Arpa::Zh,
    ];

    /// The dictionary's name for it (`AA`, `NG`).
    pub fn name(self) -> &'static str {
        const NAMES: [&str; 39] = [
            "AA", "AE", "AH", "AO", "AW", "AY", "EH", "ER", "EY", "IH", "IY", "OW", "OY", "UH", "UW", "B",
            "CH", "D", "DH", "F", "G", "HH", "JH", "K", "L", "M", "N", "NG", "P", "R", "S", "SH", "T", "TH",
            "V", "W", "Y", "Z", "ZH",
        ];
        NAMES[self as usize]
    }

    pub fn from_name(name: &str) -> Option<Arpa> {
        Arpa::ALL.into_iter().find(|a| a.name() == name)
    }

    pub fn is_vowel(self) -> bool {
        (self as usize) <= Arpa::Uw as usize
    }

    /// The Preston Blair mouth shape xLights shows for it, by xLights' `phoneme_mapping` (from
    /// Papagayo): `AI`, `E`, `O`, `U`, `WQ`, `L`, `MBP`, `FV`, or `etc`.
    pub fn mouth(self) -> &'static str {
        match self {
            Arpa::Aa | Arpa::Ae | Arpa::Ah | Arpa::Ay | Arpa::Ih => "AI",
            Arpa::Ao | Arpa::Aw | Arpa::Ow => "O",
            Arpa::Eh | Arpa::Er | Arpa::Ey | Arpa::Iy => "E",
            Arpa::Uh | Arpa::Uw => "U",
            Arpa::Oy | Arpa::W => "WQ",
            Arpa::B | Arpa::M | Arpa::P => "MBP",
            Arpa::F | Arpa::V => "FV",
            Arpa::L => "L",
            Arpa::Ch
            | Arpa::D
            | Arpa::Dh
            | Arpa::G
            | Arpa::Hh
            | Arpa::Jh
            | Arpa::K
            | Arpa::N
            | Arpa::Ng
            | Arpa::R
            | Arpa::S
            | Arpa::Sh
            | Arpa::T
            | Arpa::Th
            | Arpa::Y
            | Arpa::Z
            | Arpa::Zh => "etc",
        }
    }
}

/// One phone: a sound, with a vowel's stress (0 unstressed, 1 primary, 2 secondary; always 0
/// for a consonant).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Phone {
    pub arpa: Arpa,
    pub stress: u8,
}

impl Phone {
    pub const fn new(arpa: Arpa, stress: u8) -> Self {
        Self { arpa, stress }
    }

    /// Reads `AH0`, `T`, ...
    pub fn parse(text: &str) -> Option<Phone> {
        let (name, stress) = match text.as_bytes().last() {
            Some(d @ b'0'..=b'2') => (&text[..text.len() - 1], d - b'0'),
            _ => (text, 0),
        };
        let arpa = Arpa::from_name(name)?;
        Some(Phone::new(arpa, if arpa.is_vowel() { stress } else { 0 }))
    }

    pub fn is_vowel(self) -> bool {
        self.arpa.is_vowel()
    }

    /// Its byte in the packed dictionary: 0x80 and up, three to a phone (one per stress).
    pub fn code(self) -> u8 {
        0x80 + self.arpa as u8 * 3 + self.stress.min(2)
    }

    pub fn from_code(code: u8) -> Option<Phone> {
        let n = code.checked_sub(0x80)?;
        let arpa = *Arpa::ALL.get(usize::from(n / 3))?;
        Some(Phone::new(arpa, n % 3))
    }
}

impl fmt::Display for Phone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_vowel() {
            write!(f, "{}{}", self.arpa.name(), self.stress)
        } else {
            f.write_str(self.arpa.name())
        }
    }
}

/// Phones as the dictionary writes them: `G OW1 S T`.
pub fn spelled(phones: &[Phone]) -> String {
    phones.iter().map(Phone::to_string).collect::<Vec<_>>().join(" ")
}

/// Reads `G OW1 S T` (`None` if any phone isn't ARPAbet).
pub fn parse_phones(text: &str) -> Option<Vec<Phone>> {
    text.split_whitespace().map(Phone::parse).collect()
}

/// The mouth shapes for phones, as xLights breaks a word down: each phone's shape, a run of
/// `etc` (the many consonants that share it) kept as one.
pub fn mouth_shapes(phones: &[Phone]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for phone in phones {
        let shape = phone.arpa.mouth();
        if !(shape == "etc" && out.last() == Some(&"etc")) {
            out.push(shape);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phones_read_write_and_pack() {
        let phones = parse_phones("G OW1 S T").unwrap();
        assert_eq!(spelled(&phones), "G OW1 S T");
        for arpa in Arpa::ALL {
            for stress in 0..3 {
                let p = Phone::new(arpa, if arpa.is_vowel() { stress } else { 0 });
                assert_eq!(Phone::from_code(p.code()), Some(p));
                assert!(p.code() >= 0x80);
            }
        }
        assert_eq!(Phone::parse("XX"), None);
        assert_eq!(Phone::from_code(b'a'), None);
    }

    #[test]
    fn mouth_shapes_follow_xlights_phoneme_mapping() {
        let shape = |p: &str| Phone::parse(p).unwrap().arpa.mouth();
        // A sample of xLights' table, one per shape.
        for (phone, mouth) in [
            ("AA1", "AI"),
            ("AH0", "AI"),
            ("IH1", "AI"),
            ("AO1", "O"),
            ("AW1", "O"),
            ("OW0", "O"),
            ("EH1", "E"),
            ("ER0", "E"),
            ("IY1", "E"),
            ("UH1", "U"),
            ("UW1", "U"),
            ("OY1", "WQ"),
            ("W", "WQ"),
            ("B", "MBP"),
            ("M", "MBP"),
            ("F", "FV"),
            ("V", "FV"),
            ("L", "L"),
            ("NG", "etc"),
            ("TH", "etc"),
            ("Y", "etc"),
        ] {
            assert_eq!(shape(phone), mouth, "{phone}");
        }
        // "ghost": G OW S T, the S T run kept as one.
        assert_eq!(
            mouth_shapes(&parse_phones("G OW1 S T").unwrap()),
            ["etc", "O", "etc"]
        );
    }
}
