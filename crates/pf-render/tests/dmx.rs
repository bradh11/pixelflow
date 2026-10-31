//! DMX fixtures in a drawn frame: the colour drawn becomes their channels, and the DMX effect sets
//! channels directly.

use pf_model::{DmxFixture, Generator, Group, Prop, ShapeSource, Show, Transform, Vec3, dmx_layout};
use pf_render::{RenderPool, Renderer};
use pf_sequence::*;

fn fixture(name: &str, layout: &str, cells: u32, x: f32) -> Prop {
    let mut prop = Prop::new(
        name,
        ShapeSource::Generator(Generator::Line {
            nodes: cells,
            length: 1.0,
        }),
    );
    prop.transform = Transform {
        position: Vec3::new(x, 0.0, 0.0),
        ..Transform::default()
    };
    let mut dmx = DmxFixture::from_layout(dmx_layout(layout).unwrap(), cells, 1);
    // The mode channel of the 7-channel layout holds 200.
    if let Some(mode) = dmx
        .channels
        .iter_mut()
        .find(|c| c.role == pf_model::DmxRole::Fixed)
    {
        mode.value = 200;
    }
    prop.dmx = Some(dmx);
    prop
}

/// A 7-channel flood (dimmer, RGBW, strobe, mode) and a 4-cell RGB bar, in a group.
fn show() -> Show {
    let mut show = Show::new("floods");
    show.props
        .push(fixture("Flood", "dimmer-rgbw-strobe-mode", 1, 0.0));
    show.props.push(fixture("Bar", "bar-rgb", 4, 2.0));
    let mut group = Group::new("Floods");
    group.members = vec![show.props[0].id.into(), show.props[1].id.into()];
    show.groups.push(group);
    show
}

fn row(target: Target, layers: Vec<Vec<Effect>>) -> Row {
    let mut row = Row::new(target);
    row.layers = layers.into_iter().map(|effects| Layer { effects }).collect();
    row
}

fn on(color: Rgb) -> Effect {
    Effect::new(EffectKind::On, 0, 1000).with_palette([color])
}

fn dmx(channels: &[(u16, u8)]) -> Effect {
    Effect::new(EffectKind::Dmx, 0, 1000).with_params(EffectParams::Dmx(DmxParams {
        channels: DmxValues::new(channels.iter().copied()),
    }))
}

fn render_with(show: &Show, seq: &Sequence, t_ms: u64, pool: Option<std::sync::Arc<RenderPool>>) -> Vec<u8> {
    let (map, _) = pf_mapping::map_show(show);
    let mut renderer = Renderer::new(show, &map);
    renderer.set_pool(pool);
    let mut frame = vec![0xAA; renderer.frame_len()];
    renderer.render(seq, t_ms, &mut frame);
    frame
}

fn render(show: &Show, seq: &Sequence, t_ms: u64) -> Vec<u8> {
    render_with(show, seq, t_ms, None)
}

/// Where each part of the frame is: the flood's cell (3), the bar's cells (12), then the
/// flood's 7 channels and the bar's 12.
fn flood_channels(frame: &[u8]) -> &[u8] {
    &frame[15..22]
}

fn bar_channels(frame: &[u8]) -> &[u8] {
    &frame[22..34]
}

#[test]
fn a_colour_drawn_on_a_flood_becomes_its_channels() {
    let show = show();
    let mut seq = Sequence::new("s", 1000);
    seq.rows.push(row(
        Target::Prop(show.props[0].id),
        vec![vec![on(Rgb::new(128, 64, 64))]],
    ));
    let frame = render(&show, &seq, 500);
    assert_eq!(frame.len(), 34);
    // Dimmer at the brightest channel, red brought up to full less the white all three share,
    // strobe 0, mode held at 200.
    assert_eq!(flood_channels(&frame), &[128, 127, 0, 0, 128, 0, 200]);
    assert_eq!(&frame[..3], &[128, 64, 64], "its cell shows the colour it makes");
    // Unlit: dark, the mode still held.
    let frame = render(&show, &seq, 1000);
    assert_eq!(flood_channels(&frame), &[0, 0, 0, 0, 0, 0, 200]);
    assert!(bar_channels(&frame).iter().all(|&b| b == 0));
}

#[test]
fn the_dmx_effect_sets_channels_over_the_colour_and_draws_no_colour() {
    let show = show();
    let mut seq = Sequence::new("s", 1000);
    // Red on the flood, and on a layer above it the mode (channel 7) set.
    seq.rows.push(row(
        Target::Prop(show.props[0].id),
        vec![vec![on(Rgb::RED)], vec![dmx(&[(7, 90)])]],
    ));
    let frame = render(&show, &seq, 500);
    assert_eq!(flood_channels(&frame), &[255, 255, 0, 0, 0, 0, 90]);
    assert_eq!(
        &frame[..3],
        &[255, 0, 0],
        "the red still shows: the DMX effect covers nothing"
    );

    // The strobe (channel 6) set fastest: the previews show a flash every other frame, from
    // the frame's moment alone, while the channels stay as set.
    let mut seq = Sequence::new("s", 1000);
    seq.rows.push(row(
        Target::Prop(show.props[0].id),
        vec![vec![on(Rgb::RED)], vec![dmx(&[(6, 255)])]],
    ));
    let flash = render(&show, &seq, 500);
    let between = render(&show, &seq, 525);
    assert_eq!(&flash[..3], &[255, 0, 0], "a flash");
    assert_eq!(&between[..3], &[0, 0, 0], "between flashes");
    assert_eq!(flood_channels(&flash), flood_channels(&between));
    assert_eq!(flood_channels(&flash)[5], 255);

    // Alone on a row, it leaves the colour drawn below it on an earlier row.
    let mut seq = Sequence::new("s", 1000);
    seq.rows
        .push(row(Target::Prop(show.props[0].id), vec![vec![on(Rgb::BLUE)]]));
    seq.rows
        .push(row(Target::Prop(show.props[0].id), vec![vec![dmx(&[(1, 64)])]]));
    let frame = render(&show, &seq, 500);
    assert_eq!(flood_channels(&frame), &[64, 0, 0, 255, 0, 0, 200]);
    assert_eq!(&frame[..3], &[0, 0, 64], "the previews show the dimmer it set");
}

