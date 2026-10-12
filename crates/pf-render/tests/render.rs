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

/// A group's effect on an earlier row, then prop A's effect on a later one, as each way of
/// blending rows draws prop A.
fn group_then_prop(blending: RowBlending, group: Effect, prop: Effect) -> Vec<[u8; 3]> {
    let show = show();
    let mut seq = Sequence::new("s", 1000);
    seq.row_blending = blending;
    seq.rows
        .push(row(Target::Group(show.groups[0].id), vec![vec![group]]));
    seq.rows
        .push(row(Target::Prop(show.props[0].id), vec![vec![prop]]));
    pixels(&render(&show, &seq, 0)).0
}

#[test]
fn with_xlights_model_blending_a_rows_lowest_effect_mixes_with_the_rows_before_it() {
    const BLUE: [u8; 3] = [0, 0, 255];
    const RED: [u8; 3] = [255, 0, 0];
    // xLights' "2 reveals 1" on the prop's only layer, over a group's wash from the row above:
    // with Model Blending the wash is what's "below" and it shows (the prop stays blue); without
    // it, and in PixelFlow's own way, the prop's effect is all there is on the row and it shows.
    let mut behind = on(Rgb::RED, 0, 1000);
    behind.blend = Blend::Behind;
    let drawn = |blending| group_then_prop(blending, on(Rgb::BLUE, 0, 1000), behind.clone());
    assert_eq!(drawn(RowBlending::Blend), vec![BLUE; 4]);
    assert_eq!(drawn(RowBlending::Replace), vec![RED; 4]);
    assert_eq!(drawn(RowBlending::Cover), vec![RED; 4]);
    // Where the row above is dark (the chase lights the group's first quarter: two of prop A's
    // pixels), "2 reveals 1" shows the prop's effect.
    let drawn = |blending| group_then_prop(blending, first_pixel(Rgb::BLUE), behind.clone());
    assert_eq!(drawn(RowBlending::Blend), vec![BLUE, BLUE, RED, RED]);

    // Any blend mixes the same way: an additive effect adds to the row above.
    let mut add = on(Rgb::new(100, 0, 0), 0, 1000);
    add.blend = Blend::Add;
    let drawn = |blending| group_then_prop(blending, on(Rgb::new(0, 0, 100), 0, 1000), add.clone());
    assert_eq!(drawn(RowBlending::Blend), vec![[100, 0, 100]; 4]);
    assert_eq!(drawn(RowBlending::Cover), vec![[100, 0, 0]; 4]);

    // A row with the plain blend lets the row above show where it's clear, with Model Blending
    // as in PixelFlow's own way; without Model Blending it writes all its pixels, lit or not.
    let drawn = |blending| group_then_prop(blending, on(Rgb::BLUE, 0, 1000), first_pixel(Rgb::RED));
    assert_eq!(drawn(RowBlending::Blend), vec![RED, BLUE, BLUE, BLUE]);
    assert_eq!(drawn(RowBlending::Cover), vec![RED, BLUE, BLUE, BLUE]);
    assert_eq!(drawn(RowBlending::Replace), vec![RED, [0; 3], [0; 3], [0; 3]]);
}

#[test]
fn a_row_only_takes_over_its_pixels_while_it_has_an_effect_playing() {
    // Without Model Blending xLights writes a model's pixels only on frames where one of its
    // effects plays: before and after, the row above shows.
    let show = show();
    let mut seq = Sequence::new("s", 2000);
    seq.row_blending = RowBlending::Replace;
    seq.rows.push(row(
        Target::Group(show.groups[0].id),
        vec![vec![on(Rgb::BLUE, 0, 2000)]],
    ));
    seq.rows.push(row(
        Target::Prop(show.props[0].id),
        vec![vec![first_pixel(Rgb::RED)]],
    ));
    let (a, b) = pixels(&render(&show, &seq, 500));
    assert_eq!(a, vec![[255, 0, 0], [0; 3], [0; 3], [0; 3]]);
    assert_eq!(
        b,
        vec![[0, 0, 255, 0]; 4],
        "the other prop keeps the group's effect"
    );
    assert_eq!(pixels(&render(&show, &seq, 1500)).0, vec![[0, 0, 255]; 4]);
}

