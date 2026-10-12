//! The made-up corpus: an xLights show folder and three short sequences written here, for CI to
//! hold imports to. Everything in it is PixelFlow's own (made-up props, made-up sequences, a
//! picture drawn here, and one of PixelFlow's own shader files), so it can be published with
//! the code. No music: every sequence is an animation.
//!
//! [`files`] gives every file of `crates/pf-fidelity/corpus` that is made here, and
//! `pf-fidelity corpus-files` writes them. The `.fseq` references are not made here: only
//! xLights can make them (`corpus/README.md` says how), and the baseline and fingerprint files
//! are written by the checks themselves (`crate::regress`, `crate::fingerprint`).
//!
//! The show (1,091 pixels, 3,273 channels on one DDP controller at a documentation address):
//!
//! | Prop | Kind | Pixels |
//! |---|---|---|
//! | Matrix | horizontal matrix, 32 by 16 | 512 |
//! | Tree | 360° tree, 12 strings of 25 | 300 |
//! | Arches | three arches of 20, with a submodel "Middle Arch" | 60 |
//! | Line | a single line (an outline) | 80 |
//! | Canes | four candy canes of 15 | 60 |
//! | Star | a five-pointed star | 50 |
//! | Flake | a custom model (a snowflake on a 9 by 9 grid) | 29 |
//!
//! and two groups: Outlines (Line, Arches, Canes, Star) and Everything.

use std::fmt::Write as _;

/// The corpus folder, from the crate's.
pub const CORPUS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/corpus");

/// The xLights version the sequences say they were saved by: current, so that neither xLights
/// nor the importer changes any setting for an older version.
const VERSION: &str = "2026.14";

/// The picture the Pictures effects show, in the show folder.
pub const PICTURE: &str = "show/pictures/corpus-picture.png";
/// The shader file the Shader effects run: PixelFlow's own Prism, in the show's `Shaders`.
pub const SHADER: &str = "show/Shaders/prism.fs";
const PRISM: &str = include_str!("../../pf-shader/shaders/prism.fs");

/// One prop of the layout.
struct Model {
    name: &'static str,
    /// Everything but the name and start channel, as xLights writes it.
    attributes: &'static str,
    pixels: u32,
    /// What goes inside the element (submodels).
    inside: &'static str,
}

const MODELS: [Model; 7] = [
    Model {
        name: "Matrix",
        attributes: r#"DisplayAs="Horiz Matrix" StringType="RGB Nodes" StartSide="B" Dir="L" parm1="16" parm2="32" parm3="1" WorldPosX="640.0" WorldPosY="380.0" WorldPosZ="0.0" ScaleX="8.0" ScaleY="8.0" ScaleZ="1.0""#,
        pixels: 512,
        inside: "",
    },
    Model {
        name: "Tree",
        attributes: r#"DisplayAs="Tree 360" StringType="RGB Nodes" StartSide="B" Dir="L" parm1="12" parm2="25" parm3="1" WorldPosX="230.0" WorldPosY="380.0" WorldPosZ="0.0" ScaleX="4.0" ScaleY="3.0" ScaleZ="4.0""#,
        pixels: 300,
        inside: "",
    },
    Model {
        name: "Arches",
        attributes: r#"DisplayAs="Arches" StringType="RGB Nodes" StartSide="B" Dir="L" parm1="3" parm2="20" parm3="1" Arc="180" WorldPosX="80.0" WorldPosY="90.0" WorldPosZ="0.0" X2="420.0" Y2="0.0" Z2="0.0" Height="1.0""#,
        pixels: 60,
        inside: r#"      <subModel name="Middle Arch" bufferstyle="Default" layout="horizontal" type="ranges" line0="21-40"/>
"#,
    },
    Model {
        name: "Line",
        attributes: r#"DisplayAs="Single Line" StringType="RGB Nodes" StartSide="B" Dir="L" parm1="1" parm2="80" parm3="1" WorldPosX="80.0" WorldPosY="660.0" WorldPosZ="0.0" X2="1120.0" Y2="0.0" Z2="0.0""#,
        pixels: 80,
        inside: "",
    },
    Model {
        name: "Canes",
        attributes: r#"DisplayAs="Candy Canes" StringType="RGB Nodes" StartSide="B" Dir="L" parm1="4" parm2="15" parm3="1" WorldPosX="760.0" WorldPosY="90.0" WorldPosZ="0.0" X2="440.0" Y2="0.0" Z2="0.0" Height="1.0""#,
        pixels: 60,
        inside: "",
    },
    Model {
        name: "Star",
        attributes: r#"DisplayAs="Star" StringType="RGB Nodes" StartSide="B" Dir="L" parm1="1" parm2="50" parm3="5" WorldPosX="1080.0" WorldPosY="420.0" WorldPosZ="0.0" ScaleX="6.0" ScaleY="6.0" ScaleZ="1.0""#,
        pixels: 50,
        inside: "",
    },
    Model {
        name: "Flake",
        attributes: concat!(
            r#"DisplayAs="Custom" StringType="RGB Nodes" parm1="9" parm2="9" WorldPosX="1080.0" WorldPosY="230.0" WorldPosZ="0.0" ScaleX="10.0" ScaleY="10.0" ScaleZ="1.0" CustomModel=""#,
            ",,,,1,,,,;,2,,,3,,,4,;,,5,,6,,7,,;,,,8,9,10,,,;11,12,13,14,15,16,17,18,19;",
            ",,,20,21,22,,,;,,23,,24,,25,,;,26,,,27,,,28,;,,,,29,,,,",
            r#"""#
        ),
        pixels: 29,
        inside: "",
    },
];

