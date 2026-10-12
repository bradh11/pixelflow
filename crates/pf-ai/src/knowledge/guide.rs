//! A guide as it's written: Markdown with a title, a one-line summary, sections, names in
//! backticks, and looks (ready-made effects) in fenced `look` blocks of JSON.

use super::Suit;
use serde_json::Value;

/// A named, ready-made effect from a guide.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub name: String,
    /// The kinds of target it was written for (and is tried on).
    pub suits: Vec<Suit>,
    /// What it looks like, in a sentence.
    pub note: String,
    /// The effect, as `place_effects` takes one: kind, settings, colors, and the rest.
    pub effect: Value,
}

/// A written guide, read.
#[derive(Debug, Clone, PartialEq)]
pub struct Guide {
    pub title: String,
    /// The first paragraph: what the guide is for, in a sentence.
    pub summary: String,
    /// The `##` headings, in order.
    pub sections: Vec<String>,
    pub looks: Vec<Look>,
    /// The guide as the Assistant reads it: the text as written, each look as a line saying what
    /// it is and a line of JSON to place.
    pub text: String,
    /// Every name in backticks in the prose (not in the looks' JSON), in order, repeats and all.
    pub names: Vec<String>,
}

impl Guide {
    /// The text under the `##` heading `title` (up to the next one), or nothing.
    pub fn section(&self, title: &str) -> &str {
        let heading = format!("## {title}\n");
        let Some(start) = self.text.find(&heading) else {
            return "";
        };
        let body = &self.text[start + heading.len()..];
        body.find("\n## ").map_or(body, |end| &body[..end])
    }
}

/// The names in backticks on a line of prose.
fn backticked(line: &str, out: &mut Vec<String>) {
    let mut rest = line;
    while let Some(start) = rest.find('`') {
        let after = &rest[start + 1..];
        let Some(len) = after.find('`') else {
            break;
        };
        out.push(after[..len].to_string());
        rest = &after[len + 1..];
    }
}

