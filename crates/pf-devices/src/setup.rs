//! A controller's setup side by side with the show's: the rows "Compare with this device" and
//! "Send setup to this device…" show, and taking a device's differences into the show.
//!
//! Strings are matched by position: string 2 on port 3 of the device is string 2 on port 3 of
//! the show's controller. Only what both sides describe is compared: ports, strings, pixel counts,
//! the color order the controller applies, where each string starts (when the device says), and
//! what the controller receives. Settings only the controller keeps (its own null pixels,
//! brightness, gamma, direction) are left alone.

use crate::config::{DeviceConfig, DeviceInput, with_commas};
use crate::device::DeviceKind;
use crate::import::unique;
use pf_model::{
    ColorOrder, Controller, Generator, Port, PortSlot, Prop, PropId, Protocol, SacnConfig, ShapeSource, Show,
    UniverseSize, Vec3,
};
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};

/// One string as a controller has it, or as the show wants it.
#[derive(Debug, Clone, PartialEq)]
pub struct SetupString {
    pub name: String,
    /// Pixels that take data (the show's null pixels count: PixelFlow sends them dark).
    pub pixels: u32,
    /// The color order the controller applies; `None` when the show doesn't say.
    pub color_order: Option<ColorOrder>,
    /// Where the string starts, counted as the controller counts it (FPP: channel from 1), when
    /// known.
    pub start: Option<u32>,
    pub channels_per_pixel: u8,
    /// Show side only: which of the port's slots the string carries.
    pub slots: Vec<usize>,
}

/// A port and its strings, in wiring order.
#[derive(Debug, Clone, PartialEq)]
pub struct SetupPort {
    pub number: u16,
    pub strings: Vec<SetupString>,
}

/// What a controller receives.
#[derive(Debug, Clone, PartialEq)]
pub enum SetupInput {
    Ddp,
    Sacn {
        /// `None`: the show lets PixelFlow choose.
        start_universe: Option<u16>,
        universe_size: u16,
    },
    /// Something PixelFlow can't send.
    Other(String),
}

/// A controller's setup, ports sorted by number.
#[derive(Debug, Clone, PartialEq)]
pub struct Setup {
    pub input: SetupInput,
    pub ports: Vec<SetupPort>,
    /// What was left out of the comparison, in plain language.
    pub notes: Vec<String>,
}

impl Setup {
    pub fn port(&self, number: u16) -> Option<&SetupPort> {
        self.ports.iter().find(|p| p.number == number)
    }
}

/// What a row is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeKind {
    Pixels,
    ColorOrder,
    Start,
    StringAdded,
    StringRemoved,
    Receives,
    StartUniverse,
    UniverseSize,
    /// A controller-specific setting (WLED's realtime receive, for example).
    Setting,
}

/// One difference, before → after.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    /// Stable for the same difference: what the user's picks refer to.
    pub id: String,
    /// The port it's on; `None` for the controller as a whole.
    pub port: Option<u16>,
    pub kind: ChangeKind,
    /// The string it's about ("String 2 · Gutter"), or empty.
    pub subject: String,
    /// What changes ("Pixels").
    pub what: String,
    pub before: String,
    pub after: String,
    /// Something this change undoes or turns off, said plainly.
    pub warning: Option<String>,
    /// Taking the device's value into the show (Compare) is possible.
    pub can_take: bool,
    /// Why it can't be taken, when it can't.
    pub why_not: Option<String>,
}

/// Which way a comparison reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Before is the device, after is the show (Send setup).
    ToDevice,
    /// Before is the show, after is the device (Compare, taking the device's values).
    IntoShow,
}

/// The key of string `index` (from 0) on `port`: "port3/string1".
pub fn string_key(port: u16, index: usize) -> String {
    format!("port{port}/string{}", index + 1)
}

const RECEIVER_NOTE: &str =
    "Strings on smart receivers aren't compared yet; check them on the controller itself.";

/// Strings controllers drive one to a port (WLED's outputs), rather than several in a row.
pub fn one_string_per_port(kind: DeviceKind) -> bool {
    kind == DeviceKind::Wled
}

