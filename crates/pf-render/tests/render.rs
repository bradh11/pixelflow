//! Rendering whole sequences into the show frame.

use pf_model::{ColorOrder, Generator, Group, Prop, ShapeSource, Show, Transform, Vec3};
use pf_render::Renderer;
use pf_sequence::*;
use std::time::Instant;

fn line(name: &str, nodes: u32, x: f32) -> Prop {
    let mut prop = Prop::new(
        name,
        ShapeSource::Generator(Generator::Line { nodes, length: 1.0 }),
    );
    prop.transform = Transform {
        position: Vec3::new(x, 0.0, 0.0),
        ..Transform::default()
    };
    prop
}

/// Two 4-pixel strips side by side (A is RGB, B is RGBW), in a group.
fn show() -> Show {
    let mut show = Show::new("t");
    show.props.push(line("A", 4, 0.0));
    let mut b = line("B", 4, 2.0);
    b.color_order = ColorOrder::Grbw;
    show.props.push(b);
    let mut group = Group::new("Both");
    group.members = vec![show.props[0].id.into(), show.props[1].id.into()];
    show.groups.push(group);
    show
}

fn renderer(show: &Show) -> Renderer {
    Renderer::new(show, &pf_mapping::map_show(show).0)
}

fn render(show: &Show, seq: &Sequence, t_ms: u64) -> Vec<u8> {
    let mut r = renderer(show);
    let mut frame = vec![0xAA; r.frame_len()];
    r.render(seq, t_ms, &mut frame);
    frame
}

fn row(target: Target, layers: Vec<Vec<Effect>>) -> Row {
    let mut row = Row::new(target);
    row.layers = layers.into_iter().map(|effects| Layer { effects }).collect();
    row
}

fn on(color: Rgb, start: u64, end: u64) -> Effect {
    Effect::new(EffectKind::On, start, end).with_palette([color])
}

/// Prop A's four pixels (RGB) and prop B's four (RGBW) from a frame.
fn pixels(frame: &[u8]) -> (Vec<[u8; 3]>, Vec<[u8; 4]>) {
    let a = frame[..12].chunks(3).map(|p| [p[0], p[1], p[2]]).collect();
    let b = frame[12..].chunks(4).map(|p| [p[0], p[1], p[2], p[3]]).collect();
    (a, b)
}

#[test]
fn unlit_pixels_and_times_outside_the_sequence_are_black() {
    let show = show();
    let mut seq = Sequence::new("s", 1000);
    seq.rows.push(row(
        Target::Prop(show.props[0].id),
        vec![vec![on(Rgb::RED, 0, 1000)]],
    ));
    let frame = render(&show, &seq, 500);
    assert_eq!(frame.len(), 12 + 16);
    let (a, b) = pixels(&frame);
    assert_eq!(a, vec![[255, 0, 0]; 4]);
    assert_eq!(
        b,
        vec![[0, 0, 0, 0]; 4],
        "every pixel is written, even unlit ones"
    );
    assert!(
        render(&show, &seq, 1000).iter().all(|&b| b == 0),
        "the end is dark"
    );
}

#[test]
fn rgbw_pixels_get_canonical_rgb_with_white_off() {
    let show = show();
    let mut seq = Sequence::new("s", 1000);
    seq.rows.push(row(
        Target::Prop(show.props[1].id),
        vec![vec![on(Rgb::new(10, 20, 30), 0, 1000)]],
    ));
    let (_, b) = pixels(&render(&show, &seq, 0));
    assert_eq!(
        b,
        vec![[10, 20, 30, 0]; 4],
        "color order is applied at output, not here"
    );
}

#[test]
fn groups_draw_across_all_members_as_one_canvas() {
    let show = show();
    let mut seq = Sequence::new("s", 1000);
    let ramp = Effect::new(EffectKind::On, 0, 1000)
        .with_palette([Rgb::BLACK, Rgb::WHITE])
        .with_params(EffectParams::On(OnParams {
            gradient: Gradient::Horizontal,
            ..OnParams::default()
        }));
    seq.rows
        .push(row(Target::Group(show.groups[0].id), vec![vec![ramp]]));
    let (a, b) = pixels(&render(&show, &seq, 0));
    // The group spans x from -0.5 to 2.5, 300 xLights units: a grid 301 cells across, as in
    // xLights. A's pixels fall in cells 0, 33, 66, 100 (positions truncate); B's in 200 to 300.
    let reds: Vec<u8> = a.iter().map(|p| p[0]).chain(b.iter().map(|p| p[0])).collect();
    assert_eq!(reds, vec![0, 28, 56, 85, 170, 198, 226, 255]);

    // A chase counts pixels across the members in order.
    let chase = Effect::new(EffectKind::Chase, 0, 1000).with_params(EffectParams::Chase(ChaseParams {
        width: 0.25,
        ..ChaseParams::default()
    }));
    seq.rows[0].layers[0].effects = vec![chase];
    let (a, b) = pixels(&render(&show, &seq, 500));
    assert_eq!(a.iter().map(|p| p[0]).collect::<Vec<_>>(), vec![0, 0, 0, 0]);
    assert_eq!(b.iter().map(|p| p[0]).collect::<Vec<_>>(), vec![255, 255, 0, 0]);
}

