//! The show both sides are compared on: the xLights layout imported as PixelFlow imports it,
//! changed in two ways so the channels PixelFlow exports are the ones xLights rendered.
//!
//! - **Controller-side settings are set aside.** A model's brightness, gamma, and "reverse" on its
//!   controller connection are carried out by the controller; xLights' `.fseq` holds what it
//!   rendered before them. PixelFlow's export applies them (it sends what the lights should
//!   get), so here every port is at full brightness, gamma 1, unreversed. Each prop's own color
//!   order stays: xLights writes its channels in that order too.
//! - **Props with no controller get a stand-in.** xLights renders every model, also those whose
//!   channels are on no controller PixelFlow imports (none at all, an inactive one, or a protocol
//!   it can't send). Each of those gets a stand-in controller over exactly its xLights channels,
//!   so PixelFlow's export places it where xLights did and it can be compared too.

use crate::FidelityError;
use pf_mapping::ChannelMap;
use pf_model::{ControllerId, Show};
use pf_xlights::{ChannelRequest, XController, XOutput};
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// How many wired props had a controller-side setting set aside.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetAside {
    pub brightness: usize,
    pub gamma: usize,
    pub reverse: usize,
}

/// An xLights layout ready to compare on.
#[derive(Debug, Clone)]
pub struct ComparisonShow {
    pub show: Show,
    pub map: ChannelMap,
    /// The controllers that stand in for props PixelFlow has no controller for.
    pub stand_ins: Vec<ControllerId>,
    pub set_aside: SetAside,
    /// The last channel any model uses, counting from 1 (0 when none has channels).
    pub channels: u32,
    /// The layout import's notes.
    pub notes: Vec<String>,
}

fn read(dir: &Path, file: &str) -> Result<String, FidelityError> {
    let path = dir.join(file);
    fs::read_to_string(&path).map_err(|source| FidelityError::Read { path, source })
}

/// The name of the stand-in controller for the model `name`.
fn stand_in_name(name: &str) -> String {
    format!("(stand-in for {name})")
}

/// Imports the xLights show folder `dir` for comparing.
pub fn comparison_show(dir: &Path) -> Result<ComparisonShow, FidelityError> {
    if !dir.join("xlights_rgbeffects.xml").is_file() {
        return Err(FidelityError::NotAShowFolder(dir.to_path_buf()));
    }
    let layout = pf_xlights::parse_layout(&read(dir, "xlights_rgbeffects.xml")?)?;
    let mut controllers = if dir.join("xlights_networks.xml").is_file() {
        pf_xlights::parse_networks(&read(dir, "xlights_networks.xml")?)?
    } else {
        Vec::new()
    };

    // Where xLights puts each model's channels, worked out as the import does.
    let geometries: Vec<_> = layout.models.iter().map(pf_xlights::geometry).collect();
    let requests: Vec<ChannelRequest> = layout
        .models
        .iter()
        .zip(&geometries)
        .map(|(model, g)| ChannelRequest {
            name: &model.name,
            start: if g.absolute_channels {
                "1"
            } else {
                model.text("StartChannel", "1")
            },
            channels: g.channels_unknown.is_none().then_some(g.channels),
            first: g.nodes.first().map_or(0, |n| n.channel),
        })
        .collect();
    let starts = pf_xlights::resolve(&requests, &controllers);
    // The controllers PixelFlow imports; a model that starts on none of them gets a stand-in.
    let imported = |c: &XController| c.active && matches!(c.protocol.as_str(), "DDP" | "E131");
    let mut stand_in_names = Vec::new();
    let mut seen = HashSet::new();
    let mut channels = 0u64;
    let mut stand_ins = Vec::new();
    for ((model, g), start) in layout.models.iter().zip(&geometries).zip(&starts) {
        let Ok(start) = *start else { continue };
        if !seen.insert(model.name.as_str()) || g.nodes.is_empty() || g.channels_unknown.is_some() {
            continue;
        }
        channels = channels.max(u64::from(start) - 1 + u64::from(g.channels));
        if controllers.iter().any(|c| imported(c) && c.contains(start)) {
            continue;
        }
        let name = stand_in_name(&model.name);
        stand_ins.push(XController {
            name: name.clone(),
            ip: String::new(),
            protocol: "DDP".to_string(),
            kind: "Null".to_string(),
            active: true,
            keep_channel_numbers: false,
            defaults: None,
            outputs: vec![XOutput {
                universe: 0,
                start,
                channels: g.channels,
            }],
        });
        stand_in_names.push(name);
    }
    controllers.extend(stand_ins);

    let name = dir.file_name().map_or_else(
        || "xLights Show".to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let imported = pf_xlights::build_show(&name, &controllers, &layout, pf_xlights::geometry);
    let mut show = imported.show;

    let mut set_aside = SetAside::default();
    for port in show.controllers.iter_mut().flat_map(|c| &mut c.ports) {
        for slot in &mut port.slots {
            let brightness = slot.brightness.take().unwrap_or(port.brightness);
            let gamma = slot.gamma.take().unwrap_or(port.gamma);
            set_aside.brightness += usize::from(brightness != 100);
            set_aside.gamma += usize::from(gamma != 1.0);
            set_aside.reverse += usize::from(slot.reverse);
            slot.reverse = false;
        }
        port.brightness = 100;
        port.gamma = 1.0;
    }
    let stand_ins = show
        .controllers
        .iter()
        .filter(|c| stand_in_names.contains(&c.name))
        .map(|c| c.id)
        .collect();
    let (map, _) = pf_mapping::map_show(&show);
    Ok(ComparisonShow {
        show,
        map,
        stand_ins,
        set_aside,
        channels: u32::try_from(channels).unwrap_or(u32::MAX),
        notes: imported.notes,
    })
}