#[test]
fn on_a_group_it_sets_each_fixtures_channel_and_the_later_row_wins() {
    let show = show();
    let group = Target::Group(show.groups[0].id);
    let mut seq = Sequence::new("s", 1000);
    seq.rows.push(row(group, vec![vec![dmx(&[(1, 10), (2, 20)])]]));
    seq.rows
        .push(row(Target::Prop(show.props[1].id), vec![vec![dmx(&[(1, 99)])]]));
    let frame = render(&show, &seq, 500);
    assert_eq!(&flood_channels(&frame)[..2], &[10, 20]);
    assert_eq!(
        &bar_channels(&frame)[..2],
        &[99, 20],
        "the bar's own row came later"
    );
    // A channel past a fixture's last is passed over.
    let mut seq = Sequence::new("s", 1000);
    seq.rows.push(row(group, vec![vec![dmx(&[(12, 77), (13, 1)])]]));
    let frame = render(&show, &seq, 500);
    assert_eq!(bar_channels(&frame)[11], 77);
    assert_eq!(flood_channels(&frame), &[0, 0, 0, 0, 0, 0, 200]);
}

#[test]
fn a_curve_changes_a_channel_over_the_effect() {
    let show = show();
    let mut seq = Sequence::new("s", 1000);
    let mut swell = dmx(&[(1, 0)]);
    swell.curves.insert("channels.1".into(), Curve::ramp(0.0, 255.0));
    seq.rows
        .push(row(Target::Prop(show.props[0].id), vec![vec![swell]]));
    let early = render(&show, &seq, 0)[15];
    let middle = render(&show, &seq, 500)[15];
    let late = render(&show, &seq, 975)[15];
    assert!(early < middle && middle < late, "{early} {middle} {late}");
    assert!((120..=135).contains(&middle), "{middle}");
}

#[test]
fn a_bar_shows_each_cell_and_frames_are_the_same_on_any_number_of_threads() {
    let show = show();
    let mut seq = Sequence::new("s", 1000);
    let mut bars = Effect::new(EffectKind::ColorWash, 0, 1000).with_palette([Rgb::RED, Rgb::BLUE]);
    bars.params = EffectParams::ColorWash(ColorWashParams {
        gradient: Gradient::Horizontal,
        ..ColorWashParams::default()
    });
    seq.rows.push(row(
        Target::Group(show.groups[0].id),
        vec![vec![bars], vec![dmx(&[(6, 1)])]],
    ));
    let one = render(&show, &seq, 400);
    let cells: Vec<&[u8]> = bar_channels(&one).chunks(3).collect();
    assert_ne!(cells[0], cells[3], "the cells of the bar differ along it");
    let pool = RenderPool::new(3).expect("three threads");
    for t in [0, 400, 999] {
        assert_eq!(
            render_with(&show, &seq, t, Some(pool.clone())),
            render(&show, &seq, t),
            "at {t} ms"
        );
    }
}

#[test]
fn the_dmx_effect_moves_a_head_while_the_colour_drawn_lights_it() {
    // A 16-bit head at Home, lit green by an On, its pan swept by a curve and its tilt set.
    let mut show = Show::new("heads");
    show.props.push(fixture("Spot", "head-rgbw-16", 1, 0.0));
    let mut seq = Sequence::new("s", 1000);
    let mut sweep = dmx(&[(3, 200)]);
    sweep.curves.insert("channels.1".into(), Curve::ramp(0.0, 255.0));
    seq.rows.push(row(
        Target::Prop(show.props[0].id),
        vec![vec![sweep], vec![on(Rgb::GREEN)]],
    ));
    // The head's cell (3 bytes), then its 11 channels.
    let channels = |t: u64| render(&show, &seq, t)[3..14].to_vec();
    let start = channels(0);
    let end = channels(975);
    assert_eq!(start[2], 200, "tilt as set");
    assert_eq!(start[1], 0, "pan fine at Home's");
    assert!(
        start[0] < 10 && end[0] > 240,
        "pan swept: {} then {}",
        start[0],
        end[0]
    );
    assert_eq!(
        &start[5..],
        &[255, 0, 0, 255, 0, 0],
        "the dimmer, and green, from the On"
    );
    // With nothing setting them, pan and tilt rest at Home.
    let mut still = Sequence::new("s", 1000);
    still
        .rows
        .push(row(Target::Prop(show.props[0].id), vec![vec![on(Rgb::GREEN)]]));
    assert_eq!(&render(&show, &still, 0)[3..7], &[128, 0, 128, 0]);
}