#[test]
fn render_styles_lay_the_group_out_per_effect() {
    let show = show();
    let ramp = |style: pf_model::RenderStyle| {
        let mut effect = Effect::new(EffectKind::On, 0, 1000)
            .with_palette([Rgb::BLACK, Rgb::WHITE])
            .with_params(EffectParams::On(OnParams {
                gradient: Gradient::Horizontal,
                ..OnParams::default()
            }));
        effect.render_style = style;
        effect
    };
    let reds = |effects: Vec<Effect>| {
        let mut seq = Sequence::new("s", 1000);
        seq.rows
            .push(row(Target::Group(show.groups[0].id), vec![effects]));
        let (a, b) = pixels(&render(&show, &seq, 0));
        a.iter()
            .map(|p| p[0])
            .chain(b.iter().map(|p| p[0]))
            .collect::<Vec<u8>>()
    };
    // Per model: the ramp runs across each member on its own.
    assert_eq!(
        reds(vec![ramp(pf_model::RenderStyle::PerModelDefault)]),
        vec![0, 85, 170, 255, 0, 85, 170, 255]
    );
    // Vertical per model: each member is a row, so the ramp runs along each.
    assert_eq!(
        reds(vec![ramp(pf_model::RenderStyle::VerticalPerModel)]),
        vec![0, 85, 170, 255, 0, 85, 170, 255]
    );
    // Single line: all eight pixels in a row.
    assert_eq!(
        reds(vec![ramp(pf_model::RenderStyle::SingleLine)]),
        vec![0, 36, 73, 109, 146, 182, 219, 255]
    );
    // Layers in different styles mix on the same pixels: a flipped ramp added on top.
    let mut flipped = ramp(pf_model::RenderStyle::PerModelDefault);
    flipped.buffer_transform = pf_model::BufferTransform::FlipHorizontal;
    flipped.blend = Blend::Add;
    assert_eq!(
        reds(vec![ramp(pf_model::RenderStyle::PerModelDefault), flipped]),
        vec![255; 8]
    );
}

#[test]
fn submodel_rows_light_only_their_pixels_and_follow_row_order() {
    let mut show = show();
    let middle = pf_model::Region::nodes("Middle", vec![vec![Some(pf_model::NodeRun::new(1, 2))]]);
    let target = Target::Region {
        prop: show.props[0].id,
        region: middle.id,
    };
    show.props[0].regions.push(middle);
    let mut seq = Sequence::new("s", 1000);
    // The whole prop blue, then its middle red on a later row.
    seq.rows.push(row(
        Target::Prop(show.props[0].id),
        vec![vec![on(Rgb::BLUE, 0, 1000)]],
    ));
    seq.rows.push(row(target, vec![vec![on(Rgb::RED, 0, 1000)]]));
    let (a, _) = pixels(&render(&show, &seq, 0));
    assert_eq!(a, vec![[0, 0, 255], [255, 0, 0], [255, 0, 0], [0, 0, 255]]);
    // Rows draw in order: the whole prop now covers its middle.
    seq.rows.swap(0, 1);
    let (a, _) = pixels(&render(&show, &seq, 0));
    assert_eq!(a, vec![[0, 0, 255]; 4]);
}