/// The setup the show wants `controller` to have. Strings start where PixelFlow sends them:
/// back to back from channel 1, in the order the controller's ports and slots are listed.
pub fn show_setup(show: &Show, controller: &Controller, one_per_port: bool) -> Setup {
    let mut notes = Vec::new();
    let mut channel: u32 = 1;
    let mut ports: Vec<SetupPort> = Vec::new();
    for port in &controller.ports {
        let mut strings = Vec::new();
        for (i, slot) in port.slots.iter().enumerate() {
            let Some(prop) = show.prop(slot.prop) else {
                continue;
            };
            let nodes = prop.node_count();
            let range = slot.node_range(nodes);
            if !range.fits_within(nodes) {
                continue;
            }
            let cpp = prop.channels_per_pixel();
            let pixels = slot.null_pixels.saturating_add(range.len());
            if pixels == 0 {
                continue;
            }
            let start = channel;
            channel = channel.saturating_add(pixels.saturating_mul(u32::from(cpp)));
            if slot.smart_receiver.is_some() {
                push_once(&mut notes, RECEIVER_NOTE);
                continue;
            }
            strings.push(SetupString {
                name: prop.name.clone(),
                pixels,
                color_order: slot.controller_color_order,
                start: Some(start),
                channels_per_pixel: cpp,
                slots: vec![i],
            });
        }
        if one_per_port && strings.len() > 1 {
            strings = vec![merge(port.number, strings, &mut notes)];
        }
        match ports.iter_mut().find(|p| p.number == port.number) {
            Some(existing) => existing.strings.extend(strings),
            None => ports.push(SetupPort {
                number: port.number,
                strings,
            }),
        }
    }
    ports.sort_by_key(|p| p.number);
    let input = match controller.protocol {
        Protocol::Ddp => SetupInput::Ddp,
        Protocol::Sacn(sacn) => SetupInput::Sacn {
            start_universe: sacn.start_universe,
            universe_size: sacn.universe_size.channels(),
        },
    };
    Setup { input, ports, notes }
}

/// Several props on one output that drives a single string: one string, their pixels added up.
fn merge(port: u16, strings: Vec<SetupString>, notes: &mut Vec<String>) -> SetupString {
    let first = strings[0].clone();
    let orders: HashSet<Option<ColorOrder>> = strings.iter().map(|s| s.color_order).collect();
    let color_order = if orders.len() == 1 {
        first.color_order
    } else {
        notes.push(format!(
            "Port {port}: its props ask for different color orders, so the controller's own is left as it is."
        ));
        None
    };
    SetupString {
        name: strings
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
            .join(" + "),
        pixels: strings.iter().map(|s| s.pixels).fold(0u32, u32::saturating_add),
        color_order,
        start: first.start,
        channels_per_pixel: strings.iter().map(|s| s.channels_per_pixel).max().unwrap_or(3),
        slots: strings.into_iter().flat_map(|s| s.slots).collect(),
    }
}

/// The setup a device reports.
pub fn device_setup(config: &DeviceConfig) -> Setup {
    let mut notes = Vec::new();
    let mut ports: Vec<SetupPort> = config
        .ports
        .iter()
        .map(|port| SetupPort {
            number: port.number,
            strings: port
                .strings
                .iter()
                .filter(|s| {
                    let on_receiver = s.smart_receiver.is_some();
                    if on_receiver {
                        push_once(&mut notes, RECEIVER_NOTE);
                    }
                    !on_receiver
                })
                .enumerate()
                .map(|(i, s)| SetupString {
                    name: s.name.clone().unwrap_or_else(|| format!("String {}", i + 1)),
                    pixels: s.pixels,
                    color_order: Some(s.color_order),
                    start: None,
                    channels_per_pixel: s.color_order.channels_per_pixel(),
                    slots: Vec::new(),
                })
                .collect(),
        })
        .collect();
    ports.sort_by_key(|p| p.number);
    let input = match &config.input {
        DeviceInput::Ddp => SetupInput::Ddp,
        DeviceInput::Sacn {
            start_universe,
            channels_per_universe,
            ..
        } => SetupInput::Sacn {
            start_universe: Some(*start_universe),
            universe_size: *channels_per_universe,
        },
        DeviceInput::Unsupported { description } => SetupInput::Other(description.clone()),
    };
    Setup { input, ports, notes }
}

fn push_once(notes: &mut Vec<String>, note: &str) {
    if !notes.iter().any(|n| n == note) {
        notes.push(note.to_string());
    }
}

fn pixels_text(pixels: u32) -> String {
    let text = with_commas(i64::from(pixels));
    if pixels == 1 {
        format!("{text} pixel")
    } else {
        format!("{text} pixels")
    }
}

fn input_text(input: &SetupInput) -> String {
    match input {
        SetupInput::Ddp => "DDP".to_string(),
        SetupInput::Sacn { .. } => "sACN (E1.31)".to_string(),
        SetupInput::Other(description) => description.clone(),
    }
}

