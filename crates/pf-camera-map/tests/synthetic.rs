//! End-to-end decoding of synthetic phone videos: pixels at known positions, filmed with noise,
//! blur, exposure changes, a hand-held wobble, background lights, a reflection, and wiring
//! faults, run through sync, slot averaging, decoding, and planning as the app does.

use pf_camera_map::{
    Anomaly, Base, CodeSpec, Image, Owner, PropInput, Sample, Symbol, decode, find_sync, plan, slot_windows,
};

const W: usize = 960;
const H: usize = 540;
const FPS: f64 = 30.0;

/// xorshift64* with Box–Muller normals: deterministic test noise without a dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }

    fn normal(&mut self) -> f64 {
        let (u, v) = (self.next().max(1e-12), self.next());
        (-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos()
    }

    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next()
    }
}

/// One physical light: where the camera sees it, which sequence index it shows, how it looks.
#[derive(Clone)]
struct Light {
    x: f64,
    y: f64,
    index: u32,
    gain: f64,
    sigma: f64,
    /// Swaps red and green (a strip wired GRB but set up as RGB).
    grb: bool,
    dead: bool,
}

/// What the camera sees for a pixel's colour: some crosstalk between channels, as real sensors.
fn camera_colour(symbol: Symbol, grb: bool) -> [f64; 3] {
    let symbol = match (symbol, grb) {
        (Symbol::Red, true) => Symbol::Green,
        (Symbol::Green, true) => Symbol::Red,
        (s, _) => s,
    };
    match symbol {
        Symbol::Off => [0.0; 3],
        Symbol::Red => [235.0, 45.0, 25.0],
        Symbol::Green => [60.0, 240.0, 110.0],
        Symbol::Blue => [30.0, 80.0, 245.0],
        Symbol::White => [250.0, 245.0, 240.0],
    }
}

struct Scene {
    lights: Vec<Light>,
    /// Dim mirror images of some lights (a wet driveway): (light index, x, y).
    reflections: Vec<(usize, f64, f64)>,
    spec: CodeSpec,
    owners: Vec<Owner>,
    props: Vec<PropInput>,
}

/// An arch (50), a 16 × 8 serpentine matrix (128), and a line (30), placed in the layout and
/// filmed from a little to one side (similarity: 40 px per unit, tilted 3°).
fn scene(rng: &mut Rng) -> Scene {
    let mut layout: Vec<Vec<[f64; 2]>> = Vec::new();
    // Arch: radius 4.5 units, centre (-5, 0).
    layout.push(
        (0..50)
            .map(|i| {
                let a = std::f64::consts::PI * (1.0 - f64::from(i) / 49.0);
                [-5.0 + 4.5 * a.cos(), 4.5 * a.sin()]
            })
            .collect(),
    );
    // Matrix: 16 × 8, 0.4 units apart, bottom left at (2, 1), rows back and forth.
    layout.push(
        (0..128)
            .map(|n| {
                let (row, k) = (n / 16, n % 16);
                let col = if row % 2 == 0 { k } else { 15 - k };
                [2.0 + 0.4 * f64::from(col), 1.0 + 0.4 * f64::from(row)]
            })
            .collect(),
    );
    // Line: 30 nodes, 0.3 apart, along the bottom from (-9, -2).
    layout.push((0..30).map(|i| [-9.0 + 0.3 * f64::from(i), -2.0]).collect());

    let (scale, tilt) = (40.0, 3f64.to_radians());
    let to_camera = |p: [f64; 2]| {
        let (s, c) = tilt.sin_cos();
        let (x, y) = (c * p[0] - s * p[1], s * p[0] + c * p[1]);
        [480.0 + scale * x, 330.0 - scale * y]
    };

    let mut lights = Vec::new();
    let mut owners = Vec::new();
    let mut index = 0u32;
    for (prop, points) in layout.iter().enumerate() {
        let n = points.len() as u32;
        for node in 0..n {
            owners.push(Owner { prop, node });
            // Faults: the line is wired from its far end; matrix nodes 40 and 47 swapped.
            let physical = match (prop, node) {
                (2, _) => n - 1 - node,
                (1, 40) => 47,
                (1, 47) => 40,
                _ => node,
            };
            let [x, y] = to_camera(points[physical as usize]);
            lights.push(Light {
                x,
                y,
                index,
                gain: rng.range(0.45, 1.0),
                sigma: rng.range(1.0, 2.0),
                grb: prop == 2,
                dead: prop == 0 && node == 10,
            });
            index += 1;
        }
    }
    // The first six arch pixels reflected in a puddle below them, dim and blurred.
    let reflections = (0..6)
        .map(|i| (i, lights[i].x + 3.0, lights[i].y + 160.0))
        .collect();
    let props = layout
        .iter()
        .map(|points| PropInput {
            nodes: points.len() as u32,
            expected: points.clone(),
            color_order: "RGB".into(),
        })
        .collect();
    Scene {
        spec: CodeSpec::new(index, Base::Four),
        lights,
        reflections,
        owners,
        props,
    }
}