/// xLights keeps a group's members in the order listed, whole props and submodels mixed: a
/// chase along [Left, Middle/Centre, Right] runs left, through the centre, then right.
#[test]
fn group_chases_follow_interleaved_member_order() {
    let mut show = Show::new("t");
    show.props.push(line("Left", 4, 0.0));
    let mut middle = line("Middle", 4, 2.0);
    let centre = pf_model::Region::nodes("Centre", vec![vec![Some(pf_model::NodeRun::new(1, 2))]]);
    let centre_ref = pf_model::RegionRef {
        prop: middle.id,
        region: centre.id,
    };
    middle.regions.push(centre);
    show.props.push(middle);
    show.props.push(line("Right", 4, 4.0));
    let mut group = Group::new("Across");
    group.members = vec![
        pf_model::GroupMember::Prop(show.props[0].id),
        pf_model::GroupMember::Region(centre_ref),
        pf_model::GroupMember::Prop(show.props[2].id),
    ];
    show.groups.push(group);
    let mut seq = Sequence::new("s", 1000);
    let chase = Effect::new(EffectKind::Chase, 0, 1000).with_params(EffectParams::Chase(ChaseParams {
        width: 0.05,
        ..ChaseParams::default()
    }));
    seq.rows
        .push(row(Target::Group(show.groups[0].id), vec![vec![chase]]));
    // Show-wide pixels in member order: Left 0-3, Middle's centre 5-6, Right 8-11.
    let order = [0usize, 1, 2, 3, 5, 6, 8, 9, 10, 11];
    let mut visited = Vec::new();
    for t in (0..1000).step_by(25) {
        let frame = render(&show, &seq, t);
        let lit: Vec<usize> = (0..12).filter(|&p| frame[p * 3] > 0).collect();
        assert!(
            !lit.contains(&4) && !lit.contains(&7),
            "only the centre of Middle is in the group: {lit:?}"
        );
        if let Some(&first) = lit.first() {
            let at = order.iter().position(|&p| p == first).unwrap();
            if visited.last() != Some(&at) {
                visited.push(at);
            }
        }
    }
    assert!(
        visited.windows(2).all(|w| w[0] < w[1]),
        "the chase runs left, centre, right: {visited:?}"
    );
    assert!(visited.contains(&4) || visited.contains(&5), "{visited:?}");
}

/// A 12-pixel line with a face: mouths AI (pixels 0-1) and rest (2), eyes open (4-5) and
/// closed (6), outline (8-11).
fn singing_show() -> (Show, Sequence, TimingTrackId) {
    let mut show = Show::new("t");
    let mut prop = line("Face", 12, 0.0);
    let mut face = pf_model::FaceDefinition::default();
    face.mouths
        .insert(pf_model::Phoneme::Ai, vec![pf_model::NodeRange::new(0, 2)]);
    face.mouths
        .insert(pf_model::Phoneme::Rest, vec![pf_model::NodeRange::new(2, 3)]);
    face.eyes_open = vec![pf_model::NodeRange::new(4, 6)];
    face.eyes_closed = vec![pf_model::NodeRange::new(6, 7)];
    face.outline = vec![pf_model::NodeRange::new(8, 12)];
    prop.regions.push(pf_model::Region::face("Singer", face));
    show.props.push(prop);
    let mut seq = Sequence::new("s", 10_000);
    let track = TimingTrack::new(
        "Lyrics (phonemes)",
        TimingKind::Phonemes,
        vec![Mark::new(0, 500, "AI")],
    );
    let id = track.id;
    seq.timing_tracks.push(track);
    (show, seq, id)
}

fn faces(track: TimingTrackId, eyes: FaceEyes, colors: FaceColorSource, outline: bool) -> Effect {
    Effect::new(EffectKind::Faces, 0, 10_000)
        .with_palette([Rgb::RED, Rgb::GREEN, Rgb::BLUE])
        .with_params(EffectParams::Faces(FacesParams {
            face: "Singer".into(),
            timing_track: Some(track),
            eyes,
            colors,
            outline,
        }))
}

fn lit(frame: &[u8]) -> Vec<[u8; 3]> {
    frame.chunks(3).map(|p| [p[0], p[1], p[2]]).collect()
}

#[test]
fn faces_light_the_mouth_for_the_phoneme_and_the_eyes() {
    let (show, mut seq, track) = singing_show();
    let effect = faces(track, FaceEyes::Open, FaceColorSource::Palette, true);
    seq.rows
        .push(row(Target::Prop(show.props[0].id), vec![vec![effect]]));
    const R: [u8; 3] = [255, 0, 0];
    const G: [u8; 3] = [0, 255, 0];
    const B: [u8; 3] = [0, 0, 255];
    const O: [u8; 3] = [0, 0, 0];
    // Singing "AI": mouth red, open eyes green, outline blue.
    assert_eq!(
        lit(&render(&show, &seq, 100)),
        vec![R, R, O, O, G, G, O, O, B, B, B, B]
    );
    // After the mark the mouth is at rest.
    assert_eq!(
        lit(&render(&show, &seq, 600)),
        vec![O, O, R, O, G, G, O, O, B, B, B, B]
    );

    // Closed eyes, no outline.
    seq.rows[0].layers[0].effects[0] = faces(track, FaceEyes::Closed, FaceColorSource::Palette, false);
    assert_eq!(
        lit(&render(&show, &seq, 100)),
        vec![R, R, O, O, O, O, G, O, O, O, O, O]
    );
}