/// The differences between two setups' ports and strings, port by port.
pub fn diff_ports(before: &Setup, after: &Setup, direction: Direction) -> Vec<Change> {
    let mut numbers: Vec<u16> = before
        .ports
        .iter()
        .chain(&after.ports)
        .map(|p| p.number)
        .collect();
    numbers.sort_unstable();
    numbers.dedup();
    let none: Vec<SetupString> = Vec::new();
    let mut changes = Vec::new();
    for number in numbers {
        let b = before.port(number).map_or(&none, |p| &p.strings);
        let a = after.port(number).map_or(&none, |p| &p.strings);
        for i in 0..b.len().max(a.len()) {
            let key = string_key(number, i);
            let (old, new) = (b.get(i), a.get(i));
            // Rows name the string as the show knows it (its prop), when the show has it.
            let show_side = match direction {
                Direction::ToDevice => new,
                Direction::IntoShow => old,
            };
            let name = show_side.or(old).or(new).map_or("", |s| s.name.as_str());
            let subject = if name.is_empty() {
                format!("String {}", i + 1)
            } else {
                format!("String {} · {name}", i + 1)
            };
            let row = |kind, id: String, what: &str, before: String, after: String| Change {
                id,
                port: Some(number),
                kind,
                subject: subject.clone(),
                what: what.to_string(),
                before,
                after,
                warning: None,
                can_take: true,
                why_not: None,
            };
            match (old, new) {
                (Some(old), Some(new)) => {
                    if old.pixels != new.pixels {
                        let mut change = row(
                            ChangeKind::Pixels,
                            format!("{key}/pixels"),
                            "Pixels",
                            with_commas(i64::from(old.pixels)),
                            with_commas(i64::from(new.pixels)),
                        );
                        if new.pixels < old.pixels {
                            let lost = old.pixels - new.pixels;
                            change.warning = Some(match direction {
                                Direction::ToDevice => {
                                    format!(
                                        "{} fewer: the last {} on this string go dark.",
                                        pixels_text(lost),
                                        with_commas(i64::from(lost))
                                    )
                                }
                                Direction::IntoShow => format!(
                                    "{name} gets {} shorter; effects on its last pixels are lost.",
                                    pixels_text(lost)
                                ),
                            });
                        }
                        changes.push(change);
                    }
                    // A show that doesn't set the order agrees with a controller passing colors
                    // straight through.
                    let plain = matches!(new.color_order, Some(ColorOrder::Rgb | ColorOrder::Rgbw));
                    if let Some(order) = new.color_order
                        && old.color_order != Some(order)
                        && !(old.color_order.is_none() && plain)
                    {
                        changes.push(row(
                            ChangeKind::ColorOrder,
                            format!("{key}/colorOrder"),
                            "Color order",
                            old.color_order.map_or("Not set".to_string(), order_name),
                            order_name(order),
                        ));
                    }
                    if let (Some(from), Some(to)) = (old.start, new.start)
                        && from != to
                    {
                        changes.push(row(
                            ChangeKind::Start,
                            format!("{key}/start"),
                            "Starts at channel",
                            with_commas(i64::from(from)),
                            with_commas(i64::from(to)),
                        ));
                    }
                }
                (None, Some(new)) => {
                    let mut change = row(
                        ChangeKind::StringAdded,
                        key.clone(),
                        "String",
                        "None".to_string(),
                        {
                            let order = new
                                .color_order
                                .map(|o| format!(", {}", order_name(o)))
                                .unwrap_or_default();
                            format!("{}{order}", pixels_text(new.pixels))
                        },
                    );
                    change.what = "New string".to_string();
                    changes.push(change);
                }
                (Some(old), None) => {
                    let mut change = row(
                        ChangeKind::StringRemoved,
                        key.clone(),
                        "Removed string",
                        pixels_text(old.pixels),
                        "None".to_string(),
                    );
                    change.warning = Some(match direction {
                        Direction::ToDevice => format!("Its {} go dark.", pixels_text(old.pixels)),
                        Direction::IntoShow => format!("{name} stays in your show, no longer wired here."),
                    });
                    changes.push(change);
                }
                (None, None) => {}
            }
        }
    }
    changes
}

fn order_name(order: ColorOrder) -> String {
    serde_json::to_value(order)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default()
}

/// What the controller receives, show (before) against device (after), for Compare.
fn diff_input(show: &SetupInput, device: &SetupInput) -> Vec<Change> {
    let row = |kind, id: &str, what: &str, before: String, after: String| Change {
        id: id.to_string(),
        port: None,
        kind,
        subject: String::new(),
        what: what.to_string(),
        before,
        after,
        warning: None,
        can_take: true,
        why_not: None,
    };
    let mut changes = Vec::new();
    match (show, device) {
        (SetupInput::Ddp, SetupInput::Ddp) => {}
        (
            SetupInput::Sacn {
                start_universe: a,
                universe_size: size_a,
            },
            SetupInput::Sacn {
                start_universe: b,
                universe_size: size_b,
            },
        ) => {
            if let Some(b) = b
                && a != &Some(*b)
            {
                changes.push(row(
                    ChangeKind::StartUniverse,
                    "input/startUniverse",
                    "First universe",
                    a.map_or("Chosen by PixelFlow".to_string(), |u| u.to_string()),
                    b.to_string(),
                ));
            }
            if size_a != size_b {
                let mut change = row(
                    ChangeKind::UniverseSize,
                    "input/universeSize",
                    "Channels per universe",
                    size_a.to_string(),
                    size_b.to_string(),
                );
                if UniverseSize::new(u32::from(*size_b)).is_none() {
                    change.can_take = false;
                    change.why_not = Some("A universe carries 1 to 512 channels.".to_string());
                }
                changes.push(change);
            }
        }
        (show, device) => {
            let mut change = row(
                ChangeKind::Receives,
                "input/receives",
                "Receives",
                input_text(show),
                input_text(device),
            );
            if let SetupInput::Other(description) = device {
                change.can_take = false;
                change.why_not = Some(format!("PixelFlow can't send {description} yet."));
            }
            changes.push(change);
        }
    }
    changes
}

