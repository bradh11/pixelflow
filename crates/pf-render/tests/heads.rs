//! The Moving Head and Servo effects: where they point heads and turn servos, how they light a
//! head, how they fan across a group, and how they meet the DMX effect.

use pf_model::{
    DmxFixture, DmxMotor, DmxRole, Generator, Group, Prop, ShapeSource, Show, Transform, Vec3, dmx_layout,
};
use pf_render::Renderer;
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
    prop.dmx = Some(DmxFixture::from_layout(dmx_layout(layout).unwrap(), cells, 1));
    prop
}

/// Three 16-bit RGBW heads (11 channels each: pan, pan fine, tilt, tilt fine, speed, dimmer,
/// strobe, red, green, blue, white) and a fixture of two 16-bit servos, the heads in a group.
fn show() -> Show {
    let mut show = Show::new("heads");
    for (i, name) in ["Left", "Middle", "Right"].into_iter().enumerate() {
        show.props.push(fixture(name, "head-rgbw-16", 1, i as f32 * 2.0));
    }
    show.props.push(fixture("Arms", "servo-16", 2, 8.0));
    let mut group = Group::new("Heads");
    group.members = show.props[..3].iter().map(|p| p.id.into()).collect();
    show.groups.push(group);
    show
}

fn row(target: Target, layers: Vec<Vec<Effect>>) -> Row {
    let mut row = Row::new(target);
    row.layers = layers.into_iter().map(|effects| Layer { effects }).collect();
    row
}

fn head(set: impl FnOnce(&mut MovingHeadParams)) -> Effect {
    let mut params = MovingHeadParams::default();
    set(&mut params);
    Effect::new(EffectKind::MovingHead, 0, 4000)
        .with_params(EffectParams::MovingHead(params))
        .with_palette([Rgb::RED])
}

fn servo(set: impl FnOnce(&mut ServoParams)) -> Effect {
    let mut params = ServoParams::default();
    set(&mut params);
    Effect::new(EffectKind::Servo, 0, 4000).with_params(EffectParams::Servo(params))
}

fn render(show: &Show, seq: &Sequence, t_ms: u64) -> Vec<u8> {
    let (map, _) = pf_mapping::map_show(show);
    let mut renderer = Renderer::new(show, &map);
    let mut frame = vec![0xAA; renderer.frame_len()];
    renderer.render(seq, t_ms, &mut frame);
    frame
}

/// The cells come first (three heads' one cell each, then the servos' two), then each
/// fixture's channels: 11 per head, then the servos' 4.
fn head_channels(frame: &[u8], head: usize) -> &[u8] {
    let at = 15 + head * 11;
    &frame[at..at + 11]
}

fn servo_channels(frame: &[u8]) -> &[u8] {
    &frame[48..52]
}

/// Where a head points, in degrees from Home: its pan and tilt from their channels.
fn pointing(frame: &[u8], head: usize) -> (f64, f64) {
    let c = head_channels(frame, head);
    let pan = DmxMotor::default_for(DmxRole::Pan).degrees(c[0], Some(c[1]));
    let tilt = DmxMotor::default_for(DmxRole::Tilt).degrees(c[2], Some(c[3]));
    (pan, tilt)
}

fn near(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 0.05 && (a.1 - b.1).abs() < 0.05
}

#[test]
fn it_points_a_head_and_lights_it_in_the_first_palette_color() {
    let show = show();
    let mut seq = Sequence::new("s", 4000);
    seq.rows.push(row(
        Target::Prop(show.props[0].id),
        vec![vec![head(|p| {
            p.pan = 90.0;
            p.tilt = 45.0;
            p.dimmer = 50.0;
        })]],
    ));
    let frame = render(&show, &seq, 1000);
    let c = head_channels(&frame, 0);
    assert_eq!(&c[..4], &[170, 170, 170, 170], "pan +90 of 540, tilt +45 of 270");
    assert_eq!(c[4], 0, "speed held");
    // Red at half: the dimmer carries the brightness, red is full.
    assert_eq!(&c[5..], &[128, 0, 255, 0, 0, 0]);
    assert_eq!(&frame[..3], &[128, 0, 0], "its cell shows the light it makes");
    // The other heads aren't on its row: at Home and dark.
    assert_eq!(&head_channels(&frame, 1)[..6], &[128, 0, 128, 0, 0, 0]);
}