#[test]
fn faces_use_their_own_colors_and_work_on_a_submodel_row() {
    let (mut show, mut seq, track) = singing_show();
    let pf_model::RegionKind::Face(face) = &mut show.props[0].regions[0].kind else {
        unreachable!()
    };
    face.colors = Some(pf_model::FaceColors {
        mouths: [(pf_model::Phoneme::Ai, Rgb::new(255, 128, 0))].into(),
        ..Default::default()
    });
    // A submodel of the mouth and eyes only: the outline isn't on it.
    let half = pf_model::Region::nodes("Half", vec![vec![Some(pf_model::NodeRun::new(0, 6))]]);
    let target = Target::Region {
        prop: show.props[0].id,
        region: half.id,
    };
    show.props[0].regions.push(half);
    seq.rows.push(row(
        target,
        vec![vec![faces(track, FaceEyes::Open, FaceColorSource::Face, true)]],
    ));
    let frame = lit(&render(&show, &seq, 100));
    assert_eq!(&frame[..2], &[[255, 128, 0]; 2], "the face's mouth color");
    assert_eq!(&frame[4..6], &[[255, 255, 255]; 2], "no eye color: white");
    assert_eq!(&frame[8..], &[[0, 0, 0]; 4], "outside the submodel");
}

/// On a group every member with the face sings in its own pixels (xLights draws nothing for a
/// node-range face on a group; PixelFlow does on purpose). One renderer draws frame after frame,
/// reusing its face lookups.
#[test]
fn faces_sing_on_every_group_member_frame_after_frame() {
    let (mut show, mut seq, track) = singing_show();
    let mut second = show.props[0].clone();
    second.id = pf_model::PropId::new();
    second.name = "Face 2".into();
    second.transform.position = Vec3::new(20.0, 0.0, 0.0);
    show.props.push(second);
    let mut group = Group::new("Choir");
    group.members = vec![show.props[1].id.into(), show.props[0].id.into()];
    let gid = group.id;
    show.groups.push(group);
    seq.rows.push(row(
        Target::Group(gid),
        vec![vec![faces(
            track,
            FaceEyes::Open,
            FaceColorSource::Palette,
            false,
        )]],
    ));
    const R: [u8; 3] = [255, 0, 0];
    const G: [u8; 3] = [0, 255, 0];
    const O: [u8; 3] = [0, 0, 0];
    let singing = vec![R, R, O, O, G, G, O, O, O, O, O, O];
    let resting = vec![O, O, R, O, G, G, O, O, O, O, O, O];
    let mut r = renderer(&show);
    let mut frame = vec![0; r.frame_len()];
    for (t, face) in [(100, &singing), (600, &resting), (100, &singing)] {
        r.render(&seq, t, &mut frame);
        let pixels = lit(&frame);
        assert_eq!(&pixels[..12], face.as_slice(), "first prop at {t} ms");
        assert_eq!(&pixels[12..], face.as_slice(), "second prop at {t} ms");
    }
}

#[test]
fn blinking_eyes_close_briefly_and_render_the_same_every_time() {
    let (show, mut seq, track) = singing_show();
    let effect = faces(track, FaceEyes::Auto, FaceColorSource::Palette, false);
    let blink = (0..10_000)
        .step_by(25)
        .find(|&t| pf_render::faces::blinking(effect.id.seed(), 0, t))
        .expect("a blink within 10 s");
    seq.rows
        .push(row(Target::Prop(show.props[0].id), vec![vec![effect]]));
    let closed = lit(&render(&show, &seq, blink));
    assert_eq!(closed[6], [0, 255, 0], "closed eyes lit");
    assert_eq!(closed[4], [0, 0, 0]);
    let open = lit(&render(&show, &seq, blink + pf_render::faces::BLINK_MS + 25));
    assert_eq!((open[4], open[6]), ([0, 255, 0], [0, 0, 0]));
    assert_eq!(lit(&render(&show, &seq, blink)), closed);
}