/// "Compare with this device": what differs between the show's `controller` and the device's
/// configuration, show (before) → device (after), with whether each can be taken into the show.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    pub changes: Vec<Change>,
    pub notes: Vec<String>,
}

pub fn compare(show: &Show, controller: &Controller, kind: DeviceKind, config: &DeviceConfig) -> Comparison {
    let one = one_string_per_port(kind);
    let ours = show_setup(show, controller, one);
    let theirs = device_setup(config);
    let mut changes = diff_input(&ours.input, &theirs.input);
    changes.extend(diff_ports(&ours, &theirs, Direction::IntoShow));
    for change in &mut changes {
        if let Some(reason) = cannot_take(show, controller, &ours, change) {
            change.can_take = false;
            change.why_not = Some(reason);
        }
    }
    let mut notes = ours.notes;
    for note in theirs.notes {
        push_once(&mut notes, &note);
    }
    Comparison { changes, notes }
}

/// The show-side string a row is about.
fn show_string<'a>(ours: &'a Setup, change: &Change) -> Option<&'a SetupString> {
    let port = ours.port(change.port?)?;
    let index: usize = change
        .id
        .split('/')
        .nth(1)?
        .strip_prefix("string")?
        .parse::<usize>()
        .ok()?
        .checked_sub(1)?;
    port.strings.get(index)
}

fn cannot_take(show: &Show, controller: &Controller, ours: &Setup, change: &Change) -> Option<String> {
    if !change.can_take {
        return change.why_not.clone();
    }
    match change.kind {
        ChangeKind::Pixels => {
            let string = show_string(ours, change)?;
            let port = controller.ports.iter().find(|p| Some(p.number) == change.port)?;
            let [slot] = string.slots.as_slice() else {
                return Some(
                    "Several props share this output; change their sizes on the Layout screen.".to_string(),
                );
            };
            let slot = &port.slots[*slot];
            let prop = show.prop(slot.prop)?;
            let line = matches!(prop.shape, ShapeSource::Generator(Generator::Line { .. }));
            if !line || slot.segment.is_some() {
                return Some(format!(
                    "{}'s shape sets its size; change it on the Layout screen.",
                    prop.name
                ));
            }
            let device_pixels: u32 = change.after.replace(',', "").parse().ok()?;
            (device_pixels <= slot.null_pixels).then(|| {
                format!(
                    "{} starts with {} null pixels on this port.",
                    prop.name, slot.null_pixels
                )
            })
        }
        ChangeKind::Start => Some(
            "PixelFlow places strings back to back. Use Send setup to set the controller to match."
                .to_string(),
        ),
        _ => None,
    }
}

/// What taking a device's differences into the show changes: one undo step.
#[derive(Debug, Clone, PartialEq)]
pub struct Taken {
    pub controller: Controller,
    /// Starter props for strings the show didn't have.
    pub new_props: Vec<Prop>,
    /// Props whose size changed.
    pub changed_props: Vec<Prop>,
}