/// The video frame at `t` (seconds since the sequence started; negative before it), as 8-bit RGB.
fn frame(scene: &Scene, t: f64, gain: f64, shake: (f64, f64), rng: &mut Rng) -> Vec<u8> {
    let mut img = vec![0.0f64; W * H * 3];
    // Ambient: a dim gradient, and a porch light that's always on.
    for y in 0..H {
        for x in 0..W {
            let base = 8.0 + 10.0 * y as f64 / H as f64;
            let porch = 180.0 * (-((x as f64 - 150.0).powi(2) + (y as f64 - 90.0).powi(2)) / 200.0).exp();
            for c in 0..3 {
                img[(y * W + x) * 3 + c] = base + porch * [1.0, 0.8, 0.5][c];
            }
        }
    }
    let mut splat = |x: f64, y: f64, sigma: f64, colour: [f64; 3], gain: f64| {
        let reach = (sigma * 4.0).ceil() as i64;
        let (cx, cy) = (x.round() as i64, y.round() as i64);
        for yy in (cy - reach).max(0)..(cy + reach + 1).min(H as i64) {
            for xx in (cx - reach).max(0)..(cx + reach + 1).min(W as i64) {
                let d2 = (xx as f64 - x).powi(2) + (yy as f64 - y).powi(2);
                let f = gain * (-d2 / (2.0 * sigma * sigma)).exp();
                for c in 0..3 {
                    img[(yy as usize * W + xx as usize) * 3 + c] += colour[c] * f;
                }
            }
        }
    };
    let symbol = |light: &Light| {
        if t < 0.0 || light.dead {
            Symbol::Off
        } else {
            scene.spec.symbol_at(t as f32, light.index)
        }
    };
    for light in &scene.lights {
        let colour = camera_colour(symbol(light), light.grb);
        if colour != [0.0; 3] {
            splat(
                light.x + shake.0,
                light.y + shake.1,
                light.sigma,
                colour,
                gain * light.gain * 1.6,
            );
        }
    }
    for &(i, x, y) in &scene.reflections {
        let light = &scene.lights[i];
        let colour = camera_colour(symbol(light), light.grb);
        if colour != [0.0; 3] {
            splat(
                x + shake.0,
                y + shake.1,
                light.sigma * 1.8,
                colour,
                gain * light.gain * 0.35,
            );
        }
    }
    img.iter()
        .map(|v| (v + 3.0 * rng.normal()).round().clamp(0.0, 255.0) as u8)
        .collect()
}

struct Capture {
    images: Vec<Image>,
    start: f64,
}

/// Films the scene: the video starts `lead` seconds before the sequence, each frame with a
/// random wobble and the phone's exposure drifting; sync is found from frame brightness, and each
/// slot's middle frames are averaged, as the app does.
fn film(scene: &Scene, lead: f64, rng: &mut Rng) -> Capture {
    let length = lead + f64::from(scene.spec.duration()) + 1.0;
    let frames = (length * FPS) as usize;
    let exposure = |t: f64| 1.0 + 0.2 * (t * 0.7).sin();
    // Brightness for sync, from the lights (rendering every frame in full would be slow).
    let samples: Vec<Sample> = (0..frames)
        .map(|i| {
            let t = i as f64 / FPS;
            let since = t - lead;
            let lit: f64 = scene
                .lights
                .iter()
                .filter(|l| !l.dead && since >= 0.0)
                .map(|l| {
                    let c = camera_colour(scene.spec.symbol_at(since as f32, l.index), l.grb);
                    l.gain * (c[0] + c[1] + c[2]) * l.sigma * l.sigma
                })
                .sum();
            Sample {
                t,
                v: (12.0 + exposure(t) * lit / (W * H) as f64 + 0.01 * rng.normal()) as f32,
            }
        })
        .collect();
    let sync = find_sync(&samples, &scene.spec).expect("sync found");
    let images = slot_windows(sync.start, &scene.spec)
        .iter()
        .map(|&(a, b)| {
            let picked: Vec<f64> = (0..frames)
                .map(|i| i as f64 / FPS)
                .filter(|t| *t >= a && *t <= b)
                .collect();
            let mut sum = vec![0.0f32; W * H * 3];
            for &t in &picked {
                let shake = (1.2 * rng.normal(), 1.2 * rng.normal());
                let bytes = frame(scene, t - lead, exposure(t), shake, rng);
                for (s, b) in sum.iter_mut().zip(bytes) {
                    *s += f32::from(b);
                }
            }
            // The window sends 8-bit averages.
            let n = picked.len().max(1) as f32;
            Image {
                width: W,
                height: H,
                rgb: sum.iter().map(|s| (s / n).round()).collect(),
            }
        })
        .collect();
    Capture {
        images,
        start: sync.start,
    }
}

