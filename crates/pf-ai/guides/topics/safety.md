# Flash safety

Why fast flashing over a large area is dangerous, the limit to keep to, which effects and settings can break it, and what to use instead.

## The limit

Flashing lights can trigger seizures in people with photosensitive epilepsy, and a display on a street is seen by people who did not choose to watch it. Keep to this:

**Nothing flashes faster than about three times a second across a large part of the display.**

- "Flashes" means a big, sudden change in brightness and back: on and off, or white and dark, or two strongly different colors swapping. A smooth pulse is far safer than a hard one.
- "A large part" is roughly a quarter of the pixels or more: the whole house, a big matrix, a mega tree. A single arch or a star flickering is a small area.
- Saturated red flashing is the worst case. Be stricter with it.
- The risk is highest between about 10 and 25 flashes a second, which is exactly where a "fast strobe" sits.

This is a limit, not a style preference. A request for "crazy strobes on the whole house" gets a look that feels intense without breaking it, and a sentence saying why.

## What breaks it

- `strobe` and `shimmer` with a high rate on the whole-house group, a matrix, or a tree. Their guides say which values are how many flashes a second.
- `on` with its `shimmer` switched on: every other frame is the other color, about twenty changes a second. Over a large area that is squarely in the danger band, and it does not look as fast as it is.
- Any effect cut on a fast track and alternated with dark: an `on` and an `off` on every sixteenth note at a quick tempo. At 120 beats a minute, a beat is two flashes a second (safe), half beats are four (too fast for a large area), quarter beats are eight.
- A `square` curve on `brightness` with many cycles: count them. Cycles divided by the effect's seconds is flashes a second.
- `lightning` and big hits back to back. One is fine. Eight in two seconds is a strobe.
- Hard alternation between two full-house looks on every beat at a fast tempo.

## What to use instead

Intensity without flashing:

- Speed. A chase or a spiral at three times the speed feels far more urgent than a flicker.
- Sparkle. `twinkle`, or `sparkles` on a steady look: many small pixels changing softly, with the overall brightness steady. It reads as glitter, not flashing.
- Small areas. Let the star and the arches flash while the big props hold. The eye gets the excitement; the field of view doesn't flash.
- Soft edges. A `pulse` with a slow attack and release, or a `timingFade` curve with a long fade, breathes rather than blinks.
- Alternation in space rather than time: left half, then right half, so about half the display is lit throughout.
- Single hits. One flash on the drop, staged with `stage_cue`, then let it decay.

## The checks

- `review_draft` draws the whole sequence and counts whole-display flashes: more than three a second, kept up for two seconds, is marked unsafe, and the draft's score is capped until it is fixed. It also catches the flickers that are too fast to see frame by frame (an `on` with `shimmer`, a fast `shimmer`) from their settings.
- A safety finding comes first in the review's list. Fix it before anything else: slow the flashing, shorten it to a moment, or move it to a small prop.
- When you place anything that blinks, work out its rate first: effects a second on that layer, or cycles a second on the curve. If it is over three and the target is big, change it before you place it.

## Brightness is a kindness too

A full-white full-house flash at night is painful even once. For hits, a brightness of 70 to 80 percent on the big props reads as just as big, and a colored hit (the section's main color with white on the small props) is easier on the eyes than all white.
