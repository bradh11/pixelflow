//! Resolving xLights `StartChannel` expressions to absolute channels.
//!
//! Forms: `N` (absolute), `>Model:N` / `<Model:N` (N channels after Model's last, N=1 = next),
//! `@Model:N` (Model's first channel + N - 1), `#Universe:N`, `#IP:Universe:N`, and
//! `!Controller:N` (channel N of that controller). Models referring to other models are
//! resolved in dependency order; cycles and missing names are reported, not guessed.

use crate::networks::XController;
use std::collections::HashMap;

/// What a model needs for start-channel resolution.
pub struct ChannelRequest<'a> {
    pub name: &'a str,
    pub start: &'a str,
    /// The model's channel count (its block length).
    pub channels: u32,
}

/// The resolved absolute start channel (1-based), or why it couldn't be resolved.
pub type Resolved = Result<u32, String>;

fn split_ref(rest: &str) -> Option<(&str, u32)> {
    let (name, n) = rest.rsplit_once(':')?;
    let n = n.trim().parse::<i64>().ok()?;
    Some((name.trim(), u32::try_from(n.max(0)).unwrap_or(0)))
}

/// Resolves every model's start channel. The result is in the same order as `models`.
pub fn resolve(models: &[ChannelRequest], controllers: &[XController]) -> Vec<Resolved> {
    let index: HashMap<&str, usize> = models.iter().enumerate().map(|(i, m)| (m.name, i)).collect();
    let mut resolved: Vec<Option<Resolved>> = vec![None; models.len()];
    // Absolute, controller, and universe forms first; then model references until nothing changes.
    loop {
        let mut progressed = false;
        for (i, model) in models.iter().enumerate() {
            if resolved[i].is_some() {
                continue;
            }
            let text = model.start.trim();
            let result = match text.chars().next() {
                Some('>' | '<' | '@') => {
                    let Some((target, n)) = split_ref(&text[1..]) else {
                        resolved[i] = Some(Err(format!("its start channel \"{text}\" isn't valid")));
                        progressed = true;
                        continue;
                    };
                    let Some(&j) = index.get(target) else {
                        resolved[i] = Some(Err(format!(
                            "its start channel refers to \"{target}\", which isn't in the layout"
                        )));
                        progressed = true;
                        continue;
                    };
                    match &resolved[j] {
                        None => continue, // not ready yet
                        Some(Err(_)) => {
                            Err(format!("it starts after \"{target}\", which couldn't be placed"))
                        }
                        Some(Ok(first)) => {
                            if text.starts_with('@') {
                                if n == 0 {
                                    Err(format!("its start channel \"{text}\" isn't valid"))
                                } else {
                                    Ok(first + n - 1)
                                }
                            } else {
                                // last 0-based channel of the target + N + 1, as a 1-based channel
                                let last0 = u64::from(*first) - 1 + u64::from(models[j].channels.max(1)) - 1;
                                u32::try_from(last0 + u64::from(n) + 1)
                                    .map_err(|_| "its channels are too high".into())
                            }
                        }
                    }
                }
                Some('!') => match split_ref(&text[1..]) {
                    Some((name, n)) if n >= 1 => match controllers.iter().find(|c| c.name == name) {
                        Some(c) => Ok(c.start() + n - 1),
                        None => Err(format!("its controller \"{name}\" isn't in xlights_networks.xml")),
                    },
                    _ => Err(format!("its start channel \"{text}\" isn't valid")),
                },
                Some('#') => {
                    let parts: Vec<&str> = text[1..].split(':').map(str::trim).collect();
                    let (ip, universe, n) = match parts.as_slice() {
                        [u, n] => (None, *u, *n),
                        [ip, u, n] => (Some(*ip), *u, *n),
                        _ => (None, "", ""),
                    };
                    match (universe.parse::<u32>(), n.parse::<u32>()) {
                        (Ok(universe), Ok(n)) if n >= 1 => controllers
                            .iter()
                            .filter(|c| ip.is_none_or(|ip| c.ip == ip))
                            .flat_map(|c| &c.outputs)
                            .find(|o| o.universe == universe && universe > 0)
                            .map(|o| o.start + n - 1)
                            .ok_or_else(|| format!("no controller has universe {universe}")),
                        _ => Err(format!("its start channel \"{text}\" isn't valid")),
                    }
                }
                _ => {
                    // "Output:N" legacy form: xLights uses the part after the colon.
                    let number = text.rsplit(':').next().unwrap_or(text).trim();
                    match number.parse::<i64>() {
                        Ok(n) if n >= 1 => Ok(u32::try_from(n).unwrap_or(u32::MAX)),
                        _ => Err(format!("its start channel \"{text}\" isn't valid")),
                    }
                }
            };
            resolved[i] = Some(result);
            progressed = true;
        }
        if !progressed {
            break;
        }
    }
    resolved
        .into_iter()
        .zip(models)
        .map(|(r, m)| {
            r.unwrap_or_else(|| {
                Err(format!(
                    "its start channel \"{}\" depends on itself",
                    m.start.trim()
                ))
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::networks::XOutput;

    fn controllers() -> Vec<XController> {
        let c = |name: &str, ip: &str, outputs: Vec<XOutput>| XController {
            name: name.into(),
            ip: ip.into(),
            protocol: "E131".into(),
            kind: "Ethernet".into(),
            active: true,
            keep_channel_numbers: false,
            outputs,
        };
        vec![
            c(
                "Falcon",
                "192.0.2.20",
                vec![XOutput {
                    universe: 0,
                    start: 1,
                    channels: 6147,
                }],
            ),
            c(
                "Arches",
                "192.0.2.30",
                vec![
                    XOutput {
                        universe: 10,
                        start: 6148,
                        channels: 512,
                    },
                    XOutput {
                        universe: 11,
                        start: 6660,
                        channels: 512,
                    },
                ],
            ),
        ]
    }

    fn req<'a>(name: &'a str, start: &'a str, channels: u32) -> ChannelRequest<'a> {
        ChannelRequest {
            name,
            start,
            channels,
        }
    }

    #[test]
    fn every_form_resolves() {
        let models = [
            req("Tree", "!Falcon:1", 300),
            req("Arch 1", ">Tree:1", 150),
            req("Arch 2", "@Arch 1:10", 30),
            req("Star", "#11:5", 30),
            req("Matrix", "#192.0.2.30:10:1", 30),
            req("Plain", "7000", 3),
            req("Legacy", "2:100", 3),
        ];
        let got = resolve(&models, &controllers());
        assert_eq!(
            got,
            vec![Ok(1), Ok(301), Ok(310), Ok(6664), Ok(6148), Ok(7000), Ok(100)]
        );
    }

    #[test]
    fn references_resolve_in_any_file_order() {
        let models = [req("B", ">A:1", 3), req("A", ">Root:1", 6), req("Root", "1", 9)];
        assert_eq!(resolve(&models, &[]), vec![Ok(16), Ok(10), Ok(1)]);
    }

    #[test]
    fn problems_are_explained() {
        let models = [
            req("Loop1", ">Loop2:1", 3),
            req("Loop2", ">Loop1:1", 3),
            req("Lost", ">Nobody:1", 3),
            req("NoCtl", "!Ghost:1", 3),
            req("NoUni", "#99:1", 3),
            req("Junk", "abc", 3),
        ];
        let got = resolve(&models, &controllers());
        assert!(got[0].as_ref().unwrap_err().contains("depends on itself"));
        assert!(got[1].as_ref().unwrap_err().contains("depends on itself"));
        assert!(got[2].as_ref().unwrap_err().contains("\"Nobody\""));
        assert!(got[3].as_ref().unwrap_err().contains("\"Ghost\""));
        assert!(got[4].as_ref().unwrap_err().contains("universe 99"));
        assert!(got[5].as_ref().unwrap_err().contains("\"abc\""));
    }
}