#[test]
fn layers_blend_bottom_to_top() {
    let show = show();
    let a = Target::Prop(show.props[0].id);
    let check = |blend: Blend, expected: [u8; 3]| {
        let mut top = on(Rgb::new(0, 100, 200), 0, 1000);
        top.blend = blend;
        let mut seq = Sequence::new("s", 1000);
        seq.rows
            .push(row(a, vec![vec![on(Rgb::new(200, 100, 0), 0, 1000)], vec![top]]));
        let (pixels, _) = pixels(&render(&show, &seq, 0));
        assert_eq!(pixels[0], expected, "{blend:?}");
    };
    check(Blend::Normal, [0, 100, 200]);
    check(Blend::Add, [200, 200, 200]);
    check(Blend::Max, [200, 100, 200]);
    check(Blend::Multiply, [0, 39, 0]);

    // A top layer that lights only some pixels lets the bottom show through elsewhere.
    let mut seq = Sequence::new("s", 1000);
    let chase = Effect::new(EffectKind::Chase, 0, 1000)
        .with_palette([Rgb::BLUE])
        .with_params(EffectParams::Chase(ChaseParams {
            width: 0.25,
            speed: 0.0,
            ..ChaseParams::default()
        }));
    seq.rows
        .push(row(a, vec![vec![on(Rgb::RED, 0, 1000)], vec![chase]]));
    let (pixels, _) = pixels(&render(&show, &seq, 0));
    assert_eq!(pixels, vec![[0, 0, 255], [255, 0, 0], [255, 0, 0], [255, 0, 0]]);
}

/// A chase lighting only prop A's first pixel.
fn first_pixel(color: Rgb) -> Effect {
    Effect::new(EffectKind::Chase, 0, 1000)
        .with_palette([color])
        .with_params(EffectParams::Chase(ChaseParams {
            width: 0.25,
            speed: 0.0,
            ..ChaseParams::default()
        }))
}

#[test]
fn behind_is_xlights_2_reveals_1_with_layer_1_on_top() {
    // xLights layer 1 (drawn on top, PixelFlow's last layer) set to "2 reveals 1" over layer 2
    // (PixelFlow's first): layer 2 shows where it's lit, layer 1 only where layer 2 is dark.
    let show = show();
    let a = Target::Prop(show.props[0].id);
    let mut top = on(Rgb::RED, 0, 1000);
    top.blend = Blend::Behind;
    let mut seq = Sequence::new("s", 1000);
    seq.rows
        .push(row(a, vec![vec![first_pixel(Rgb::BLUE)], vec![top.clone()]]));
    let (lit, _) = pixels(&render(&show, &seq, 0));
    assert_eq!(lit, vec![[0, 0, 255], [255, 0, 0], [255, 0, 0], [255, 0, 0]]);

    // The other way up, the chase would be hidden where the red is lit.
    let mut chase = first_pixel(Rgb::BLUE);
    chase.blend = Blend::Behind;
    let mut seq = Sequence::new("s", 1000);
    seq.rows
        .push(row(a, vec![vec![on(Rgb::RED, 0, 1000)], vec![chase]]));
    assert_eq!(pixels(&render(&show, &seq, 0)).0, vec![[255, 0, 0]; 4]);
}

#[test]
fn the_lowest_effect_drawn_covers_whatever_its_blend() {
    // As in xLights: with nothing below it, a mask or a blend that needs a lit layer below still
    // draws its effect. Here the bottom layer is empty at this moment.
    let show = show();
    let a = Target::Prop(show.props[0].id);
    for blend in [
        Blend::Mask,
        Blend::Clip,
        Blend::Multiply,
        Blend::Subtract,
        Blend::Behind,
    ] {
        let mut top = on(Rgb::GREEN, 0, 1000);
        top.blend = blend;
        let mut seq = Sequence::new("s", 2000);
        seq.rows
            .push(row(a, vec![vec![on(Rgb::RED, 1000, 2000)], vec![top]]));
        assert_eq!(
            pixels(&render(&show, &seq, 0)).0,
            vec![[0, 255, 0]; 4],
            "{blend:?}"
        );
    }
    // Once the bottom layer has an effect, the blend applies: the mask blacks out the red.
    let mut mask = on(Rgb::GREEN, 0, 2000);
    mask.blend = Blend::Mask;
    let mut seq = Sequence::new("s", 2000);
    seq.rows
        .push(row(a, vec![vec![on(Rgb::RED, 1000, 2000)], vec![mask]]));
    assert_eq!(pixels(&render(&show, &seq, 1500)).0, vec![[0, 0, 0]; 4]);
}