/// Takes the picked differences (by [`Change::id`]) from the device into the show's
/// `controller`. A new string becomes a starter prop, or wires the existing prop `use_props`
/// names for its [`string_key`]. Fails, changing nothing, when a pick isn't (or is no longer) a
/// difference that can be taken.
pub fn take_from_device(
    show: &Show,
    controller: &Controller,
    kind: DeviceKind,
    config: &DeviceConfig,
    picks: &[String],
    use_props: &BTreeMap<String, PropId>,
) -> Result<Taken, String> {
    let comparison = compare(show, controller, kind, config);
    let ours = show_setup(show, controller, one_string_per_port(kind));
    let theirs = device_setup(config);
    let mut picked = Vec::new();
    for id in picks {
        let change = comparison.changes.iter().find(|c| &c.id == id).ok_or_else(|| {
            "The controller or your show changed since you compared them. Compare again.".to_string()
        })?;
        if !change.can_take {
            return Err(change
                .why_not
                .clone()
                .unwrap_or_else(|| "That difference can't be taken into your show.".to_string()));
        }
        picked.push(change);
    }
    let mut out = controller.clone();
    let mut changed_props: Vec<Prop> = Vec::new();
    let mut new_props: Vec<Prop> = Vec::new();
    let mut prop_names: HashSet<String> = show.props.iter().map(|p| p.name.clone()).collect();
    let mut removals: Vec<(u16, Vec<usize>)> = Vec::new();
    for change in &picked {
        let number = change.port;
        let device_string = number.and_then(|n| {
            let index = change
                .id
                .split('/')
                .nth(1)?
                .strip_prefix("string")?
                .parse::<usize>()
                .ok()?;
            theirs.port(n)?.strings.get(index.checked_sub(1)?)
        });
        match change.kind {
            ChangeKind::Pixels => {
                let (Some(string), Some(device)) = (show_string(&ours, change), device_string) else {
                    continue;
                };
                let port = out.ports.iter().find(|p| Some(p.number) == number);
                let Some(slot) = port.and_then(|p| p.slots.get(string.slots[0])) else {
                    continue;
                };
                let Some(prop) = show.prop(slot.prop) else {
                    continue;
                };
                let mut prop = changed_props
                    .iter()
                    .find(|p| p.id == prop.id)
                    .cloned()
                    .unwrap_or_else(|| prop.clone());
                let nodes = device.pixels - slot.null_pixels;
                if let ShapeSource::Generator(Generator::Line { nodes: n, length }) = &mut prop.shape {
                    // Same spacing between pixels, more or fewer of them.
                    *length = (*length * nodes as f32 / (*n).max(1) as f32).max(0.01);
                    *n = nodes;
                }
                changed_props.retain(|p| p.id != prop.id);
                changed_props.push(prop);
            }
            ChangeKind::ColorOrder => {
                let (Some(string), Some(device)) = (show_string(&ours, change), device_string) else {
                    continue;
                };
                if let Some(port) = out.ports.iter_mut().find(|p| Some(p.number) == number) {
                    for &i in &string.slots {
                        if let Some(slot) = port.slots.get_mut(i) {
                            slot.controller_color_order = device.color_order;
                        }
                    }
                }
            }
            ChangeKind::StringAdded => {
                let (Some(number), Some(device)) = (number, device_string) else {
                    continue;
                };
                let prop_id = match use_props.get(&change.id) {
                    Some(id) => {
                        if show.prop(*id).is_none() {
                            return Err(
                                "A prop you picked is no longer in your show. Compare again.".to_string()
                            );
                        }
                        *id
                    }
                    None => {
                        let prop = starter_prop(
                            &out.name,
                            number,
                            &change.id,
                            device,
                            show.props.len() + new_props.len(),
                            &mut prop_names,
                        );
                        let id = prop.id;
                        new_props.push(prop);
                        id
                    }
                };
                let mut slot = PortSlot::new(prop_id);
                slot.controller_color_order = device.color_order;
                port_mut(&mut out, number).slots.push(slot);
            }
            ChangeKind::StringRemoved => {
                if let (Some(number), Some(string)) = (number, show_string(&ours, change)) {
                    removals.push((number, string.slots.clone()));
                }
            }
            ChangeKind::Receives | ChangeKind::StartUniverse | ChangeKind::UniverseSize => {
                take_input(&mut out, &theirs.input, change.kind);
            }
            ChangeKind::Start | ChangeKind::Setting => {}
        }
    }
    for (number, mut slots) in removals {
        slots.sort_unstable_by(|a, b| b.cmp(a));
        if let Some(port) = out.ports.iter_mut().find(|p| p.number == number) {
            for i in slots {
                if i < port.slots.len() {
                    port.slots.remove(i);
                }
            }
        }
    }
    Ok(Taken {
        controller: out,
        new_props,
        changed_props,
    })
}

fn take_input(controller: &mut Controller, device: &SetupInput, kind: ChangeKind) {
    match (device, kind) {
        (SetupInput::Ddp, ChangeKind::Receives) => controller.protocol = Protocol::Ddp,
        (
            SetupInput::Sacn {
                start_universe,
                universe_size,
            },
            _,
        ) => {
            let mut sacn = match controller.protocol {
                Protocol::Sacn(sacn) => sacn,
                Protocol::Ddp => SacnConfig::default(),
            };
            let size = UniverseSize::new(u32::from(*universe_size));
            match kind {
                ChangeKind::StartUniverse => sacn.start_universe = *start_universe,
                ChangeKind::UniverseSize => sacn.universe_size = size.unwrap_or(sacn.universe_size),
                _ => {
                    sacn.start_universe = *start_universe;
                    sacn.universe_size = size.unwrap_or_default();
                }
            }
            controller.protocol = Protocol::Sacn(sacn);
        }
        _ => {}
    }
}

