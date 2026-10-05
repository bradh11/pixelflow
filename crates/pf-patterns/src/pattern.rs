//! Pattern definitions and rendering.

use crate::{Rgbw, TargetRange};

/// A test pattern. Times are in seconds; speeds are in pixels per second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pattern {
    /// Every target pixel one color.
    Solid(Rgbw),
    /// Red, green, blue, then white, one second each.
    RgbwCycle,
    /// A block of `width` lit pixels moving along the target in wiring order.
    Chase { color: Rgbw, width: u32, speed: f32 },
    /// Brightness ramps from off to full over `period` seconds, then repeats.
    Ramp { color: Rgbw, period: f32 },
    /// Even and odd pixels swap between two colors every `period` seconds.
    Alternate { a: Rgbw, b: Rgbw, period: f32 },
    /// Blinks white twice per second, to find a prop or port.
    Identify,
    /// One lit pixel stepping along the target, to check pixel order.
    PixelWalk { color: Rgbw, speed: f32 },
}

/// Clears `frame` and paints `pattern` at time `t` onto `targets`.
///
/// Pattern positions count pixels across all target ranges in order, so a chase flows
/// from one prop into the next exactly as they are wired.
pub fn render(pattern: &Pattern, t: f32, targets: &[TargetRange], frame: &mut [u8]) {
    frame.fill(0);
    let total: u64 = targets.iter().map(|r| u64::from(r.pixels)).sum();
    if total == 0 {
        return;
    }
    let mut index: u64 = 0;
    for range in targets {
        let cpp = range.channels_per_pixel as usize;
        for k in 0..range.pixels {
            let node = if range.reverse { range.pixels - 1 - k } else { k };
            let start = range.frame_offset + node as usize * cpp;
            if let Some(pixel) = frame.get_mut(start..start + cpp) {
                color_at(pattern, t, index, total).write(pixel);
            }
            index += 1;
        }
    }
}

fn color_at(pattern: &Pattern, t: f32, index: u64, total: u64) -> Rgbw {
    let t = t.max(0.0);
    match *pattern {
        Pattern::Solid(color) => color,
        Pattern::RgbwCycle => [Rgbw::RED, Rgbw::GREEN, Rgbw::BLUE, Rgbw::WHITE][(t as u64 % 4) as usize],
        Pattern::Chase { color, width, speed } => {
            let head = (t * speed) as u64 % total;
            let behind = (head + total - index) % total;
            if behind < u64::from(width) {
                color
            } else {
                Rgbw::OFF
            }
        }
        Pattern::Ramp { color, period } => {
            let period = period.max(0.001);
            color.scaled((t % period) / period)
        }
        Pattern::Alternate { a, b, period } => {
            let phase = (t / period.max(0.001)) as u64 % 2;
            if index.is_multiple_of(2) == (phase == 0) {
                a
            } else {
                b
            }
        }
        Pattern::Identify => {
            if (t * 2.0).fract() < 0.5 {
                Rgbw::WHITE
            } else {
                Rgbw::OFF
            }
        }
        Pattern::PixelWalk { color, speed } => {
            if (t * speed) as u64 % total == index {
                color
            } else {
                Rgbw::OFF
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb_range(frame_offset: usize, pixels: u32, reverse: bool) -> TargetRange {
        TargetRange {
            frame_offset,
            pixels,
            channels_per_pixel: 3,
            reverse,
        }
    }

    /// Red-channel value of each RGB pixel in the frame.
    fn reds(frame: &[u8]) -> Vec<u8> {
        frame.chunks(3).map(|p| p[0]).collect()
    }

    #[test]
    fn solid_paints_only_targets_and_clears_the_rest() {
        let mut frame = vec![9u8; 12];
        render(
            &Pattern::Solid(Rgbw::RED),
            0.0,
            &[rgb_range(3, 2, false)],
            &mut frame,
        );
        assert_eq!(frame, vec![0, 0, 0, 255, 0, 0, 255, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn rgbw_cycle_steps_each_second_and_uses_white_channel() {
        let range = TargetRange {
            frame_offset: 0,
            pixels: 1,
            channels_per_pixel: 4,
            reverse: false,
        };
        let mut frame = vec![0u8; 4];
        let at = |t: f32, frame: &mut Vec<u8>| {
            render(&Pattern::RgbwCycle, t, &[range], frame);
            frame.clone()
        };
        assert_eq!(at(0.5, &mut frame), vec![255, 0, 0, 0]);
        assert_eq!(at(1.5, &mut frame), vec![0, 255, 0, 0]);
        assert_eq!(at(2.5, &mut frame), vec![0, 0, 255, 0]);
        assert_eq!(at(3.5, &mut frame), vec![255, 255, 255, 255]);
        assert_eq!(at(4.5, &mut frame), vec![255, 0, 0, 0]);
    }

    #[test]
    fn chase_flows_across_ranges_in_wiring_order_and_wraps() {
        let chase = Pattern::Chase {
            color: Rgbw::RED,
            width: 2,
            speed: 1.0,
        };
        // Two 3-pixel props; the second is wired in reverse.
        let targets = [rgb_range(0, 3, false), rgb_range(9, 3, true)];
        let mut frame = vec![0u8; 18];
        render(&chase, 3.0, &targets, &mut frame);
        // head = 3 → pattern indices 2 and 3 lit: prop A node 2, and prop B's last node.
        assert_eq!(reds(&frame), vec![0, 0, 255, 0, 0, 255]);
        render(&chase, 6.0, &targets, &mut frame);
        // head wraps to 0 → indices 0 and 5 lit (5 is prop B's first node).
        assert_eq!(reds(&frame), vec![255, 0, 0, 255, 0, 0]);
    }

    #[test]
    fn ramp_alternate_identify_and_walk() {
        let targets = [rgb_range(0, 4, false)];
        let mut frame = vec![0u8; 12];

        render(
            &Pattern::Ramp {
                color: Rgbw::RED,
                period: 2.0,
            },
            1.0,
            &targets,
            &mut frame,
        );
        assert_eq!(reds(&frame), vec![128; 4]);

        let alternate = Pattern::Alternate {
            a: Rgbw::RED,
            b: Rgbw::OFF,
            period: 1.0,
        };
        render(&alternate, 0.2, &targets, &mut frame);
        assert_eq!(reds(&frame), vec![255, 0, 255, 0]);
        render(&alternate, 1.2, &targets, &mut frame);
        assert_eq!(reds(&frame), vec![0, 255, 0, 255]);

        render(&Pattern::Identify, 0.1, &targets, &mut frame);
        assert_eq!(reds(&frame), vec![255; 4]);
        render(&Pattern::Identify, 0.3, &targets, &mut frame);
        assert_eq!(reds(&frame), vec![0; 4]);

        let walk = Pattern::PixelWalk {
            color: Rgbw::RED,
            speed: 2.0,
        };
        render(&walk, 1.0, &targets, &mut frame);
        assert_eq!(reds(&frame), vec![0, 0, 255, 0]);
    }

    #[test]
    fn empty_targets_leave_a_blank_frame() {
        let mut frame = vec![7u8; 6];
        render(&Pattern::Identify, 0.0, &[], &mut frame);
        assert_eq!(frame, vec![0; 6]);
    }
}