#[test]
fn blur_spreads_an_effect_over_the_targets_grid_like_xlights() {
    // A 9-pixel line is a 9 × 1 grid: Blur 4 (xLights 5) averages each pixel with two either
    // side, inside the line. The lit first pixel spreads over three, at 1/3, 1/4, 1/5 coverage,
    // and as the bottom layer its blurred color is dimmed by that coverage again, as in xLights.
    let mut show = Show::new("t");
    show.props.push(line("Line", 9, 0.0));
    let mut chase = Effect::new(EffectKind::Chase, 0, 1000)
        .with_palette([Rgb::BLUE])
        .with_params(EffectParams::Chase(ChaseParams {
            width: 0.1,
            speed: 0.0,
            ..ChaseParams::default()
        }));
    let mut seq = Sequence::new("s", 1000);
    seq.rows
        .push(row(Target::Prop(show.props[0].id), vec![vec![chase.clone()]]));
    let blue = |frame: &[u8]| frame.chunks(3).map(|p| p[2]).collect::<Vec<u8>>();
    assert_eq!(blue(&render(&show, &seq, 0)), vec![255, 0, 0, 0, 0, 0, 0, 0, 0]);
    chase.blur = 4;
    seq.rows[0].layers[0].effects[0] = chase;
    let blurred = blue(&render(&show, &seq, 0));
    let want = |a: f32| (255.0 * a * a).round() as u8;
    assert_eq!(
        blurred,
        vec![want(1.0 / 3.0), want(0.25), want(0.2), 0, 0, 0, 0, 0, 0]
    );
}

#[test]
fn curves_change_settings_over_the_effect() {
    // One band, standing still, growing from one pixel of nine to all of them over the effect.
    let mut show = Show::new("t");
    show.props.push(line("Line", 9, 0.0));
    let mut chase = Effect::new(EffectKind::Chase, 1000, 2000)
        .with_palette([Rgb::BLUE])
        .with_params(EffectParams::Chase(ChaseParams {
            width: 0.5,
            speed: 0.0,
            ..ChaseParams::default()
        }));
    chase.curves.insert("width".into(), Curve::ramp(0.1, 1.0));
    let mut seq = Sequence::new("s", 3000);
    seq.rows
        .push(row(Target::Prop(show.props[0].id), vec![vec![chase.clone()]]));
    let lit = |seq: &Sequence, t: u64| render(&show, seq, t).chunks(3).filter(|p| p[2] > 0).count();
    assert_eq!(lit(&seq, 1000), 1, "the curve's start, not the setting's 0.5");
    assert_eq!(lit(&seq, 1500), 5);
    assert_eq!(lit(&seq, 1999), 9);

    // Blur that steps on halfway: sharp, then soft.
    chase.curves.clear();
    chase.blur = 0;
    chase.curves.insert(
        "blur".into(),
        Curve::custom(0.0, 4.0, vec![[0.5, 0.0], [0.5, 1.0]]),
    );
    if let EffectParams::Chase(p) = &mut chase.params {
        p.width = 0.1;
    }
    seq.rows[0].layers[0].effects[0] = chase;
    assert_eq!(lit(&seq, 1400), 1);
    assert_eq!(lit(&seq, 1500), 3, "blurred over its neighbours");
}

#[test]
fn sparkles_are_the_same_every_render_and_cover_about_5_in_208_minus_the_setting() {
    let mut show = Show::new("t");
    show.props.push(line("Line", 2000, 0.0));
    let target = Target::Prop(show.props[0].id);
    let mut effect = on(Rgb::RED, 0, 10_000);
    effect.sparkles = 150;
    effect.sparkle_color = Rgb::new(0, 0, 255);
    let mut seq = Sequence::new("s", 10_000);
    seq.rows.push(row(target, vec![vec![effect]]));
    // Frame 40 at 25 ms: one renderer twice, and a fresh one (as an export would), agree.
    let mut r = renderer(&show);
    let mut first = vec![0; r.frame_len()];
    r.render(&seq, 1000, &mut first);
    let mut again = vec![0; r.frame_len()];
    r.render(&seq, 1000, &mut again);
    assert_eq!(first, again);
    assert_eq!(render(&show, &seq, 1000), first);

    let sparkling = |frame: &[u8]| frame.chunks(3).filter(|p| p[2] > 0).count();
    let n = sparkling(&first);
    let want = 2000.0 * 5.0 / 58.0;
    assert!(
        (n as f64 - want).abs() < want * 0.2,
        "{n} sparkling, about {want} expected"
    );
    assert!(
        first
            .chunks(3)
            .all(|p| p == [255, 0, 0] || (p[0] == 0 && p[2] >= 135))
    );
    // They move on from frame to frame.
    assert_ne!(render(&show, &seq, 1025), first);
    // None where the effect is unlit, and none at 0.
    seq.rows[0].layers[0].effects[0].sparkles = 0;
    assert_eq!(sparkling(&render(&show, &seq, 1000)), 0);
}