/// The controller's channels: exactly every model's, so that the `.fseq` xLights writes ends
/// where the last prop does.
const CHANNELS: u32 = 3_273;

fn layout() -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!-- PixelFlow's made-up fidelity corpus (crates/pf-fidelity/src/madeup.rs writes this\n     \
         file): seven props of the common kinds on one controller, and two groups. -->\n\
         <xrgb>\n  <models type=\"rgb_effects\">\n",
    );
    let mut start = 1;
    for model in &MODELS {
        let _ = writeln!(
            xml,
            "    <model name=\"{}\" {} StartChannel=\"{start}\" LayoutGroup=\"Default\" versionNumber=\"8\"{}",
            model.name,
            model.attributes,
            if model.inside.is_empty() { "/>" } else { ">" }
        );
        if !model.inside.is_empty() {
            xml.push_str(model.inside);
            xml.push_str("    </model>\n");
        }
        start += model.pixels * 3;
    }
    xml.push_str(
        "  </models>\n  <modelGroups type=\"rgb_effects\">\n    \
         <modelGroup name=\"Outlines\" models=\"Line,Arches,Canes,Star\" layout=\"minimalGrid\" GridSize=\"400\" LayoutGroup=\"Default\"/>\n    \
         <modelGroup name=\"Everything\" models=\"Matrix,Tree,Arches,Line,Canes,Star,Flake\" layout=\"minimalGrid\" GridSize=\"400\" LayoutGroup=\"Default\"/>\n  \
         </modelGroups>\n  <settings>\n    <previewWidth value=\"1280\"/>\n    <previewHeight value=\"720\"/>\n  </settings>\n</xrgb>\n",
    );
    xml
}

fn networks() -> String {
    // 192.0.2.0/24 is set aside for documentation: nothing answers there.
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!-- PixelFlow's made-up fidelity corpus: one DDP controller at a documentation address\n     \
         (192.0.2.0/24, where nothing answers). -->\n\
         <Networks>\n  <Controller Id=\"1\" Name=\"Main\" Type=\"Ethernet\" IP=\"192.0.2.40\" Protocol=\"DDP\" ActiveState=\"Active\" AutoLayout=\"0\" AutoSize=\"0\" AutoUpload=\"0\">\n    \
         <network NetworkType=\"DDP\" ComPort=\"192.0.2.40\" BaudRate=\"1\" MaxChannels=\"{CHANNELS}\" KeepChannelNumbers=\"0\" ChannelsPerPacket=\"1440\"/>\n  \
         </Controller>\n</Networks>\n"
    )
}

/// A palette: its colors, each checked, and anything else of the color panel (`extra`).
fn palette(colors: &[&str], extra: &str) -> String {
    let mut text = String::new();
    for (i, color) in colors.iter().enumerate() {
        let _ = write!(text, "C_BUTTON_Palette{}={color},", i + 1);
    }
    for i in 0..colors.len() {
        let _ = write!(text, "C_CHECKBOX_Palette{}=1,", i + 1);
    }
    text.push_str(extra);
    text.trim_end_matches(',').to_string()
}

const RED_WHITE: &[&str] = &["#FF0000", "#FFFFFF"];
const BLUE_GREEN: &[&str] = &["#0040FF", "#00FF40"];
const WARM: &[&str] = &["#FF0000", "#FF8000", "#FFFF00"];
const COOL: &[&str] = &["#00FFFF", "#0000FF", "#FF00FF"];
const RED_GREEN: &[&str] = &["#FF0000", "#00FF00"];
const WHITE: &[&str] = &["#FFFFFF"];
const GOLD: &[&str] = &["#FFB000"];
const PURPLE: &[&str] = &["#8000FF"];

/// One effect on a row.
struct Fx {
    name: &'static str,
    start: u32,
    end: u32,
    settings: String,
    palette: String,
}

fn fx(name: &'static str, start: u32, end: u32, settings: &str, palette: String) -> Fx {
    Fx {
        name,
        start,
        end,
        settings: settings.to_string(),
        palette,
    }
}