#[test]
fn limits_keep_a_head_where_it_may_point() {
    let mut show = show();
    let dmx = show.props[0].dmx.as_mut().unwrap();
    let tilt = dmx.channels.iter_mut().find(|c| c.role == DmxRole::Tilt).unwrap();
    tilt.motor = Some(DmxMotor {
        min_deg: Some(-30.0),
        max_deg: Some(30.0),
        ..DmxMotor::default_for(DmxRole::Tilt)
    });
    let mut seq = Sequence::new("s", 4000);
    seq.rows.push(row(
        Target::Prop(show.props[0].id),
        vec![vec![head(|p| p.tilt = 120.0)]],
    ));
    let (_, tilt) = pointing(&render(&show, &seq, 0), 0);
    assert!((tilt - 30.0).abs() < 0.01, "{tilt}");
}

#[test]
fn a_group_fans_out_about_its_middle_head_and_ripples_with_a_delay() {
    let show = show();
    let group = Target::Group(show.groups[0].id);
    let mut seq = Sequence::new("s", 4000);
    seq.rows.push(row(
        group,
        vec![vec![head(|p| {
            p.tilt = 30.0;
            p.fan_pan = 40.0;
        })]],
    ));
    let frame = render(&show, &seq, 500);
    assert!(near(pointing(&frame, 0), (-40.0, 30.0)));
    assert!(near(pointing(&frame, 1), (0.0, 30.0)));
    assert!(near(pointing(&frame, 2), (40.0, 30.0)));
    // Mirrored: the right half points the other way across.
    let mut seq = Sequence::new("s", 4000);
    seq.rows.push(row(
        group,
        vec![vec![head(|p| {
            p.pan = 30.0;
            p.mirror = true;
        })]],
    ));
    let frame = render(&show, &seq, 500);
    assert!(near(pointing(&frame, 0), (30.0, 0.0)));
    assert!(
        near(pointing(&frame, 1), (30.0, 0.0)),
        "the middle head is in the left half"
    );
    assert!(near(pointing(&frame, 2), (-30.0, 0.0)));
    // A sweep with a delay: each head is where the one before it was a moment ago.
    let mut seq = Sequence::new("s", 4000);
    seq.rows.push(row(
        group,
        vec![vec![head(|p| {
            p.pattern = HeadPattern::Sweep;
            p.width = 100.0;
            p.height = 0.0;
            p.speed = 0.5;
            p.delay = 0.25;
        })]],
    ));
    let later = render(&show, &seq, 1500);
    let earlier = render(&show, &seq, 1250);
    assert!(near(pointing(&later, 1), pointing(&earlier, 0)));
    assert!(near(pointing(&later, 2), pointing(&render(&show, &seq, 1000), 0)));
    // A second and a half in, the first head is three quarters round: at its left end.
    assert!(near(
        pointing(&later, 0),
        (50.0 * (0.75 * 360f64).to_radians().sin(), 0.0)
    ));
    // A head running behind waits at the start.
    assert!(near(pointing(&render(&show, &seq, 100), 2), (0.0, 0.0)));
}

#[test]
fn each_frame_comes_from_its_moment_alone() {
    // Scrubbing to a moment and playing up to it agree, whatever was drawn before.
    let show = show();
    let group = Target::Group(show.groups[0].id);
    let mut seq = Sequence::new("s", 4000);
    let mut circling = head(|p| {
        p.pattern = HeadPattern::Circle;
        p.speed = 0.7;
        p.delay = 0.1;
        p.color = HeadColor::Wash;
    });
    circling.palette = Palette::new([Rgb::RED, Rgb::BLUE]);
    circling.curves.insert("speed".into(), Curve::ramp(0.2, 2.0));
    seq.rows.push(row(group, vec![vec![circling]]));
    let (map, _) = pf_mapping::map_show(&show);
    let mut playing = Renderer::new(&show, &map);
    let mut frame = vec![0; playing.frame_len()];
    for t in (0..=2000).step_by(25) {
        playing.render(&seq, t, &mut frame);
    }
    assert_eq!(frame, render(&show, &seq, 2000));
}