#[test]
fn without_model_blending_a_submodels_row_mixes_with_its_props_own_row() {
    // xLights draws a model's submodels right after the model. Without Model Blending a
    // submodel's effects mix with the model's own (by the lowest one's blend) on frames where
    // the model has an effect playing, and replace what's there on frames where it doesn't.
    let mut show = show();
    let middle = pf_model::Region::nodes("Middle", vec![vec![Some(pf_model::NodeRun::new(1, 2))]]);
    let target = Target::Region {
        prop: show.props[0].id,
        region: middle.id,
    };
    show.props[0].regions.push(middle);
    let mut seq = Sequence::new("s", 2000);
    seq.row_blending = RowBlending::Replace;
    seq.rows.push(row(
        Target::Group(show.groups[0].id),
        vec![vec![on(Rgb::new(0, 0, 100), 0, 2000)]],
    ));
    seq.rows.push(row(
        Target::Prop(show.props[0].id),
        vec![vec![on(Rgb::new(0, 100, 0), 0, 1000)]],
    ));
    let mut add = on(Rgb::new(100, 0, 0), 0, 2000);
    add.blend = Blend::Add;
    seq.rows.push(row(target, vec![vec![add]]));
    // While the prop's own row plays, the submodel adds to it.
    let (a, _) = pixels(&render(&show, &seq, 500));
    assert_eq!(a, vec![[0, 100, 0], [100, 100, 0], [100, 100, 0], [0, 100, 0]]);
    // Once it has ended, the submodel replaces the group's effect on its pixels.
    let (a, _) = pixels(&render(&show, &seq, 1500));
    assert_eq!(a, vec![[0, 0, 100], [100, 0, 0], [100, 0, 0], [0, 0, 100]]);
    // With Model Blending it always mixes with whatever is there.
    seq.row_blending = RowBlending::Blend;
    let (a, _) = pixels(&render(&show, &seq, 1500));
    assert_eq!(a, vec![[0, 0, 100], [100, 0, 100], [100, 0, 100], [0, 0, 100]]);
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
    // The share is a chance one, so it is read over forty frames: a single frame's count moves
    // by about a tenth of itself from one effect's seed to the next.
    let frames = 40;
    let n = (0..frames)
        .map(|f| sparkling(&render(&show, &seq, 1000 + 25 * f)))
        .sum::<usize>() as f64
        / frames as f64;
    let want = 2000.0 * 5.0 / 58.0;
    assert!(
        (n - want).abs() < want * 0.2,
        "{n} sparkling a frame, about {want} expected"
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
    let mut chase = Effect::new(EffectKind::Chase, 0, 10_000).with_palette([Rgb::RED]);
    chase.sparkle_color = Rgb::new(0, 0, 255);
    seq.rows[0].layers[0].effects[0] = chase;
    let plain = render(&show, &seq, 1000);
    seq.rows[0].layers[0].effects[0].sparkles = 200;
    let sparkled = render(&show, &seq, 1000);
    let dark = |frame: &[u8]| frame.chunks(3).filter(|p| p == &[0, 0, 0]).count();
    assert!(sparkling(&sparkled) > 20 && dark(&plain) > 1000);
    assert_eq!(
        dark(&sparkled),
        dark(&plain),
        "the pixels the chase leaves dark stay dark"
    );
}

/// A red On with white sparkles over a blue one on a line of 2000 pixels, at one second, as
/// `change` leaves it: the frame, and which pixels are sparkling (the only ones with any green).
/// `top` is the On, so that every frame of a test is of the one effect and sparkles alike.
fn sparkling_over_blue(top: &Effect, change: &dyn Fn(&mut Effect, &mut OnParams)) -> (Vec<u8>, Vec<usize>) {
    let mut show = Show::new("t");
    show.props.push(line("Line", 2000, 0.0));
    let target = Target::Prop(show.props[0].id);
    let mut top = top.clone();
    top.sparkles = 150;
    let mut params = OnParams::default();
    change(&mut top, &mut params);
    top.params = EffectParams::On(params);
    let mut seq = Sequence::new("s", 10_000);
    seq.rows
        .push(row(target, vec![vec![on(Rgb::BLUE, 0, 10_000)], vec![top]]));
    let frame = render(&show, &seq, 1000);
    let sparkling = frame
        .chunks(3)
        .enumerate()
        .filter(|(_, p)| p[1] > 0)
        .map(|(i, _)| i)
        .collect();
    (frame, sparkling)
}

#[test]
fn an_effects_transparency_hides_the_effect_and_not_its_sparkles() {
    // In xLights' frames an On at full Transparency is dark and still flashes its sparkles, on
    // as many pixels as at none and as brightly: a layer of sparkles alone.
    let top = on(Rgb::RED, 0, 10_000);
    let over_blue = |change: &dyn Fn(&mut Effect, &mut OnParams)| sparkling_over_blue(&top, change);
    let (solid, want) = over_blue(&|_, _| {});
    let expected = 2000.0 * 5.0 / 58.0;
    assert!(
        (want.len() as f64 - expected).abs() < expected * 0.2,
        "{}",
        want.len()
    );
    for (transparency, between) in [(0.5, [128, 0, 128]), (0.99, [3, 0, 252]), (1.0, [0, 0, 255])] {
        let (frame, sparkling) = over_blue(&|_, on| on.transparency = transparency);
        assert_eq!(sparkling, want, "{transparency}: the same pixels flash");
        for (i, (p, full)) in frame.chunks(3).zip(solid.chunks(3)).enumerate() {
            if want.binary_search(&i).is_ok() {
                assert_eq!(p, full, "{transparency}: pixel {i} flashes as brightly");
            } else {
                assert_eq!(
                    p, between,
                    "{transparency}: pixel {i} is the On, as see-through as it's set"
                );
            }
        }
    }
    // The same when a curve sets the Transparency, and when the effect is blurred.
    let (_, curved) = over_blue(&|e, _| {
        e.curves.insert("transparency".into(), Curve::ramp(1.0, 0.999));
    });
    assert_eq!(curved, want);
    let (_, blurred) = over_blue(&|e, _| e.blur = 3);
    let (frame, sheer) = over_blue(&|e, on| (e.blur, on.transparency) = (3, 1.0));
    assert_eq!(sheer, blurred);
    assert!(frame.chunks(3).all(|p| p[0] == p[1]), "nothing of the red shows");

    // Without sparkles it is as see-through as it ever was.
    let (frame, none) = over_blue(&|e, on| (e.sparkles, on.transparency) = (0, 1.0));
    assert!(none.is_empty() && frame.chunks(3).all(|p| p == [0, 0, 255]));
}

#[test]
fn a_see_through_effects_sparkles_fade_with_it_and_stay_off_what_it_leaves_dark() {
    // What xLights' frames show sparkles looking past is the effect's own Transparency and
    // nothing else. They dim with its fade...
    let top = on(Rgb::RED, 0, 10_000);
    let over_blue = |change: &dyn Fn(&mut Effect, &mut OnParams)| sparkling_over_blue(&top, change);
    let (_, want) = over_blue(&|_, _| {});
    let (frame, fading) = over_blue(&|e, on| (e.fade_in_ms, on.transparency) = (4000, 1.0));
    assert_eq!(fading, want);
    // (41 of the fade's 160 frames in: a full sparkle is a quarter white over the blue.)
    let brightest = frame.chunks(3).map(|p| p[1]).max();
    assert_eq!(brightest, Some(65), "a quarter of the way up the fade");
    // ...and an On dimmed to nothing has no light to sparkle on, see-through or not, whether
    // it dims by showing what's below or by going dark.
    for solid in [false, true] {
        for transparency in [0.0, 0.5, 1.0] {
            let (frame, sparkling) = over_blue(&|_, on| {
                (on.start_level, on.end_level) = (0.0, 0.0);
                (on.solid, on.transparency) = (solid, transparency);
            });
            assert!(sparkling.is_empty(), "solid {solid}, transparency {transparency}");
            let below = if solid {
                (255.0 * transparency + 0.5) as u8
            } else {
                255
            };
            assert!(
                frame.chunks(3).all(|p| p == [0, 0, below]),
                "solid {solid}, transparency {transparency}: {:?}",
                &frame[..3]
            );
        }
    }
}

#[test]
fn color_adjustments_shift_what_an_effect_drew_and_dim_its_sparkles_too() {
    let mut show = Show::new("t");
    show.props.push(line("Line", 2000, 0.0));
    let target = Target::Prop(show.props[0].id);
    let mut seq = Sequence::new("s", 10_000);
    seq.rows.push(row(target, vec![vec![on(Rgb::RED, 0, 10_000)]]));
    let frame = |change: &dyn Fn(&mut Effect)| {
        let mut seq = seq.clone();
        change(&mut seq.rows[0].layers[0].effects[0]);
        render(&show, &seq, 1000)
    };
    let all = |frame: &[u8], color: [u8; 3]| frame.chunks(3).all(|p| p == color);
    assert!(all(&frame(&|_| {}), [255, 0, 0]));
    // A third of a turn round the color wheel; a curve takes the place of the setting (a tenth
    // of the way from -0.4 to 4/15 is a third of a turn back).
    assert!(all(&frame(&|e| e.hue_adjust = 1.0 / 3.0), [0, 255, 0]));
    assert!(all(
        &frame(&|e| {
            e.hue_adjust = 1.0 / 3.0;
            e.curves.insert("hueAdjust".into(), Curve::ramp(-0.4, 4.0 / 15.0));
        }),
        [0, 0, 255]
    ));
    assert!(all(&frame(&|e| e.saturation_adjust = -0.5), [255, 128, 128]));
    assert!(all(&frame(&|e| e.value_adjust = -0.5), [128, 0, 0]));
    assert!(all(&frame(&|e| e.brightness = 0.5), [128, 0, 0]));
    // Half brightness with a contrast of 0.2: value 0.5, from the middle up, gains a fifth.
    assert!(all(
        &frame(&|e| (e.brightness, e.contrast) = (0.5, 0.2)),
        [153, 0, 0]
    ));
    // The sparkles are drawn after the hue turns (they stay white) and before the brightness
    // (they dim with the effect).
    let sparkled = frame(&|e| {
        e.sparkles = 150;
        e.hue_adjust = 1.0 / 3.0;
        e.brightness = 0.5;
    });
    assert!(sparkled.chunks(3).any(|p| p == [128, 128, 128]), "a full sparkle");
    assert!(sparkled.chunks(3).any(|p| p == [0, 128, 0]), "the effect");
    assert!(
        sparkled
            .chunks(3)
            .all(|p| p[1] == 128 || p[1] < 128 && p[0] == p[2] && p[0] < 128),
        "nothing brighter than half: {:?}",
        sparkled.chunks(3).find(|p| p[1] > 128)
    );
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
    // Stepped as xLights steps a fade, here over 16 frames in and 8 out: the first frame is one
    // step up already, and the last frame is dark.
    assert_eq!(red_at(&seq, 1000), 16, "1/16");
    assert_eq!(red_at(&seq, 1200), 143, "9/16");
    assert_eq!(red_at(&seq, 1375), 255, "16/16");
    assert_eq!(red_at(&seq, 1500), 255);
    assert_eq!(red_at(&seq, 1900), 96, "3/8");
    assert_eq!(red_at(&seq, 1975), 0, "its last frame");
    assert_eq!(red_at(&seq, 2000), 0, "ended");
    // Where a fade-in and a fade-out overlap, the effect shows their average; a fade shorter
    // than a frame is no fade.
    let mut short = on(Rgb::WHITE, 0, 200);
    (short.fade_in_ms, short.fade_out_ms) = (200, 100);
    let mut seq = Sequence::new("s", 5000);
    seq.rows.push(row(a, vec![vec![short]]));
    assert_eq!(red_at(&seq, 50), 96, "3/8 in");
    assert_eq!(red_at(&seq, 125), 159, "6/8 in and 2/4 out");
    seq.rows[0].layers[0].effects[0].fade_in_ms = 20;
    assert_eq!(red_at(&seq, 0), 255);

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
fn a_fade_is_transparency_under_the_plain_blend_and_brightness_under_the_others() {
    // As xLights mixes a fading layer (`PixelBufferClass::mixColors`): "Normal" scales the
    // layer's alpha, so what's below shows through; every other method dims the layer's color
    // and then mixes as usual.
    let show = show();
    let a = Target::Prop(show.props[0].id);
    let half_faded = |blend: Blend| {
        let mut top = on(Rgb::BLUE, 0, 1000);
        // 8 frames to fade in: half way up on the fourth.
        (top.fade_in_ms, top.blend) = (200, blend);
        let mut seq = Sequence::new("s", 1000);
        seq.rows
            .push(row(a, vec![vec![on(Rgb::RED, 0, 1000)], vec![top]]));
        pixels(&render(&show, &seq, 75)).0[0]
    };
    assert_eq!(half_faded(Blend::Normal), [128, 0, 128], "half see-through");
    assert_eq!(half_faded(Blend::Add), [255, 0, 128], "half as bright, added");
    assert_eq!(
        half_faded(Blend::Over),
        [0, 0, 128],
        "half as bright, and it still covers"
    );
    assert_eq!(
        half_faded(Blend::Behind),
        [255, 0, 0],
        "what's below is lit, so it shows"
    );
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
    // Rolling flames, and xLights' flickering ones growing and shrinking.
    let flicker = Effect::new(EffectKind::Fire, 0, 10_000).with_params(EffectParams::Fire(FireParams {
        style: FireStyle::Flicker,
        growth_cycles: 4.0,
        hue_shift: 0.1,
        ..Default::default()
    }));
    seq.rows.push(row(
        g,
        vec![
            vec![Effect::new(EffectKind::Fire, 0, 10_000)],
            vec![Effect::new(EffectKind::Twinkle, 0, 10_000).with_palette([Rgb::BLUE])],
        ],
    ));
    seq.rows
        .push(row(Target::Prop(show.props[0].id), vec![vec![flicker]]));
    let mut r = renderer(&show);
    let mut forward = Vec::new();
    let mut frame = vec![0; r.frame_len()];
    for i in 0..40 {
        r.render_frame(&seq, i, &mut frame);
        forward.push(frame.clone());
    }
    assert!(forward.windows(2).all(|w| w[0] != w[1]), "flames move");
    let mut fresh = renderer(&show);
    for i in (0..40).rev() {
        fresh.render_frame(&seq, i, &mut frame);
        assert_eq!(frame, forward[i as usize], "frame {i}");
    }
}

#[test]
fn effects_worked_out_frame_by_frame_look_the_same_played_or_jumped_to() {
    // Falling snow, Lines, Life, and Tendril carry their state from frame to frame; the renderer
    // keeps it while playing, and works it out again from the start when it jumps back.
    let mut show = Show::new("m");
    show.props.push(Prop::new(
        "Matrix",
        ShapeSource::Generator(Generator::Matrix {
            columns: 20,
            rows: 12,
            width: 2.0,
            height: 1.0,
            wiring: Default::default(),
        }),
    ));
    let target = Target::Prop(show.props[0].id);
    let mut seq = Sequence::new("s", 4000);
    let mut lines = Effect::new(EffectKind::Lines, 0, 4000).with_params(EffectParams::Lines(LinesParams {
        trails: 2,
        ..Default::default()
    }));
    lines.curves.insert("speed".into(), Curve::ramp(0.5, 3.0));
    let snow = Effect::new(EffectKind::Snowflakes, 500, 4000).with_params(EffectParams::Snowflakes(
        SnowflakesParams {
            count: 15,
            motion: SnowflakesMotion::PilingUp,
            speed: 25.0,
            ..Default::default()
        },
    ));
    let life = Effect::new(EffectKind::Life, 0, 2000).with_palette([Rgb::GREEN]);
    let tendril =
        Effect::new(EffectKind::Tendril, 2000, 4000).with_params(EffectParams::Tendril(TendrilParams {
            movement: TendrilMovement::Random,
            ..Default::default()
        }));
    seq.rows
        .push(row(target, vec![vec![lines], vec![snow], vec![life, tendril]]));
    let mut played = renderer(&show);
    let mut frames = Vec::new();
    let mut frame = vec![0; played.frame_len()];
    for i in 0..80 {
        played.render_frame(&seq, i, &mut frame);
        frames.push(frame.clone());
    }
    assert!(
        frames.windows(2).filter(|w| w[0] != w[1]).count() > 70,
        "it moves"
    );
    let mut jumping = renderer(&show);
    for i in [79, 3, 40, 39, 41, 10, 79, 0, 60] {
        jumping.render_frame(&seq, i, &mut frame);
        assert_eq!(frame, frames[i as usize], "frame {i}");
    }
    // An edit starts the effect's state afresh.
    let mut edited = seq.clone();
    if let EffectParams::Lines(p) = &mut edited.rows[0].layers[0].effects[0].params {
        p.count = 5;
    }
    played.render_frame(&edited, 80 - 1, &mut frame);
    let mut fresh = renderer(&show);
    let mut expected = vec![0; fresh.frame_len()];
    fresh.render_frame(&edited, 79, &mut expected);
    assert_eq!(frame, expected);
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

/// A 40 x 24 matrix between two lines, and a group of all three that draws on each member in
/// turn: a show with a prop large enough to be cut into several tiles.
fn tiled_show() -> Show {
    let mut show = Show::new("t");
    let mut matrix = Prop::new(
        "Matrix",
        ShapeSource::Generator(Generator::Matrix {
            columns: 40,
            rows: 24,
            width: 2.0,
            height: 1.2,
            wiring: pf_model::MatrixWiring::default(),
        }),
    );
    matrix.transform.position = Vec3::new(4.0, 0.0, 0.0);
    show.props.push(line("A", 30, 0.0));
    show.props.push(matrix);
    show.props.push(line("B", 17, 8.0));
    let mut group = Group::new("All");
    group.layout = pf_model::GroupLayout::PerModelDefault;
    group.members = show.props.iter().map(|p| p.id.into()).collect();
    show.groups.push(group);
    show
}

/// Layers of effects on every prop of [`tiled_show`] and on its group: blurred, sparkling,
/// faded, mixed by blends that look at what's below, and one that carries on from frame to frame.
fn layered_sequence(show: &Show, blending: RowBlending) -> Sequence {
    let mut seq = Sequence::new("s", 2000);
    seq.row_blending = blending;
    let effect = |kind: EffectKind, blend: Blend, blur: u32| {
        let mut effect = Effect::new(kind, 0, 2000).with_palette([Rgb::RED, Rgb::GREEN, Rgb::BLUE]);
        effect.blend = blend;
        effect.blur = blur;
        effect.sparkles = 40;
        effect.fade_in_ms = 300;
        effect
    };
    for (n, prop) in show.props.iter().enumerate() {
        seq.rows.push(row(
            Target::Prop(prop.id),
            vec![
                vec![effect(EffectKind::Plasma, Blend::Normal, n as u32 * 2)],
                vec![effect(EffectKind::Bars, Blend::Average, 0)],
                vec![effect(EffectKind::Life, Blend::Add, 0)],
            ],
        ));
    }
    seq.rows.push(row(
        Target::Group(show.groups[0].id),
        vec![
            vec![effect(EffectKind::Butterfly, Blend::Max, 3)],
            vec![effect(EffectKind::Twinkle, Blend::Reveal, 0)],
        ],
    ));
    seq
}

/// Frames 0 to 5, then 30, then 2 again (a jump back), drawn in turn.
fn drawn(renderer: &mut Renderer, seq: &Sequence) -> Vec<Vec<u8>> {
    [0, 1, 2, 3, 4, 5, 30, 2]
        .into_iter()
        .map(|index| {
            let mut frame = vec![0xAA; renderer.frame_len()];
            renderer.render_frame(seq, index, &mut frame);
            frame
        })
        .collect()
}

/// Two groups, each of a small matrix and a line well away from it, so that each group's grid has
/// gaps between its pixels: the props of the first, then of the second.
fn gappy_groups() -> Show {
    let mut show = Show::new("t");
    for (n, x) in [0.0, 20.0].into_iter().enumerate() {
        let mut matrix = Prop::new(
            format!("Matrix {n}"),
            ShapeSource::Generator(Generator::Matrix {
                columns: 12,
                rows: 10,
                width: 1.2,
                height: 1.0,
                wiring: pf_model::MatrixWiring::default(),
            }),
        );
        matrix.transform.position = Vec3::new(x, 0.0, 0.0);
        show.props.push(matrix);
        show.props.push(line(&format!("Line {n}"), 20, x + 6.0));
        let mut group = Group::new(format!("Group {n}"));
        let members = &show.props[show.props.len() - 2..];
        group.members = members.iter().map(|p| p.id.into()).collect();
        show.groups.push(group);
    }
    show
}

/// Blurred effects on the second of [`gappy_groups`] all the way through, under blends that ask
/// whether a pixel is lit and with color adjustments that light unlit ones; and, while
/// `earlier` says (its start, end, and how bright), a blurred effect on the first group.
fn after_another_blur(show: &Show, earlier: Option<(u64, u64, u8)>) -> Sequence {
    let mut seq = Sequence::new("s", 3000);
    if let Some((start, end, level)) = earlier {
        let mut plasma = Effect::new(EffectKind::Plasma, start, end)
            .with_palette([Rgb::new(level, 40, level), Rgb::new(0, level, 90)]);
        plasma.blur = 9;
        plasma.curves.insert("blur".into(), Curve::ramp(3.0, 12.0));
        seq.rows
            .push(row(Target::Group(show.groups[0].id), vec![vec![plasma]]));
    }
    let blends = [
        Blend::Over,
        Blend::Reveal,
        Blend::Mask,
        Blend::Average,
        Blend::Behind,
        Blend::Highlight,
        Blend::Clip,
        Blend::Subtract,
    ];
    let mut layers = vec![vec![on(Rgb::new(30, 0, 60), 0, 3000)]];
    for (n, blend) in blends.into_iter().enumerate() {
        let kind = [EffectKind::Plasma, EffectKind::Butterfly, EffectKind::Bars][n % 3];
        let mut effect = Effect::new(kind, 0, 3000).with_palette([Rgb::RED, Rgb::BLACK, Rgb::BLUE]);
        effect.blend = blend;
        effect.blur = 4 + n as u32;
        effect.value_adjust = if n % 2 == 0 { 0.2 } else { 0.0 };
        layers.push(vec![effect]);
    }
    seq.rows.push(row(Target::Group(show.groups[1].id), layers));
    seq
}

#[test]
fn a_blurred_effect_draws_the_same_whatever_was_blurred_before_it() {
    // What one blur leaves in the blurs' shared working space never shows in another's pixels:
    // the second group's pixels are the same bytes whether or not the first group's blurred
    // effect played before them (in this frame or in earlier ones), however bright it was, and
    // whether the frame was played up to or jumped to.
    let show = gappy_groups();
    let first = show.props[0].node_count() + show.props[1].node_count();
    let second = |frame: &[u8]| frame[3 * first as usize..].to_vec();
    let played = |seq: &Sequence, frames: u64| {
        let mut r = renderer(&show);
        let mut frame = vec![0; r.frame_len()];
        (0..frames)
            .map(|i| {
                r.render_frame(seq, i, &mut frame);
                second(&frame)
            })
            .collect::<Vec<_>>()
    };
    let frames = 40;
    let alone = played(&after_another_blur(&show, None), frames);
    assert!(alone.windows(2).all(|w| w[0] != w[1]), "it moves");
    for earlier in [(0, 1000, 255), (0, 1000, 254), (0, 3000, 255), (0, 3000, 254)] {
        let seq = after_another_blur(&show, Some(earlier));
        assert!(played(&seq, frames) == alone, "after {earlier:?}");
        let mut jumped = renderer(&show);
        let mut frame = vec![0; jumped.frame_len()];
        for i in [frames - 1, 25, 3] {
            jumped.render_frame(&seq, i, &mut frame);
            assert!(
                second(&frame) == alone[i as usize],
                "after {earlier:?}, jumped to {i}"
            );
        }
    }
}

#[test]
fn a_frame_is_the_same_bytes_on_any_number_of_threads_and_tiles() {
    let show = tiled_show();
    let pool = pf_render::RenderPool::new(3).expect("three threads");
    for blending in RowBlending::ALL {
        let seq = layered_sequence(&show, blending);
        let reference = {
            let mut one = renderer(&show);
            one.set_pool(None);
            drawn(&mut one, &seq)
        };
        assert!(reference[3].iter().any(|&b| b != 0 && b != 0xAA));
        // Left alone (a frame this small stays on the caller's thread), on three threads, and
        // with the show cut into tiles of a pixel, of a size nothing here is a multiple of, and
        // of more than the whole show.
        let mut alone = renderer(&show);
        assert_eq!(drawn(&mut alone, &seq), reference, "{blending:?}");
        for tile in [1, 97, 5000] {
            for threads in [None, Some(pool.clone())] {
                let mut cut = renderer(&show);
                cut.set_tile_pixels(tile);
                cut.set_pool(threads.clone());
                assert_eq!(
                    drawn(&mut cut, &seq),
                    reference,
                    "{blending:?}, tiles of {tile}, {} threads",
                    threads.map_or(1, |pool| pool.threads())
                );
            }
        }
        // Laid out ahead, and cut again part way through: still the same frames.
        let mut recut = renderer(&show);
        recut.lay_out_ahead(&seq);
        recut.set_pool(Some(pool.clone()));
        let mut frame = vec![0; recut.frame_len()];
        recut.render_frame(&seq, 0, &mut frame);
        recut.set_tile_pixels(13);
        assert_eq!(drawn(&mut recut, &seq), reference, "{blending:?}, cut again");
    }
}

#[test]
fn props_out_of_order_in_the_frame_are_written_where_the_map_puts_them() {
    let show = tiled_show();
    let seq = layered_sequence(&show, RowBlending::Cover);
    let (map, _) = pf_mapping::map_show(&show);
    let mut in_order = Renderer::new(&show, &map);
    let reference = drawn(&mut in_order, &seq);
    // The same props with the last one's bytes first in the frame.
    let mut moved = map.clone();
    let last = moved.props.len() - 1;
    let shift = moved.props[last].byte_len();
    for prop in &mut moved.props[..last] {
        prop.frame_offset += shift;
    }
    moved.props[last].frame_offset = 0;
    let pool = pf_render::RenderPool::new(3).expect("three threads");
    for threads in [None, Some(pool)] {
        let mut out_of_order = Renderer::new(&show, &moved);
        out_of_order.set_pool(threads);
        for (frame, wanted) in drawn(&mut out_of_order, &seq).iter().zip(&reference) {
            for (was, now) in map.props.iter().zip(&moved.props) {
                let len = was.byte_len();
                assert_eq!(
                    frame[now.frame_offset..now.frame_offset + len],
                    wanted[was.frame_offset..was.frame_offset + len]
                );
            }
        }
    }
}

#[test]
fn a_frame_shorter_than_the_shows_is_written_as_far_as_it_goes() {
    let show = tiled_show();
    let seq = layered_sequence(&show, RowBlending::Cover);
    let mut whole = renderer(&show);
    let mut full = vec![0xAA; whole.frame_len()];
    whole.render_frame(&seq, 3, &mut full);
    let pool = pf_render::RenderPool::new(3).expect("three threads");
    for threads in [None, Some(pool)] {
        // Ending in the middle of the matrix, part way through a pixel: the pixels that fit are
        // written, and the bytes of the one that doesn't are left alone.
        for len in [0, 100, 1001, full.len() - 1] {
            let mut short = vec![0xAA; len];
            let mut renderer = renderer(&show);
            renderer.set_tile_pixels(64);
            renderer.set_pool(threads.clone());
            renderer.render_frame(&seq, 3, &mut short);
            let pixels = len / 3 * 3;
            assert_eq!(short[..pixels], full[..pixels], "{len} bytes");
            assert!(short[pixels..].iter().all(|&b| b == 0xAA), "{len} bytes");
        }
    }
}

#[test]
fn a_frame_one_thread_has_ready_in_time_is_shared_out_only_when_drawn_flat_out() {
    // 100,000 pixels with one effect on them: a few milliseconds' drawing for one thread.
    let show = big_show(100);
    let map = pf_mapping::map_show(&show).0;
    let seq = one_effect(Target::Group(show.groups[0].id), EffectKind::On);
    let mut renderer = Renderer::new(&show, &map);
    let mut by_the_clock = vec![0; renderer.frame_len()];
    renderer.render_frame(&seq, 0, &mut by_the_clock);
    assert_eq!(renderer.frame_times().threads, 1, "played by a clock");

    // However many threads this machine shares frames out among (one, on a small machine).
    let shared = pf_render::RenderPool::shared().map_or(1, |pool| pool.threads());
    renderer.set_flat_out(true);
    let mut flat_out = vec![0; renderer.frame_len()];
    renderer.render_frame(&seq, 0, &mut flat_out);
    assert_eq!(renderer.frame_times().threads, shared, "flat out");
    assert_eq!(flat_out, by_the_clock);
    renderer.set_flat_out(false);
    renderer.render_frame(&seq, 0, &mut flat_out);
    assert_eq!(renderer.frame_times().threads, 1, "by a clock again");

    // A frame that isn't worth handing over stays where it is, flat out or not.
    let small = big_show(10);
    let map = pf_mapping::map_show(&small).0;
    let seq = one_effect(Target::Group(small.groups[0].id), EffectKind::On);
    let mut renderer = Renderer::new(&small, &map);
    renderer.set_flat_out(true);
    let mut frame = vec![0; renderer.frame_len()];
    renderer.render_frame(&seq, 0, &mut frame);
    assert_eq!(renderer.frame_times().threads, 1, "10,000 pixels, flat out");
}