#[test]
fn fades_scale_the_effect_and_later_rows_cover_earlier_ones() {
    let show = show();
    let a = Target::Prop(show.props[0].id);
    let mut fading = on(Rgb::WHITE, 1000, 2000);
    fading.fade_in_ms = 400;
    fading.fade_out_ms = 200;
    let mut seq = Sequence::new("s", 5000);
    seq.rows.push(row(a, vec![vec![fading]]));
    let red_at = |seq: &Sequence, t: u64| pixels(&render(&show, seq, t)).0[0][0];
    assert_eq!(red_at(&seq, 1000), 0);
    assert_eq!(red_at(&seq, 1200), 128);
    assert_eq!(red_at(&seq, 1500), 255);
    assert_eq!(red_at(&seq, 1900), 128);
    assert_eq!(red_at(&seq, 2000), 0, "ended");

    // A later row covers an earlier one on the same pixels: the group row draws over prop A's row.
    let mut seq = Sequence::new("s", 5000);
    seq.rows.push(row(a, vec![vec![on(Rgb::RED, 0, 5000)]]));
    let mut half = on(Rgb::BLUE, 0, 5000);
    half.params = EffectParams::On(OnParams {
        start_level: 0.5,
        end_level: 0.5,
        ..OnParams::default()
    });
    seq.rows
        .push(row(Target::Group(show.groups[0].id), vec![vec![half]]));
    let (a_pixels, b_pixels) = pixels(&render(&show, &seq, 0));
    assert_eq!(a_pixels[0], [128, 0, 128], "half-covered red");
    assert_eq!(b_pixels[0], [0, 0, 128, 0]);
}

#[test]
fn broken_effects_and_unknown_targets_are_skipped() {
    let show = show();
    let mut seq = Sequence::new("s", 1000);
    seq.rows.push(row(
        Target::Prop(pf_model::PropId::new()),
        vec![vec![on(Rgb::RED, 0, 1000)]],
    ));
    seq.rows.push(row(
        Target::Prop(show.props[0].id),
        vec![vec![on(Rgb::RED, 600, 400)]],
    ));
    seq.rows
        .push(row(Target::Group(pf_model::GroupId::new()), vec![]));
    assert!(render(&show, &seq, 500).iter().all(|&b| b == 0));
}

#[test]
fn frames_are_reproducible_in_any_order() {
    let show = show();
    let mut seq = Sequence::new("s", 10_000);
    let g = Target::Group(show.groups[0].id);
    seq.rows.push(row(
        g,
        vec![
            vec![Effect::new(EffectKind::Fire, 0, 10_000)],
            vec![Effect::new(EffectKind::Twinkle, 0, 10_000).with_palette([Rgb::BLUE])],
        ],
    ));
    let mut r = renderer(&show);
    let mut forward = Vec::new();
    let mut frame = vec![0; r.frame_len()];
    for i in 0..40 {
        r.render_frame(&seq, i, &mut frame);
        forward.push(frame.clone());
    }
    let mut fresh = renderer(&show);
    for i in (0..40).rev() {
        fresh.render_frame(&seq, i, &mut frame);
        assert_eq!(frame, forward[i as usize], "frame {i}");
    }
}

/// A synthetic show of `props` matrices of 50 × 20 pixels (1000 each).
fn big_show(props: usize) -> Show {
    let mut show = Show::new("big");
    let mut group = Group::new("All");
    for i in 0..props {
        let mut prop = Prop::new(
            format!("M{i}"),
            ShapeSource::Generator(Generator::Matrix {
                columns: 50,
                rows: 20,
                width: 2.0,
                height: 1.0,
                wiring: Default::default(),
            }),
        );
        prop.transform.position = Vec3::new((i % 10) as f32 * 2.5, (i / 10) as f32 * 1.5, 0.0);
        group.members.push(prop.id.into());
        show.props.push(prop);
    }
    show.groups.push(group);
    show
}