fn look_from(block: &str) -> Result<Look, String> {
    let value: Value = serde_json::from_str(block).map_err(|e| format!("a look isn't valid JSON ({e})"))?;
    let object = value.as_object().ok_or("a look must be a JSON object")?;
    for key in object.keys() {
        if !["name", "suits", "note", "effect"].contains(&key.as_str()) {
            return Err(format!(
                "a look has no \"{key}\": it takes name, suits, note, effect"
            ));
        }
    }
    let text = |key: &str| {
        object
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .ok_or(format!("a look needs \"{key}\""))
    };
    let name = text("name")?;
    let suits = object
        .get("suits")
        .and_then(Value::as_array)
        .filter(|s| !s.is_empty())
        .ok_or(format!(
            "the look \"{name}\" needs \"suits\": some of {}",
            Suit::words().join(", ")
        ))?
        .iter()
        .map(|s| {
            s.as_str().and_then(Suit::parse).ok_or(format!(
                "the look \"{name}\" suits {s}, which isn't one of {}",
                Suit::words().join(", ")
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let effect = object
        .get("effect")
        .filter(|e| e.is_object())
        .cloned()
        .ok_or(format!("the look \"{name}\" needs an \"effect\" object"))?;
    Ok(Look {
        note: text("note")
            .map_err(|_| format!("the look \"{name}\" needs a \"note\" saying what it looks like"))?,
        name,
        suits,
        effect,
    })
}

/// Reads a guide. `Err` says what's wrong with it, in words a writer can act on.
pub fn parse(source: &str) -> Result<Guide, String> {
    let mut lines = source.lines();
    let title = lines
        .next()
        .and_then(|l| l.strip_prefix("# "))
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or("a guide starts with its title: \"# Name\"")?
        .to_string();
    let mut guide = Guide {
        text: format!("# {title}\n"),
        title,
        summary: String::new(),
        sections: Vec::new(),
        looks: Vec::new(),
        names: Vec::new(),
    };
    let mut look: Option<String> = None;
    let mut fenced = false;
    let mut summary_done = false;
    for line in lines {
        if let Some(block) = &mut look {
            if line.trim() == "```" {
                let found = look_from(block)?;
                let suits: Vec<&str> = found.suits.iter().map(|s| s.word()).collect();
                guide.text.push_str(&format!(
                    "- **{}** ({}): {}\n  `{}`\n",
                    found.name,
                    suits.join(", "),
                    found.note,
                    found.effect
                ));
                guide.looks.push(found);
                look = None;
            } else {
                block.push_str(line);
                block.push('\n');
            }
            continue;
        }
        if line.trim() == "```look" {
            look = Some(String::new());
            continue;
        }
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        }
        if let Some(section) = line.strip_prefix("## ") {
            guide.sections.push(section.trim().to_string());
            summary_done = true;
        } else if !fenced {
            backticked(line, &mut guide.names);
            if !summary_done {
                if line.trim().is_empty() {
                    summary_done = !guide.summary.is_empty();
                } else {
                    if !guide.summary.is_empty() {
                        guide.summary.push(' ');
                    }
                    guide.summary.push_str(line.trim());
                }
            }
        }
        guide.text.push_str(line);
        guide.text.push('\n');
    }
    if look.is_some() {
        return Err("a look block isn't closed (``` on its own line)".into());
    }
    if guide.summary.is_empty() {
        return Err("a guide needs a sentence under its title saying what it's for".into());
    }
    Ok(guide)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# Bars\n\nStripes that slide: the workhorse\nfor outlines.\n\n## What it draws\n\nOn a string, `count` stripes.\n\n## Looks\n\n```look\n{\"name\": \"Candy stripe\", \"suits\": [\"string\", \"arch\"], \"note\": \"Red and white stripes sliding.\",\n \"effect\": {\"kind\": \"bars\", \"settings\": {\"count\": 6}, \"colors\": [\"#ff0000\", \"#ffffff\"]}}\n```\n";

    #[test]
    fn a_guide_is_read_into_its_title_summary_sections_names_and_looks() {
        let guide = parse(SAMPLE).unwrap();
        assert_eq!(guide.title, "Bars");
        assert_eq!(guide.summary, "Stripes that slide: the workhorse for outlines.");
        assert_eq!(guide.sections, ["What it draws", "Looks"]);
        assert_eq!(guide.names, ["count"]);
        assert_eq!(guide.looks.len(), 1);
        let look = &guide.looks[0];
        assert_eq!(look.name, "Candy stripe");
        assert_eq!(look.suits, [Suit::String, Suit::Arch]);
        assert_eq!(look.effect["settings"]["count"], 6);
        // The look reads as a line of words and a line to place.
        assert!(
            guide.text.contains(
                "- **Candy stripe** (string, arch): Red and white stripes sliding.\n  `{\"colors\":"
            ),
            "{}",
            guide.text
        );
        assert!(!guide.text.contains("```look"));
        assert_eq!(
            guide.section("What it draws").trim(),
            "On a string, `count` stripes."
        );
        assert_eq!(guide.section("Nothing"), "");
    }

    #[test]
    fn what_is_wrong_with_a_guide_is_said() {
        assert!(parse("Bars\n").unwrap_err().contains("title"));
        assert!(parse("# Bars\n\n## Looks\n").unwrap_err().contains("sentence"));
        let bad = |look: &str| parse(&format!("# B\n\nS.\n\n```look\n{look}\n```\n")).unwrap_err();
        assert!(bad("{").contains("valid JSON"));
        assert!(
            bad(r#"{"name": "x", "suits": ["roof"], "note": "n", "effect": {}}"#).contains("string, arch")
        );
        assert!(bad(r#"{"name": "x", "suits": ["tree"], "effect": {}}"#).contains("note"));
        assert!(
            bad(r#"{"name": "x", "suits": ["tree"], "note": "n", "seconds": 3, "effect": {}}"#)
                .contains("seconds")
        );
        assert!(
            parse("# B\n\nS.\n\n```look\n{}\n")
                .unwrap_err()
                .contains("closed")
        );
    }
}