#[test]
fn a_speed_curve_speeds_the_movement_up_smoothly() {
    let show = show();
    let mut seq = Sequence::new("s", 4000);
    let mut sweep = head(|p| {
        p.pattern = HeadPattern::Circle;
        p.width = 90.0;
        p.height = 90.0;
    });
    // From 0 to 1 a second over four seconds: 2 times round by the end, 0.5 by half way.
    sweep.curves.insert("speed".into(), Curve::ramp(0.0, 1.0));
    seq.rows
        .push(row(Target::Prop(show.props[0].id), vec![vec![sweep]]));
    // At 2 s it has gone half way round (0.5 cycles): at the bottom of the circle.
    assert!(near(pointing(&render(&show, &seq, 2000), 0), (0.0, -45.0)));
    // Never a jump between frames (at its fastest it turns 11 degrees in 40 ms).
    let mut last = pointing(&render(&show, &seq, 0), 0);
    for t in (40..4000).step_by(40) {
        let now = pointing(&render(&show, &seq, t), 0);
        assert!(
            (now.0 - last.0).abs() < 12.0 && (now.1 - last.1).abs() < 12.0,
            "{t}: {last:?} to {now:?}"
        );
        last = now;
    }
}

#[test]
fn colors_per_head_strobe_fades_and_drawn() {
    let show = show();
    let group = Target::Group(show.groups[0].id);
    let mut seq = Sequence::new("s", 4000);
    let mut each = head(|p| p.color = HeadColor::PerHead);
    each.palette = Palette::new([Rgb::RED, Rgb::GREEN]);
    seq.rows.push(row(group, vec![vec![each]]));
    let frame = render(&show, &seq, 1000);
    assert_eq!(&head_channels(&frame, 0)[7..], &[255, 0, 0, 0]);
    assert_eq!(&head_channels(&frame, 1)[7..], &[0, 255, 0, 0]);
    assert_eq!(&head_channels(&frame, 2)[7..], &[255, 0, 0, 0]);
    // A strobe: lit for the first half of each flash, dark for the second.
    let mut seq = Sequence::new("s", 4000);
    seq.rows.push(row(group, vec![vec![head(|p| p.strobe = 2.0)]]));
    assert_eq!(head_channels(&render(&show, &seq, 100), 0)[5], 255);
    assert_eq!(head_channels(&render(&show, &seq, 300), 0)[5], 0);
    // A fade dims the light and leaves the position.
    let mut faded = head(|p| p.pan = 90.0);
    faded.fade_in_ms = 2000;
    let mut seq = Sequence::new("s", 4000);
    seq.rows.push(row(group, vec![vec![faded]]));
    let frame = render(&show, &seq, 1000);
    assert_eq!(head_channels(&frame, 0)[5], 128);
    assert!(near(pointing(&frame, 0), (90.0, 0.0)));
    // Color from the layers below: the head moves, the On below lights it.
    let mut seq = Sequence::new("s", 4000);
    let on = Effect::new(EffectKind::On, 0, 4000).with_palette([Rgb::BLUE]);
    seq.rows.push(row(
        group,
        vec![
            vec![on],
            vec![head(|p| {
                p.color = HeadColor::Drawn;
                p.tilt = 45.0;
            })],
        ],
    ));
    let frame = render(&show, &seq, 1000);
    assert_eq!(&head_channels(&frame, 0)[5..], &[255, 0, 0, 0, 255, 0]);
    assert!(near(pointing(&frame, 0), (0.0, 45.0)));
}

#[test]
fn the_later_row_or_higher_layer_wins_a_channel_with_the_dmx_effect() {
    let show = show();
    let spot = Target::Prop(show.props[0].id);
    let dmx = |channels: &[(u16, u8)]| {
        Effect::new(EffectKind::Dmx, 0, 4000).with_params(EffectParams::Dmx(DmxParams {
            channels: DmxValues::new(channels.iter().copied()),
        }))
    };
    // A DMX effect above the Moving Head sets the pan; the tilt and light stay the head's.
    let mut seq = Sequence::new("s", 4000);
    seq.rows.push(row(
        spot,
        vec![vec![head(|p| p.tilt = 45.0)], vec![dmx(&[(1, 0), (2, 0)])]],
    ));
    let c = head_channels(&render(&show, &seq, 500), 0).to_vec();
    assert_eq!(&c[..4], &[0, 0, 170, 170]);
    assert_eq!(c[7], 255, "red from the Moving Head");
    // The other way up, the Moving Head wins.
    let mut seq = Sequence::new("s", 4000);
    seq.rows.push(row(
        spot,
        vec![vec![dmx(&[(1, 0), (2, 0)])], vec![head(|p| p.tilt = 45.0)]],
    ));
    assert_eq!(
        &head_channels(&render(&show, &seq, 500), 0)[..4],
        &[128, 0, 170, 170]
    );
    // A later row wins over an earlier one.
    let mut seq = Sequence::new("s", 4000);
    seq.rows.push(row(spot, vec![vec![head(|p| p.pan = 90.0)]]));
    seq.rows.push(row(spot, vec![vec![head(|p| p.pan = -90.0)]]));
    assert!(near(pointing(&render(&show, &seq, 500), 0), (-90.0, 0.0)));
}

