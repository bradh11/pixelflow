//! Copies a controller's pixels out of the show frame into its channel buffer.

use crate::plan::ControllerPlan;

/// Fills `out` (the controller's channel buffer) from `frame`, applying reverse, color
/// order, brightness, and gamma. Channels not covered by a span (null pixels) are left as
/// they are, so callers keep them zeroed. Ranges outside `frame` or `out` are skipped.
pub fn render_controller(frame: &[u8], plan: &ControllerPlan, luts: &[[u8; 256]], out: &mut [u8]) {
    for span in &plan.spans {
        let cpp = span.channels_per_pixel as usize;
        let lut = &luts[span.lut];
        let pixels = span.pixels as usize;
        for k in 0..pixels {
            let src = span.frame_offset + k * cpp;
            let wire = if span.reverse { pixels - 1 - k } else { k };
            let dst = span.controller_channel + wire * cpp;
            let (Some(source), Some(target)) = (frame.get(src..src + cpp), out.get_mut(dst..dst + cpp))
            else {
                continue;
            };
            for (j, channel) in target.iter_mut().enumerate() {
                *channel = lut[source[span.order[j] as usize] as usize];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lut::build_lut;
    use crate::plan::{GatherSpan, Wire};
    use pf_model::ControllerId;

    fn plan(spans: Vec<GatherSpan>, channel_count: usize) -> ControllerPlan {
        ControllerPlan {
            id: ControllerId::new(),
            name: "C".into(),
            destination: Err("unused".into()),
            channel_count,
            spans,
            wire: Wire::Ddp { data_type: 0x0B },
        }
    }

    fn span(frame_offset: usize, controller_channel: usize, pixels: u32, reverse: bool) -> GatherSpan {
        GatherSpan {
            frame_offset,
            controller_channel,
            pixels,
            channels_per_pixel: 3,
            reverse,
            order: [1, 0, 2, 3],
            lut: 0,
        }
    }

    #[test]
    fn applies_color_order_reverse_and_leaves_null_pixels_zero() {
        // Frame: two RGB pixels (10,20,30) and (40,50,60).
        let frame = [10, 20, 30, 40, 50, 60];
        let luts = [build_lut(100, 1.0)];
        // One null pixel (3 channels) before a reversed GRB span.
        let plan = plan(vec![span(0, 3, 2, true)], 9);
        let mut out = vec![0u8; 9];
        render_controller(&frame, &plan, &luts, &mut out);
        assert_eq!(out, vec![0, 0, 0, 50, 40, 60, 20, 10, 30]);
    }

    #[test]
    fn applies_brightness_lut() {
        let frame = [255, 255, 255];
        let luts = [build_lut(50, 1.0)];
        let plan = plan(vec![span(0, 0, 1, false)], 3);
        let mut out = vec![0u8; 3];
        render_controller(&frame, &plan, &luts, &mut out);
        assert_eq!(out, vec![128, 128, 128]);
    }

    #[test]
    fn rgbw_pixels_carry_the_white_channel() {
        let frame = [1, 2, 3, 4];
        let luts = [build_lut(100, 1.0)];
        let mut s = span(0, 0, 1, false);
        s.channels_per_pixel = 4;
        let plan = plan(vec![s], 4);
        let mut out = vec![0u8; 4];
        render_controller(&frame, &plan, &luts, &mut out);
        assert_eq!(out, vec![2, 1, 3, 4]);
    }

    #[test]
    fn out_of_range_spans_are_skipped() {
        let luts = [build_lut(100, 1.0)];
        let plan = plan(vec![span(0, 0, 4, false)], 3);
        let mut out = vec![0u8; 3];
        render_controller(&[9; 3], &plan, &luts, &mut out);
        assert_eq!(out, vec![9, 9, 9]);
    }
}