/// A row: a model or group, its layers (the first is on top, as xLights lists them), and its
/// submodels' layers.
struct Row {
    name: &'static str,
    layers: Vec<Vec<Fx>>,
    submodels: Vec<(&'static str, Vec<Fx>)>,
}

fn row(name: &'static str, layers: Vec<Vec<Fx>>) -> Row {
    Row {
        name,
        layers,
        submodels: Vec::new(),
    }
}

/// A timing track's mark: start and end (milliseconds), and its label.
type Mark = (u32, u32, String);

/// A sequence: an animation (no music) of `duration_ms` in frames of `frame_ms`.
struct Xsq {
    title: &'static str,
    about: &'static str,
    frame_ms: u32,
    duration_ms: u32,
    /// Timing tracks: a name and its marks.
    timing: Vec<(&'static str, Vec<Mark>)>,
    rows: Vec<Row>,
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

impl Xsq {
    fn xml(&self) -> String {
        let mut palettes: Vec<&str> = Vec::new();
        let mut settings: Vec<&str> = Vec::new();
        let all = self.rows.iter().flat_map(|r| {
            r.layers
                .iter()
                .flatten()
                .chain(r.submodels.iter().flat_map(|(_, effects)| effects))
        });
        for effect in all {
            if !palettes.contains(&effect.palette.as_str()) {
                palettes.push(&effect.palette);
            }
            if !settings.contains(&effect.settings.as_str()) {
                settings.push(&effect.settings);
            }
        }
        let place = |list: &[&str], item: &str| list.iter().position(|x| *x == item).unwrap_or(0);
        let effect = |xml: &mut String, indent: &str, e: &Fx| {
            let _ = writeln!(
                xml,
                "{indent}<Effect ref=\"{}\" name=\"{}\" palette=\"{}\" startTime=\"{}\" endTime=\"{}\"/>",
                place(&settings, &e.settings),
                e.name,
                place(&palettes, &e.palette),
                e.start,
                e.end
            );
        };

        let mut xml = String::new();
        let _ = write!(
            xml,
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <!-- PixelFlow's made-up fidelity corpus (crates/pf-fidelity/src/madeup.rs writes this file).\n     \
             {}\n     \
             Render it in xLights on this show folder and save the .fseq beside it: see ../README.md. -->\n\
             <xsequence BaseChannel=\"0\" ChanCtrlBasic=\"0\" ChanCtrlColor=\"0\" FixedPointTiming=\"1\" ModelBlending=\"true\">\n  \
             <head>\n    <version>{VERSION}</version>\n    <author>PixelFlow</author>\n    <song>{}</song>\n    \
             <sequenceTiming>{} ms</sequenceTiming>\n    <sequenceType>Animation</sequenceType>\n    \
             <sequenceDuration>{}.{:03}</sequenceDuration>\n  </head>\n",
            self.about,
            escape(self.title),
            self.frame_ms,
            self.duration_ms / 1000,
            self.duration_ms % 1000
        );
        xml.push_str("  <ColorPalettes>\n");
        for p in &palettes {
            let _ = writeln!(xml, "    <ColorPalette>{}</ColorPalette>", escape(p));
        }
        xml.push_str("  </ColorPalettes>\n  <EffectDB>\n");
        for s in &settings {
            let _ = writeln!(xml, "    <Effect>{}</Effect>", escape(s));
        }
        xml.push_str("  </EffectDB>\n  <DisplayElements>\n");
        for (name, _) in &self.timing {
            let _ = writeln!(
                xml,
                "    <Element collapsed=\"0\" type=\"timing\" name=\"{name}\" visible=\"1\" views=\"\" active=\"1\"/>"
            );
        }
        for r in &self.rows {
            let _ = writeln!(
                xml,
                "    <Element collapsed=\"0\" type=\"model\" name=\"{}\" visible=\"1\"/>",
                r.name
            );
        }
        xml.push_str("  </DisplayElements>\n  <ElementEffects>\n");
        for (name, marks) in &self.timing {
            let _ = writeln!(
                xml,
                "    <Element type=\"timing\" name=\"{name}\">\n      <EffectLayer>"
            );
            for (start, end, label) in marks {
                let _ = writeln!(
                    xml,
                    "        <Effect label=\"{}\" startTime=\"{start}\" endTime=\"{end}\"/>",
                    escape(label)
                );
            }
            xml.push_str("      </EffectLayer>\n    </Element>\n");
        }
        for r in &self.rows {
            let _ = writeln!(xml, "    <Element type=\"model\" name=\"{}\">", r.name);
            for layer in &r.layers {
                xml.push_str("      <EffectLayer>\n");
                for e in layer {
                    effect(&mut xml, "        ", e);
                }
                xml.push_str("      </EffectLayer>\n");
            }
            for (name, effects) in &r.submodels {
                let _ = writeln!(xml, "      <SubModelEffectLayer name=\"{name}\">");
                for e in effects {
                    effect(&mut xml, "        ", e);
                }
                xml.push_str("      </SubModelEffectLayer>\n");
            }
            xml.push_str("    </Element>\n");
        }
        xml.push_str("  </ElementEffects>\n  <lastView>0</lastView>\n</xsequence>\n");
        xml
    }
}

/// Spirals: `count` arms per color, `rotation` cells of lean, `movement` turns over the effect.
fn spirals(count: u32, rotation: i32, thickness: u32, movement: &str) -> String {
    format!(
        "E_CHECKBOX_Spirals_3D=0,E_CHECKBOX_Spirals_Blend=0,E_CHECKBOX_Spirals_Grow=0,\
         E_CHECKBOX_Spirals_Shrink=0,E_SLIDER_Spirals_Count={count},E_SLIDER_Spirals_Rotation={rotation},\
         E_SLIDER_Spirals_Thickness={thickness},E_TEXTCTRL_Spirals_Movement={movement}"
    )
}

fn twinkle(count: u32, steps: u32) -> String {
    format!(
        "E_CHECKBOX_Twinkle_ReRandom=0,E_CHECKBOX_Twinkle_Strobe=0,E_CHOICE_Twinkle_Style=New Render Method,\
         E_SLIDER_Twinkle_Count={count},E_SLIDER_Twinkle_Steps={steps}"
    )
}

const ON: &str =
    "E_CHECKBOX_On_Shimmer=0,E_TEXTCTRL_Eff_On_End=100,E_TEXTCTRL_Eff_On_Start=100,E_TEXTCTRL_On_Cycles=1.0";

fn color_wash(cycles: &str) -> String {
    format!(
        "E_CHECKBOX_ColorWash_CircularPalette=0,E_CHECKBOX_ColorWash_HFade=0,E_CHECKBOX_ColorWash_Shimmer=0,\
         E_CHECKBOX_ColorWash_VFade=0,E_TEXTCTRL_ColorWash_Cycles={cycles}"
    )
}

fn bars(direction: &str, count: u32, cycles: &str) -> String {
    format!(
        "E_CHECKBOX_Bars_3D=0,E_CHECKBOX_Bars_Gradient=0,E_CHECKBOX_Bars_Highlight=0,\
         E_CHOICE_Bars_Direction={direction},E_SLIDER_Bars_BarCount={count},E_TEXTCTRL_Bars_Cycles={cycles}"
    )
}

fn shockwave(cycles: u32, end_radius: u32, extra: &str) -> String {
    format!(
        "E_CHECKBOX_Shockwave_Blend_Edges=1,E_CHECKBOX_Shockwave_Scale=1,E_SLIDER_Shockwave_Accel=0,\
         E_SLIDER_Shockwave_CenterX=50,E_SLIDER_Shockwave_CenterY=50,E_SLIDER_Shockwave_Cycles={cycles},\
         E_SLIDER_Shockwave_End_Radius={end_radius},E_SLIDER_Shockwave_End_Width=8,\
         E_SLIDER_Shockwave_Start_Radius=1,E_SLIDER_Shockwave_Start_Width=4{extra}"
    )
}

fn shape(object: &str, count: u32, extra: &str) -> String {
    format!(
        "E_CHECKBOX_Shape_FadeAway=1,E_CHECKBOX_Shape_HoldColour=1,E_CHECKBOX_Shape_RandomInitial=1,\
         E_CHECKBOX_Shape_RandomLocation=1,E_CHECKBOX_Shapes_RandomMovement=0,E_CHOICE_Shape_ObjectToDraw={object},\
         E_SLIDER_Shape_Growth=10,E_SLIDER_Shape_Lifetime=5,E_SLIDER_Shape_StartSize=1,\
         E_SLIDER_Shape_Thickness=2,E_TEXTCTRL_Shape_Count={count}{extra}"
    )
}

/// Fill rising: its position from 0 to 100 over the effect.
const FILL: &str = "E_CHECKBOX_Fill_Offset_In_Pixels=1,E_CHECKBOX_Fill_Wrap=1,E_CHOICE_Fill_Direction=Up,\
     E_SLIDER_Fill_Band_Size=0,E_SLIDER_Fill_Offset=0,E_SLIDER_Fill_Position=0,E_SLIDER_Fill_Skip_Size=0,\
     E_VALUECURVE_Fill_Position=Active=TRUE|Id=ID_VALUECURVE_Fill_Position|Type=Ramp|Min=0.00|Max=100.00|P1=0.00|P2=100.00|RV=TRUE|";

fn pictures(direction: &str, scaling: &str, speed: &str) -> String {
    format!(
        "E_CHECKBOX_LoopGIF=0,E_CHECKBOX_Pictures_PixelOffsets=0,E_CHECKBOX_Pictures_Shimmer=0,\
         E_CHECKBOX_Pictures_TransparentBlack=0,E_CHECKBOX_Pictures_WrapX=0,E_CHECKBOX_SuppressGIFBackground=1,\
         E_CHOICE_Pictures_Direction={direction},E_CHOICE_Scaling={scaling},\
         E_FILEPICKERCTRL_Pictures_Filename=pictures/corpus-picture.png,E_SLIDER_PicturesEndXC=0,\
         E_SLIDER_PicturesEndYC=0,E_SLIDER_PicturesXC=0,E_SLIDER_PicturesYC=0,\
         E_TEXTCTRL_Pictures_FrameRateAdj=1.0,E_TEXTCTRL_Pictures_Speed={speed}"
    )
}

fn shader(path: &str, speed: u32) -> String {
    format!(
        "E_0FILEPICKERCTRL_IFS=Shaders/prism.fs,E_CHOICE_SHADERXYZZY_path={path},E_SLIDER_SHADERXYZZY_rainbows=150,\
         E_SLIDER_SHADERXYZZY_speed={speed},E_SLIDER_Shader_Speed=100,E_TEXTCTRL_Shader_LeadIn=0"
    )
}

/// "Washes and shapes": the effects that keep time, on every kind of prop, with layers and their
/// blends, fades, value curves, a group drawn per prop, and a submodel.
fn washes() -> Xsq {
    let matrix = row(
        "Matrix",
        vec![vec![
            fx(
                "Shockwave",
                0,
                4000,
                &shockwave(2, 60, ""),
                palette(RED_WHITE, ""),
            ),
            fx(
                "Color Wash",
                4000,
                8000,
                &format!(
                    "{},T_TEXTCTRL_Fadein=1.00,T_TEXTCTRL_Fadeout=1.00",
                    color_wash("1.0")
                ),
                palette(BLUE_GREEN, ""),
            ),
            fx(
                "Plasma",
                8000,
                12000,
                "B_SLIDER_Blur=1,B_VALUECURVE_Blur=Active=TRUE|Type=Ramp|Min=1.00|Max=15.00|P1=1.00|P2=6.00|RV=TRUE|,\
                 E_CHOICE_Plasma_Color=Normal,E_SLIDER_Plasma_Line_Density=1,E_SLIDER_Plasma_Speed=10,E_SLIDER_Plasma_Style=2",
                palette(COOL, ""),
            ),
            fx(
                "Fan",
                12000,
                16000,
                "E_CHECKBOX_Fan_Blend_Edges=1,E_CHECKBOX_Fan_Reverse=0,E_CHECKBOX_Fan_Scale=1,E_SLIDER_Fan_Accel=0,\
                 E_SLIDER_Fan_Blade_Angle=90,E_SLIDER_Fan_Blade_Width=50,E_SLIDER_Fan_CenterX=50,E_SLIDER_Fan_CenterY=50,\
                 E_SLIDER_Fan_Duration=80,E_SLIDER_Fan_Element_Width=100,E_SLIDER_Fan_End_Radius=50,\
                 E_SLIDER_Fan_Num_Blades=3,E_SLIDER_Fan_Num_Elements=1,E_SLIDER_Fan_Revolutions=720,\
                 E_SLIDER_Fan_Start_Angle=0,E_SLIDER_Fan_Start_Radius=1,\
                 E_VALUECURVE_Fan_Revolutions=Active=TRUE|Id=ID_VALUECURVE_Fan_Revolutions|Type=Ramp|Min=0.00|Max=3600.00|P1=360.00|P2=1080.00|RV=TRUE|",
                palette(WARM, ""),
            ),
            fx(
                "Pinwheel",
                16000,
                20000,
                "E_CHECKBOX_Pinwheel_Rotation=1,E_CHOICE_Pinwheel_3D=None,E_CHOICE_Pinwheel_Style=New Render Method,\
                 E_SLIDER_PinwheelXC=0,E_SLIDER_PinwheelYC=0,E_SLIDER_Pinwheel_ArmSize=100,E_SLIDER_Pinwheel_Arms=4,\
                 E_SLIDER_Pinwheel_Offset=0,E_SLIDER_Pinwheel_Speed=10,E_SLIDER_Pinwheel_Thickness=30,E_SLIDER_Pinwheel_Twist=0",
                palette(RED_GREEN, ""),
            ),
            fx(
                "Spirals",
                20000,
                24000,
                &spirals(2, 20, 50, "2.0"),
                palette(RED_GREEN, ""),
            ),
        ]],
    );
    let tree = row(
        "Tree",
        vec![
            vec![
                fx(
                    "Twinkle",
                    0,
                    6000,
                    &format!("{},T_CHOICE_LayerMethod=Additive", twinkle(10, 30)),
                    palette(WHITE, ""),
                ),
                fx(
                    "On",
                    12000,
                    18000,
                    &format!("{ON},T_CHOICE_LayerMethod=Average"),
                    palette(PURPLE, ""),
                ),
                fx(
                    "Color Wash",
                    18000,
                    24000,
                    &format!("{},T_CHOICE_LayerMethod=Max", color_wash("2.0")),
                    palette(BLUE_GREEN, ""),
                ),
            ],
            vec![
                fx(
                    "Spirals",
                    0,
                    6000,
                    &spirals(1, 20, 40, "1.5"),
                    palette(RED_WHITE, ""),
                ),
                fx(
                    "Butterfly",
                    6000,
                    12000,
                    "E_CHOICE_Butterfly_Colors=Palette,E_CHOICE_Butterfly_Direction=Normal,E_SLIDER_Butterfly_Chunks=1,\
                     E_SLIDER_Butterfly_Skip=2,E_SLIDER_Butterfly_Speed=10,E_SLIDER_Butterfly_Style=1,\
                     E_VALUECURVE_Butterfly_Speed=Active=TRUE|Id=ID_VALUECURVE_Butterfly_Speed|Type=Ramp|Min=0.00|Max=100.00|P1=10.00|P2=30.00|RV=TRUE|",
                    palette(WARM, ""),
                ),
                fx(
                    "Morph",
                    12000,
                    18000,
                    "E_CHECKBOX_Morph_End_Link=0,E_CHECKBOX_Morph_Start_Link=0,E_CHECKBOX_ShowHeadAtStart=0,\
                     E_SLIDER_MorphAccel=0,E_SLIDER_MorphDuration=20,E_SLIDER_MorphEndLength=1,E_SLIDER_MorphStartLength=1,\
                     E_SLIDER_Morph_End_X1=0,E_SLIDER_Morph_End_X2=100,E_SLIDER_Morph_End_Y1=100,E_SLIDER_Morph_End_Y2=100,\
                     E_SLIDER_Morph_Repeat_Count=3,E_SLIDER_Morph_Repeat_Skip=4,E_SLIDER_Morph_Stagger=0,\
                     E_SLIDER_Morph_Start_X1=0,E_SLIDER_Morph_Start_X2=100,E_SLIDER_Morph_Start_Y1=0,E_SLIDER_Morph_Start_Y2=0",
                    palette(COOL, ""),
                ),
                fx("Bars", 18000, 24000, &bars("up", 2, "2.0"), palette(WARM, "")),
            ],
        ],
    );
    let outlines = row(
        "Outlines",
        vec![vec![
            fx(
                "Color Wash",
                0,
                6000,
                &format!("B_CHOICE_BufferStyle=Per Model Default,{}", color_wash("1.0")),
                palette(RED_GREEN, ""),
            ),
            fx("Bars", 6000, 12000, &bars("Left", 1, "2.0"), palette(COOL, "")),
            fx(
                "Fill",
                12000,
                18000,
                &format!("B_CHOICE_BufferStyle=Per Model Default,{FILL}"),
                palette(GOLD, ""),
            ),
            fx(
                "On",
                18000,
                24000,
                "E_CHECKBOX_On_Shimmer=0,E_TEXTCTRL_Eff_On_End=0,E_TEXTCTRL_Eff_On_Start=100,E_TEXTCTRL_On_Cycles=1.0,\
                 T_TEXTCTRL_Fadein=0.50",
                palette(RED_WHITE, ""),
            ),
        ]],
    );
    let mut arches = row("Arches", Vec::new());
    arches
        .submodels
        .push(("Middle Arch", vec![fx("On", 0, 12000, ON, palette(PURPLE, ""))]));
    let flake = row(
        "Flake",
        vec![vec![fx(
            "On",
            0,
            24000,
            ON,
            palette(GOLD, "C_SLIDER_SparkleFrequency=100"),
        )]],
    );
    Xsq {
        title: "Washes and shapes",
        about: "Washes and shapes: effects that keep time on every kind of prop, layers and blends,\n     \
                fades, value curves, a group drawn per prop, and a submodel. 24 seconds of 50 ms frames.",
        frame_ms: 50,
        duration_ms: 24_000,
        timing: Vec::new(),
        rows: vec![matrix, tree, outlines, arches, flake],
    }
}

/// "Chance": the effects that draw on chance (snow, fireworks, circles, shapes, fire, twinkles,
/// sparkles), at 25 ms frames.
fn chance() -> Xsq {
    let matrix = row(
        "Matrix",
        vec![vec![
            fx(
                "Snowflakes",
                0,
                5000,
                "E_CHOICE_Falling=Falling,E_SLIDER_Snowflakes_Count=12,E_SLIDER_Snowflakes_Speed=10,E_SLIDER_Snowflakes_Type=3",
                palette(WHITE, ""),
            ),
            fx(
                "Fireworks",
                5000,
                12000,
                "E_CHECKBOX_FIRETIMING=0,E_CHECKBOX_Fireworks_Gravity=1,E_CHECKBOX_Fireworks_HoldColour=1,\
                 E_CHECKBOX_Fireworks_UseMusic=0,E_SLIDER_Fireworks_Count=50,E_SLIDER_Fireworks_Explosions=30,\
                 E_SLIDER_Fireworks_Fade=50,E_SLIDER_Fireworks_Sensitivity=50,E_SLIDER_Fireworks_Velocity=2,\
                 E_SLIDER_Fireworks_XLocation=-1,E_SLIDER_Fireworks_XVelocity=0,E_SLIDER_Fireworks_YLocation=-1,\
                 E_SLIDER_Fireworks_YVelocity=0",
                palette(&["#FFFFFF", "#FF00FF", "#FF0000"], ""),
            ),
            fx(
                "Circles",
                12000,
                16000,
                "E_CHECKBOX_Circles_Bounce=1,E_CHECKBOX_Circles_Collide=0,E_CHECKBOX_Circles_Linear_Fade=0,\
                 E_CHECKBOX_Circles_Plasma=0,E_CHECKBOX_Circles_Radial=0,E_CHECKBOX_Circles_Radial_3D=0,\
                 E_CHECKBOX_Circles_Random_m=0,E_SLIDER_Circles_Count=4,E_SLIDER_Circles_Size=4,E_SLIDER_Circles_Speed=10",
                palette(COOL, ""),
            ),
            fx("Shape", 16000, 20000, &shape("Star", 6, ""), palette(WARM, "")),
        ]],
    );
    let tree = row(
        "Tree",
        vec![vec![
            fx(
                "Fire",
                0,
                10000,
                "E_CHECKBOX_Fire_GrowWithMusic=0,E_CHOICE_Fire_Location=Bottom,E_SLIDER_Fire_Height=60,\
                 E_SLIDER_Fire_HueShift=0,E_TEXTCTRL_Fire_GrowthCycles=0.0",
                palette(WARM, ""),
            ),
            fx("Twinkle", 10000, 20000, &twinkle(30, 30), palette(RED_GREEN, "")),
        ]],
    );
    let outlines = row(
        "Outlines",
        vec![vec![
            fx("Twinkle", 0, 10000, &twinkle(20, 40), palette(WHITE, "")),
            fx(
                "On",
                10000,
                20000,
                ON,
                palette(BLUE_GREEN, "C_SLIDER_SparkleFrequency=120"),
            ),
        ]],
    );
    let flake = row(
        "Flake",
        vec![vec![fx("Twinkle", 0, 20000, &twinkle(40, 20), palette(GOLD, ""))]],
    );
    Xsq {
        title: "Chance",
        about: "Chance: snow, fireworks, circles, shapes, fire, twinkles, and sparkles, which no two\n     \
                renders draw alike. 20 seconds of 25 ms frames.",
        frame_ms: 25,
        duration_ms: 20_000,
        timing: Vec::new(),
        rows: vec![matrix, tree, outlines, flake],
    }
}

/// "Marks, words, and pictures": a timing track and shapes set off by it, text, a picture
/// standing and scrolling, and a shader file on a group and on a tree.
fn marks() -> Xsq {
    let beats: Vec<(u32, u32, String)> = (0..40).map(|i| (i * 500, (i + 1) * 500, String::new())).collect();
    let matrix = row(
        "Matrix",
        vec![vec![
            fx(
                "Text",
                0,
                5000,
                "E_CHECKBOX_TextToCenter=0,E_CHOICE_Text_Count=none,E_CHOICE_Text_Dir=left,E_CHOICE_Text_Effect=normal,\
                 E_CHOICE_Text_Font=Use OS Fonts,E_FONTPICKER_Text_Font='Arial' 12,E_SLIDER_Text_XStart=0,\
                 E_SLIDER_Text_YStart=0,E_TEXTCTRL_Text=PIXELFLOW,E_TEXTCTRL_Text_Speed=10",
                palette(RED_WHITE, ""),
            ),
            fx(
                "Pictures",
                5000,
                10000,
                &pictures("none", "Scale To Fit", "1.0"),
                palette(WHITE, ""),
            ),
            fx(
                "Shape",
                10000,
                15000,
                &shape(
                    "Circle",
                    5,
                    ",E_CHECKBOX_Shape_FireTiming=1,E_CHOICE_Shape_FireTimingTrack=Beats",
                ),
                palette(COOL, ""),
            ),
            fx(
                "Pictures",
                15000,
                20000,
                &pictures("left", "No Scaling", "4.0"),
                palette(WHITE, ""),
            ),
        ]],
    );
    let tree = row(
        "Tree",
        vec![vec![
            fx(
                "Morph",
                0,
                10000,
                "E_CHECKBOX_Morph_End_Link=0,E_CHECKBOX_Morph_Start_Link=0,E_CHECKBOX_ShowHeadAtStart=0,\
                 E_SLIDER_MorphAccel=0,E_SLIDER_MorphDuration=40,E_SLIDER_MorphEndLength=10,E_SLIDER_MorphStartLength=10,\
                 E_SLIDER_Morph_End_X1=100,E_SLIDER_Morph_End_X2=100,E_SLIDER_Morph_End_Y1=0,E_SLIDER_Morph_End_Y2=100,\
                 E_SLIDER_Morph_Repeat_Count=0,E_SLIDER_Morph_Repeat_Skip=1,E_SLIDER_Morph_Stagger=0,\
                 E_SLIDER_Morph_Start_X1=0,E_SLIDER_Morph_Start_X2=0,E_SLIDER_Morph_Start_Y1=0,E_SLIDER_Morph_Start_Y2=100",
                palette(RED_GREEN, ""),
            ),
            fx("Shader", 10000, 20000, &shader("Up", 100), palette(WHITE, "")),
        ]],
    );
    let outlines = row(
        "Outlines",
        vec![vec![
            fx("Shader", 0, 10000, &shader("Across", 150), palette(WHITE, "")),
            fx(
                "Shockwave",
                10000,
                20000,
                &shockwave(
                    3,
                    40,
                    ",E_VALUECURVE_Shockwave_End_Radius=Active=TRUE|Id=ID_VALUECURVE_Shockwave_End_Radius|Type=Ramp|Min=0.00|Max=750.00|P1=20.00|P2=80.00|RV=TRUE|",
                ),
                palette(BLUE_GREEN, ""),
            ),
        ]],
    );
    let flake = row(
        "Flake",
        vec![vec![fx(
            "Color Wash",
            0,
            20000,
            &color_wash("3.0"),
            palette(WARM, ""),
        )]],
    );
    Xsq {
        title: "Marks, words, and pictures",
        about: "Marks, words, and pictures: a timing track and shapes it sets off, text, a picture\n     \
                standing and scrolling, and a shader file on a group and a tree. 20 seconds of 50 ms frames.",
        frame_ms: 50,
        duration_ms: 20_000,
        timing: vec![("Beats", beats)],
        rows: vec![matrix, tree, outlines, flake],
    }
}

/// The picture: 48 by 16, a rainbow from left to right that darkens toward the bottom, with a
/// white frame and a white diagonal, so that where it is and which way up both show.
fn picture() -> Vec<u8> {
    let (w, h) = (48u32, 16u32);
    let mut image = image::RgbImage::new(w, h);
    for (x, y, px) in image.enumerate_pixels_mut() {
        let edge = x == 0 || y == 0 || x == w - 1 || y == h - 1;
        let diagonal = x / 3 == y;
        *px = if edge || diagonal {
            image::Rgb([255, 255, 255])
        } else {
            // Six sectors of the color wheel across the width, as whole numbers.
            let sector = x * 6 / w;
            let rise = ((x * 6 % w) * 255 / w) as u8;
            let [r, g, b] = match sector {
                0 => [255, rise, 0],
                1 => [255 - rise, 255, 0],
                2 => [0, 255, rise],
                3 => [0, 255 - rise, 255],
                4 => [rise, 0, 255],
                _ => [255, 0, 255 - rise],
            };
            let level = |c: u8| (u32::from(c) * (h - y) / h) as u8;
            image::Rgb([level(r), level(g), level(b)])
        };
    }
    let mut bytes = std::io::Cursor::new(Vec::new());
    // Encoding a picture held in memory into memory can't fail.
    let _ = image.write_to(&mut bytes, image::ImageFormat::Png);
    bytes.into_inner()
}

/// The manifest of the corpus: each sequence with the `.fseq` xLights will render from it.
fn manifest() -> String {
    let mut text = String::from("{ \"sequences\": [\n");
    let entries: Vec<String> = SEQUENCES
        .iter()
        .map(|(file, label)| {
            format!(
                "  {{ \"label\": \"{label}\", \"show\": \"show\", \"xsq\": \"show/{file}.xsq\", \"fseq\": \"show/{file}.fseq\" }}"
            )
        })
        .collect();
    text.push_str(&entries.join(",\n"));
    text.push_str("\n] }\n");
    text
}

/// Each sequence's file name (without `.xsq`) and the label the reports give it.
pub const SEQUENCES: [(&str, &str); 3] = [
    ("washes", "Washes and shapes"),
    ("chance", "Chance"),
    ("marks", "Marks, words, and pictures"),
];

/// Every file of the corpus that is made here, by its path in the corpus folder.
pub fn files() -> Vec<(String, Vec<u8>)> {
    let sequences = [washes(), chance(), marks()];
    let mut files = vec![
        ("manifest.json".to_string(), manifest().into_bytes()),
        ("show/xlights_rgbeffects.xml".to_string(), layout().into_bytes()),
        ("show/xlights_networks.xml".to_string(), networks().into_bytes()),
        (SHADER.to_string(), PRISM.as_bytes().to_vec()),
        (PICTURE.to_string(), picture()),
    ];
    for ((file, _), xsq) in SEQUENCES.iter().zip(&sequences) {
        files.push((format!("show/{file}.xsq"), xsq.xml().into_bytes()));
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// The corpus in the repository is what this module makes: change it here and run
    /// `cargo run -p pf-fidelity -- corpus-files`, never by hand.
    #[test]
    fn the_committed_corpus_is_what_this_module_makes() {
        let dir = Path::new(CORPUS_DIR);
        for (path, made) in files() {
            let committed = std::fs::read(dir.join(&path)).unwrap_or_else(|e| {
                panic!("{path} is missing from the corpus ({e}): run `pf-fidelity corpus-files`")
            });
            if path.ends_with(".png") {
                // The picture's pixels, not the encoder's bytes, which another version may
                // write otherwise.
                let pixels = |bytes: &[u8]| {
                    image::load_from_memory(bytes)
                        .map(|i| i.to_rgb8().into_raw())
                        .ok()
                };
                assert_eq!(
                    pixels(&committed),
                    pixels(&made),
                    "{path} differs: run `pf-fidelity corpus-files`"
                );
            } else {
                assert!(
                    committed == made,
                    "{path} differs from what madeup.rs makes: run `pf-fidelity corpus-files`"
                );
            }
        }
    }

    #[test]
    fn the_layout_has_every_channel_on_its_controller() {
        let used: u32 = MODELS.iter().map(|m| m.pixels * 3).sum();
        assert_eq!(
            used, CHANNELS,
            "the models use {used} channels, the controller has {CHANNELS}"
        );
        assert_eq!(MODELS.iter().map(|m| m.pixels).sum::<u32>(), 1_091);
    }
}