#[test]
fn decodes_a_synthetic_phone_video_within_two_pixels_at_1080p_and_flags_the_faults() {
    let mut rng = Rng(0x5eed_cafe);
    let scene = scene(&mut rng);
    let lead = 2.37;
    let capture = film(&scene, lead, &mut rng);
    assert!((capture.start - lead).abs() < 0.1, "sync at {}", capture.start);

    let decoded = decode(&capture.images, &scene.spec).unwrap();
    let total = scene.lights.len();
    let mut squared = 0.0;
    let mut wrong = 0;
    for f in &decoded.pixels {
        let light = &scene.lights[f.index as usize];
        let d2 = (f.x - light.x).powi(2) + (f.y - light.y).powi(2);
        if d2 > 9.0 {
            wrong += 1;
        }
        squared += d2;
    }
    let rms_1080 = (squared / decoded.pixels.len() as f64).sqrt() * 1080.0 / H as f64;
    println!(
        "found {}/{} live pixels, {} misread, RMS {:.2} px at 1080p, {} duplicates, {} unreadable",
        decoded.pixels.len(),
        total - 1,
        wrong,
        rms_1080,
        decoded.duplicates.len(),
        decoded.unreadable.len()
    );
    assert_eq!(wrong, 0);
    assert!(
        decoded.pixels.len() >= total - 1 - 2,
        "found {}",
        decoded.pixels.len()
    );
    assert!(rms_1080 < 2.0, "RMS {rms_1080:.2} px at 1080p");

    let result = plan(&decoded, &scene.owners, &scene.props, &[]);
    let has = |want: &dyn Fn(&Anomaly) -> bool| result.anomalies.iter().any(want);
    assert!(
        has(&|a| matches!(a, Anomaly::Missing { prop: 0, ranges } if ranges.contains(&[10, 10]))),
        "{:?}",
        result.anomalies
    );
    let reflected = result
        .anomalies
        .iter()
        .filter(|a| matches!(a, Anomaly::Duplicate { prop: 0, node, .. } if *node < 6))
        .count();
    assert!(reflected >= 5, "reflections flagged: {reflected}");
    assert!(
        has(&|a| *a == Anomaly::Reversed { prop: 2 }),
        "{:?}",
        result.anomalies
    );
    assert!(!has(&|a| matches!(a, Anomaly::Reversed { prop: 0 | 1 })));
    assert!(has(
        &|a| matches!(a, Anomaly::ColorOrder { prop: 2, suggested, .. } if suggested == "GRB")
    ));
    assert!(!has(&|a| matches!(a, Anomaly::ColorOrder { prop: 0 | 1, .. })));
    assert!(
        has(&|a| matches!(
            a,
            Anomaly::Jump {
                prop: 1,
                node: 40 | 41 | 47 | 48
            }
        )),
        "{:?}",
        result.anomalies
    );

    // Lined up with the layout: the arch lands where it is, within a few hundredths of a unit.
    let arch = &result.props[0];
    let error: f64 = arch
        .points
        .iter()
        .zip(&scene.props[0].expected)
        .enumerate()
        .filter(|(i, _)| arch.measured[*i])
        .map(|(_, (p, e))| ((p[0] - e[0]).powi(2) + (p[1] - e[1]).powi(2)).sqrt())
        .fold(0.0, f64::max);
    assert!(error < 0.1, "arch off by up to {error:.3} units");
    assert!(arch.fit.unwrap().fits);
}

#[test]
fn binary_codes_decode_too() {
    let mut rng = Rng(42);
    let mut scene = scene(&mut rng);
    scene.spec = CodeSpec::new(scene.spec.pixels, Base::Two);
    scene.reflections.clear();
    let capture = film(&scene, 0.8, &mut rng);
    let decoded = decode(&capture.images, &scene.spec).unwrap();
    let misread = decoded
        .pixels
        .iter()
        .filter(|f| {
            let l = &scene.lights[f.index as usize];
            (f.x - l.x).hypot(f.y - l.y) > 3.0
        })
        .count();
    assert_eq!(misread, 0);
    assert!(
        decoded.pixels.len() >= scene.lights.len() - 3,
        "found {}",
        decoded.pixels.len()
    );
}