/// Run with `cargo test -p pf-render --release -- --ignored --nocapture`.
#[test]
#[ignore = "benchmark; run with --release --ignored --nocapture"]
fn benchmark_100k_pixels() {
    let show = big_show(100);
    let map = pf_mapping::map_show(&show).0;
    let started = Instant::now();
    let mut r = Renderer::new(&show, &map);
    println!(
        "geometry for 100k pixels: {:.1} ms",
        started.elapsed().as_secs_f64() * 1000.0
    );
    let mut frame = vec![0; r.frame_len()];
    let group = Target::Group(show.groups[0].id);
    let props: Vec<Target> = show.props.iter().map(|p| Target::Prop(p.id)).collect();

    let cases: Vec<(&str, Sequence)> = vec![
        ("on (group)", one_effect(group, EffectKind::On)),
        ("color wash (group)", one_effect(group, EffectKind::ColorWash)),
        ("chase (group)", one_effect(group, EffectKind::Chase)),
        ("bars (group)", one_effect(group, EffectKind::Bars)),
        ("wave (group)", one_effect(group, EffectKind::Wave)),
        ("twinkle (group)", one_effect(group, EffectKind::Twinkle)),
        ("spiral (group)", one_effect(group, EffectKind::Spiral)),
        ("ripple (group)", one_effect(group, EffectKind::Ripple)),
        ("meteors x5 (group)", one_effect(group, EffectKind::Meteors)),
        ("fire (group)", one_effect(group, EffectKind::Fire)),
        ("fire on each of 100 props", {
            let mut seq = Sequence::new("b", 60_000);
            for &t in &props {
                seq.rows
                    .push(row(t, vec![vec![Effect::new(EffectKind::Fire, 0, 60_000)]]));
            }
            seq
        }),
        ("on + twinkle (add) + chase, 2 rows", {
            let mut seq = one_effect(group, EffectKind::On);
            let mut tw = Effect::new(EffectKind::Twinkle, 0, 60_000);
            tw.blend = Blend::Add;
            seq.rows[0].layers.push(Layer { effects: vec![tw] });
            seq.rows
                .push(row(group, vec![vec![Effect::new(EffectKind::Chase, 0, 60_000)]]));
            seq
        }),
    ];
    for (name, seq) in cases {
        let frames = 200;
        let started = Instant::now();
        for i in 0..frames {
            r.render_frame(&seq, i, &mut frame);
        }
        let ms = started.elapsed().as_secs_f64() * 1000.0 / frames as f64;
        println!(
            "{name:40} {ms:7.2} ms/frame  ({:.0} fps at 200k pixels)",
            1000.0 / (ms * 2.0)
        );
    }
}

fn one_effect(target: Target, kind: EffectKind) -> Sequence {
    let mut seq = Sequence::new("b", 60_000);
    seq.rows.push(row(
        target,
        vec![vec![Effect::new(kind, 0, 60_000).with_palette([
            Rgb::RED,
            Rgb::GREEN,
            Rgb::BLUE,
        ])]],
    ));
    seq
}

#[test]
fn shapes_on_a_timing_track_appear_at_its_marks() {
    let mut show = Show::new("t");
    show.props.push(line("Strip", 12, 0.0));
    let mut seq = Sequence::new("s", 10_000);
    let track = TimingTrack::new(
        "Beats",
        TimingKind::Custom,
        vec![
            Mark::new(500, 600, ""),
            Mark::new(2000, 2100, ""),
            Mark::new(5000, 5100, ""),
        ],
    );
    let id = track.id;
    seq.timing_tracks.push(track);
    // From 1 s to 10 s: the first mark is before it, so it doesn't count.
    let effect = Effect::new(EffectKind::Shape, 1000, 10_000)
        .with_palette([Rgb::RED, Rgb::BLUE])
        .with_params(EffectParams::Shape(ShapeParams {
            shape: ShapeObject::Circle,
            start_size: 3.0,
            growth: 0.0,
            lifetime: 10.0,
            fade: false,
            random_location: false,
            timing_track: Some(id),
            ..ShapeParams::default()
        }));
    seq.rows
        .push(row(Target::Prop(show.props[0].id), vec![vec![effect]]));
    let on = |seq: &Sequence, t: u64| -> Vec<usize> {
        lit(&render(&show, seq, t))
            .iter()
            .enumerate()
            .filter(|(_, p)| **p != [0, 0, 0])
            .map(|(i, _)| i)
            .collect()
    };
    assert!(
        on(&seq, 1500).is_empty(),
        "nothing before the first mark in the effect"
    );
    // A circle of radius 3 around the strip's middle (cell 6) crosses it at cells 3 and 9.
    assert_eq!(on(&seq, 2100), vec![3, 9]);
    assert_eq!(lit(&render(&show, &seq, 2100))[3], [255, 0, 0]);
    assert!(on(&seq, 3500).is_empty(), "it lasts 0.9 s");
    assert_eq!(
        lit(&render(&show, &seq, 5100))[3],
        [0, 0, 255],
        "the next one takes the next color"
    );
    // A timing track that's gone: no shapes.
    seq.timing_tracks.clear();
    assert!(on(&seq, 2100).is_empty());
}