fn port_mut(controller: &mut Controller, number: u16) -> &mut Port {
    let index = match controller.ports.iter().position(|p| p.number == number) {
        Some(index) => index,
        None => {
            let at = controller
                .ports
                .iter()
                .position(|p| p.number > number)
                .unwrap_or(controller.ports.len());
            controller.ports.insert(at, Port::new(number));
            at
        }
    };
    &mut controller.ports[index]
}

/// A starter prop for a device string: a line of its pixels, named after the string.
fn starter_prop(
    controller: &str,
    port: u16,
    key: &str,
    string: &SetupString,
    placed: usize,
    taken: &mut HashSet<String>,
) -> Prop {
    let fallback = format!(
        "{controller} Port {port} {}",
        key.rsplit('/').next().unwrap_or("").replace("string", "String ")
    );
    let base = if string.name.is_empty() || string.name.starts_with("String ") {
        fallback
    } else {
        string.name.clone()
    };
    let mut prop = Prop::new(
        unique(&base, taken),
        ShapeSource::Generator(Generator::Line {
            nodes: string.pixels,
            length: (string.pixels as f32 * 0.05).max(1.0),
        }),
    );
    // The controller reorders colors itself, so PixelFlow sends plain RGB (or RGBW).
    prop.color_order = if string.channels_per_pixel == 4 {
        ColorOrder::Rgbw
    } else {
        ColorOrder::Rgb
    };
    prop.transform.position = Vec3::new(0.0, -(placed as f32) * 0.5, 0.0);
    prop
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{PortConfig, StringConfig};

    fn line(name: &str, nodes: u32) -> Prop {
        Prop::new(
            name,
            ShapeSource::Generator(Generator::Line {
                nodes,
                length: nodes as f32 * 0.1,
            }),
        )
    }

    fn string(name: &str, pixels: u32, order: ColorOrder) -> StringConfig {
        StringConfig {
            name: Some(name.to_string()),
            pixels,
            color_order: order,
            null_pixels: 0,
            reverse: false,
            brightness: 100,
            gamma: 1.0,
            smart_receiver: None,
        }
    }

    /// A show with "Arch" (50) and "Gutter" (100) on port 1, and "Roof" (200, GRB on the
    /// controller) on port 2.
    fn show() -> (Show, Controller) {
        let mut show = Show::new("t");
        let arch = line("Arch", 50);
        let gutter = line("Gutter", 100);
        let roof = line("Roof", 200);
        let mut controller = Controller::new("Garage FPP", "192.0.2.30", Protocol::Ddp);
        let mut one = Port::new(1);
        one.slots = vec![PortSlot::new(arch.id), PortSlot::new(gutter.id)];
        let mut two = Port::new(2);
        let mut slot = PortSlot::new(roof.id);
        slot.controller_color_order = Some(ColorOrder::Grb);
        two.slots = vec![slot];
        controller.ports = vec![one, two];
        show.props = vec![arch, gutter, roof];
        show.controllers.push(controller.clone());
        (show, controller)
    }

    fn device(ports: Vec<PortConfig>) -> DeviceConfig {
        DeviceConfig {
            input: DeviceInput::Ddp,
            ports,
            destinations: vec![],
            notes: vec![],
        }
    }

    fn port(number: u16, strings: Vec<StringConfig>) -> PortConfig {
        PortConfig {
            number,
            strings,
            max_pixels: None,
        }
    }

    fn matching() -> DeviceConfig {
        device(vec![
            port(
                1,
                vec![
                    string("Arch", 50, ColorOrder::Rgb),
                    string("Gutter", 100, ColorOrder::Rgb),
                ],
            ),
            port(2, vec![string("Roof", 200, ColorOrder::Grb)]),
        ])
    }

    #[test]
    fn the_show_setup_places_strings_back_to_back_from_channel_one() {
        let (show, controller) = show();
        let setup = show_setup(&show, &controller, false);
        let starts: Vec<_> = setup
            .ports
            .iter()
            .flat_map(|p| {
                p.strings
                    .iter()
                    .map(|s| (p.number, s.name.as_str(), s.pixels, s.start))
            })
            .collect();
        assert_eq!(
            starts,
            vec![
                (1, "Arch", 50, Some(1)),
                (1, "Gutter", 100, Some(151)),
                (2, "Roof", 200, Some(451)),
            ]
        );
        // One string per output (WLED): the props on a port are added up.
        let merged = show_setup(&show, &controller, true);
        assert_eq!(merged.ports[0].strings.len(), 1);
        assert_eq!(merged.ports[0].strings[0].pixels, 150);
        assert_eq!(merged.ports[0].strings[0].name, "Arch + Gutter");
        assert_eq!(merged.ports[0].strings[0].slots, vec![0, 1]);
    }

    #[test]
    fn null_pixels_count_and_receivers_are_left_out_with_a_note() {
        let (show, mut controller) = show();
        controller.ports[0].slots[0].null_pixels = 2;
        controller.ports[1].slots[0].smart_receiver = Some(1);
        let setup = show_setup(&show, &controller, false);
        assert_eq!(setup.ports[0].strings[0].pixels, 52);
        assert!(setup.ports[1].strings.is_empty());
        assert_eq!(setup.notes, vec![RECEIVER_NOTE.to_string()]);
    }

    #[test]
    fn a_matching_device_has_no_differences() {
        let (show, controller) = show();
        let comparison = compare(&show, &controller, DeviceKind::Fpp, &matching());
        assert_eq!(comparison.changes, vec![]);
    }

    #[test]
    fn differences_read_show_to_device_with_plain_rows() {
        let (show, controller) = show();
        let mut config = matching();
        config.ports[0].strings[1].pixels = 80; // Gutter is shorter on the device
        config.ports[0].strings[0].color_order = ColorOrder::Grb; // Arch's order is set there
        config.ports[1].strings.clear(); // Roof isn't on the device
        config
            .ports
            .push(port(3, vec![string("Porch", 30, ColorOrder::Bgr)]));
        config.input = DeviceInput::Sacn {
            start_universe: 7,
            channels_per_universe: 510,
            universe_count: 2,
        };
        let changes = compare(&show, &controller, DeviceKind::Fpp, &config).changes;
        let rows: Vec<_> = changes
            .iter()
            .map(|c| {
                (
                    c.id.as_str(),
                    c.subject.as_str(),
                    c.what.as_str(),
                    c.before.as_str(),
                    c.after.as_str(),
                    c.can_take,
                )
            })
            .collect();
        assert_eq!(
            rows,
            vec![
                ("input/receives", "", "Receives", "DDP", "sACN (E1.31)", true),
                (
                    "port1/string1/colorOrder",
                    "String 1 · Arch",
                    "Color order",
                    "Not set",
                    "GRB",
                    true
                ),
                (
                    "port1/string2/pixels",
                    "String 2 · Gutter",
                    "Pixels",
                    "100",
                    "80",
                    true
                ),
                (
                    "port2/string1",
                    "String 1 · Roof",
                    "Removed string",
                    "200 pixels",
                    "None",
                    true
                ),
                (
                    "port3/string1",
                    "String 1 · Porch",
                    "New string",
                    "None",
                    "30 pixels, BGR",
                    true
                ),
            ]
        );
        assert_eq!(
            changes[2].warning.as_deref(),
            Some("Gutter gets 20 pixels shorter; effects on its last pixels are lost.")
        );
        assert_eq!(
            changes[3].warning.as_deref(),
            Some("Roof stays in your show, no longer wired here.")
        );
    }

    #[test]
    fn pushing_reads_device_to_show_and_warns_about_what_goes_dark() {
        let (show, controller) = show();
        let mut device = device_setup(&matching());
        device.ports[0].strings[1].pixels = 120;
        device.ports.push(SetupPort {
            number: 4,
            strings: vec![device.ports[1].strings[0].clone()],
        });
        let changes = diff_ports(
            &device,
            &show_setup(&show, &controller, false),
            Direction::ToDevice,
        );
        assert_eq!(changes.len(), 2, "{changes:#?}");
        assert_eq!(
            (changes[0].before.as_str(), changes[0].after.as_str()),
            ("120", "100")
        );
        assert_eq!(
            changes[0].warning.as_deref(),
            Some("20 pixels fewer: the last 20 on this string go dark.")
        );
        assert_eq!(changes[1].kind, ChangeKind::StringRemoved);
        assert_eq!(changes[1].warning.as_deref(), Some("Its 200 pixels go dark."));
        // Colour orders the show doesn't set are left alone.
        let mut unset = show_setup(&show, &controller, false);
        unset.ports[1].strings[0].color_order = None;
        let mut other = device_setup(&matching());
        other.ports[1].strings[0].color_order = Some(ColorOrder::Bgr);
        assert!(diff_ports(&other, &unset, Direction::ToDevice).is_empty());
    }

    #[test]
    fn some_differences_cant_be_taken() {
        let (mut show, controller) = show();
        // Gutter is an arch, so its size comes from its shape.
        show.props[1].shape = ShapeSource::Generator(Generator::arch(100, 2.0, 1.0));
        let mut config = matching();
        config.ports[0].strings[1].pixels = 80;
        config.input = DeviceInput::Unsupported {
            description: "Art-Net".into(),
        };
        let changes = compare(&show, &controller, DeviceKind::Fpp, &config).changes;
        assert!(!changes[0].can_take);
        assert_eq!(
            changes[0].why_not.as_deref(),
            Some("PixelFlow can't send Art-Net yet.")
        );
        assert!(!changes[1].can_take);
        assert_eq!(
            changes[1].why_not.as_deref(),
            Some("Gutter's shape sets its size; change it on the Layout screen.")
        );
        let err = take_from_device(
            &show,
            &controller,
            DeviceKind::Fpp,
            &config,
            &[changes[1].id.clone()],
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert!(err.contains("Layout screen"), "{err}");
    }

    #[test]
    fn taking_picked_differences_changes_only_those() {
        let (show, controller) = show();
        let mut config = matching();
        config.ports[0].strings[1].pixels = 80;
        config.ports[0].strings[0].color_order = ColorOrder::Grb;
        config.ports[1].strings.clear();
        config
            .ports
            .push(port(3, vec![string("Porch", 30, ColorOrder::Bgr)]));
        let picks: Vec<String> = ["port1/string2/pixels", "port2/string1", "port3/string1"]
            .map(String::from)
            .to_vec();
        let taken = take_from_device(
            &show,
            &controller,
            DeviceKind::Fpp,
            &config,
            &picks,
            &BTreeMap::new(),
        )
        .unwrap();
        // Gutter is now 80 pixels, at the same spacing.
        assert_eq!(taken.changed_props.len(), 1);
        assert_eq!(taken.changed_props[0].name, "Gutter");
        assert_eq!(taken.changed_props[0].node_count(), 80);
        let ShapeSource::Generator(Generator::Line { length, .. }) = taken.changed_props[0].shape else {
            panic!("a line");
        };
        assert!((length - 8.0).abs() < 1e-4, "{length}");
        // Arch's color order wasn't picked.
        assert_eq!(taken.controller.ports[0].slots[0].controller_color_order, None);
        // Roof is unwired from port 2; Porch is a new prop on a new port 3.
        assert!(taken.controller.ports[1].slots.is_empty());
        assert_eq!(taken.new_props.len(), 1);
        assert_eq!(taken.new_props[0].name, "Porch");
        assert_eq!(taken.new_props[0].node_count(), 30);
        let three = &taken.controller.ports[2];
        assert_eq!(three.number, 3);
        assert_eq!(three.slots[0].prop, taken.new_props[0].id);
        assert_eq!(three.slots[0].controller_color_order, Some(ColorOrder::Bgr));

        // Afterwards only the difference left alone remains.
        let mut after = show.clone();
        after.props.extend(taken.new_props.iter().cloned());
        for p in &taken.changed_props {
            *after.props.iter_mut().find(|q| q.id == p.id).unwrap() = p.clone();
        }
        let left = compare(&after, &taken.controller, DeviceKind::Fpp, &config).changes;
        assert_eq!(
            left.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            vec!["port1/string1/colorOrder"]
        );
    }

    #[test]
    fn a_new_string_can_wire_an_existing_prop() {
        let (mut show, controller) = show();
        let star = line("Star", 30);
        let star_id = star.id;
        show.props.push(star);
        let mut config = matching();
        config
            .ports
            .push(port(3, vec![string("Porch", 30, ColorOrder::Rgb)]));
        let use_props = BTreeMap::from([("port3/string1".to_string(), star_id)]);
        let taken = take_from_device(
            &show,
            &controller,
            DeviceKind::Fpp,
            &config,
            &["port3/string1".to_string()],
            &use_props,
        )
        .unwrap();
        assert!(taken.new_props.is_empty());
        assert_eq!(taken.controller.ports[2].slots[0].prop, star_id);
    }

    #[test]
    fn stale_picks_change_nothing() {
        let (show, controller) = show();
        let err = take_from_device(
            &show,
            &controller,
            DeviceKind::Fpp,
            &matching(),
            &["port1/string2/pixels".to_string()],
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert!(err.contains("changed since you compared"), "{err}");
    }

    #[test]
    fn universes_are_taken_from_the_device() {
        let (show, mut controller) = show();
        controller.protocol = Protocol::Sacn(SacnConfig {
            start_universe: Some(1),
            multicast: true,
            ..SacnConfig::default()
        });
        let mut config = matching();
        config.input = DeviceInput::Sacn {
            start_universe: 7,
            channels_per_universe: 512,
            universe_count: 2,
        };
        let changes = compare(&show, &controller, DeviceKind::Fpp, &config).changes;
        let ids: Vec<_> = changes.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["input/startUniverse", "input/universeSize"]);
        let taken = take_from_device(
            &show,
            &controller,
            DeviceKind::Fpp,
            &config,
            &["input/startUniverse".to_string()],
            &BTreeMap::new(),
        )
        .unwrap();
        let Protocol::Sacn(sacn) = taken.controller.protocol else {
            panic!("sACN");
        };
        assert_eq!(sacn.start_universe, Some(7));
        assert_eq!(sacn.universe_size, UniverseSize::CHANNELS_510, "not picked");
        assert!(sacn.multicast, "kept");
    }
}