#[test]
fn servos_turn_fan_and_step() {
    let show = show();
    let arms = Target::Prop(show.props[3].id);
    let degrees = |frame: &[u8], k: usize| {
        let c = servo_channels(frame);
        DmxMotor::default_for(DmxRole::Servo).degrees(c[k * 2], Some(c[k * 2 + 1]))
    };
    let mut seq = Sequence::new("s", 4000);
    seq.rows.push(row(
        arms,
        vec![vec![servo(|p| {
            p.position = 30.0;
            p.fan = 20.0;
        })]],
    ));
    let frame = render(&show, &seq, 500);
    assert!((degrees(&frame, 0) - 20.0).abs() < 0.01);
    assert!((degrees(&frame, 1) - 40.0).abs() < 0.01);
    // Only the second servo: the first rests at Home.
    let mut seq = Sequence::new("s", 4000);
    seq.rows.push(row(
        arms,
        vec![vec![servo(|p| {
            p.position = -45.0;
            p.servo = 2;
        })]],
    ));
    let frame = render(&show, &seq, 500);
    assert_eq!(&servo_channels(&frame)[..2], &[128, 0]);
    assert!((degrees(&frame, 1) + 45.0).abs() < 0.01);
    // Steps: four places across 90 degrees, a quarter of a cycle each.
    let mut seq = Sequence::new("s", 4000);
    seq.rows.push(row(
        arms,
        vec![vec![servo(|p| {
            p.pattern = ServoPattern::Steps;
            p.size = 90.0;
            p.speed = 1.0;
            p.servo = 1;
        })]],
    ));
    let at: Vec<i64> = [0, 300, 600, 900]
        .iter()
        .map(|&t| degrees(&render(&show, &seq, t), 0).round() as i64)
        .collect();
    assert_eq!(at, [-45, -15, 15, 45]);
    // A curve on which servo moves: the first, then the second.
    let mut handing = servo(|p| p.position = 45.0);
    handing.curves.insert("servo".into(), Curve::ramp(1.0, 2.0));
    let mut seq = Sequence::new("s", 4000);
    seq.rows.push(row(arms, vec![vec![handing]]));
    let early = render(&show, &seq, 500);
    assert!((degrees(&early, 0) - 45.0).abs() < 0.01);
    assert_eq!(&servo_channels(&early)[2..], &[128, 0]);
    let late = render(&show, &seq, 3500);
    assert_eq!(&servo_channels(&late)[..2], &[128, 0]);
    assert!((degrees(&late, 1) - 45.0).abs() < 0.01);
    // A position curve turns it over the effect; the servos make no light.
    let mut turning = servo(|_| {});
    turning.curves.insert("position".into(), Curve::ramp(-90.0, 90.0));
    let mut seq = Sequence::new("s", 4000);
    seq.rows.push(row(arms, vec![vec![turning]]));
    let frame = render(&show, &seq, 3000);
    assert!((degrees(&frame, 0) - 45.0).abs() < 0.1, "{}", degrees(&frame, 0));
    assert_eq!(&frame[9..15], &[0; 6]);
}

#[test]
fn it_does_nothing_on_props_without_motors() {
    let mut show = show();
    show.props.push(fixture("Flood", "dimmer-rgb", 1, 12.0));
    let mut seq = Sequence::new("s", 4000);
    seq.rows.push(row(
        Target::Prop(show.props[4].id),
        vec![vec![head(|p| p.pan = 10.0)]],
    ));
    seq.rows.push(row(
        Target::Prop(show.props[4].id),
        vec![vec![servo(|p| p.position = 10.0)]],
    ));
    let frame = render(&show, &seq, 500);
    // The flood's cell and its four channels stay dark.
    assert_eq!(&frame[15..18], &[0, 0, 0]);
    assert_eq!(&frame[frame.len() - 4..], &[0, 0, 0, 0]);
}
