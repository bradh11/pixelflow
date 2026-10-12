# Motion

Making things move on purpose: direction that agrees across props, symmetry, call and response, chases across the house, and wipes.

## Direction is a decision

Every moving effect has a direction. Decide it for the section, not effect by effect.

- **Agree.** If the roofline's bars slide right, the arches chase right and the tree's spiral turns the same way round. A house where everything travels one way looks like one thing moving.
- **Or mirror.** Left half moving right, right half moving left, meeting in the middle (or the reverse, opening out). Symmetry suits a house, which is usually close to symmetrical itself. Use it for arrivals: closing in on the door into a chorus, opening out on the hit.
- **Never neither.** Props moving in unrelated directions at unrelated speeds is the look of a sequence nobody planned.
- Change direction when the music changes: reverse for the second half of a chorus, or on the answer to a call.

How to steer: most moving effects have their own `direction` setting (and `chase` has `order`); use those. `bufferTransform` flips an effect drawn on a grid. For a group, the effect travels across the whole layout as one picture unless its `renderStyle` says otherwise (topic `layouts`).

## Speed follows the music

- Tie speed to the tempo. A chase that crosses its prop once a bar, or bars that advance one stripe a beat, look locked to the song even with nothing else in time.
- Slow in quiet sections, faster in loud ones, and a `ramp` curve on speed through a build (topic `curves`).
- Two speeds at most across the house at one time: a slow one for the structure, a quicker one for the rhythm props.

## Moves that read from the street

- **A chase across the house.** Props light one after another, left to right. `place_effects` with a `spread` of `sweep` on "Beats", or the `chase` cue of `stage_cue`. For drum fills and the bar before a chorus.
- **A wipe.** Color sweeping across the whole layout: a `wipe` effect on the whole-house group, or the `sweep` cue. For section changes; it turns the page.
- **Call and response.** Two groups take turns: left then right, tree then arches, high props then low. `place_effects` with a `spread` of `alternate`, or the `call_response` cue. For melodies that answer each other and for verses with a back-and-forth.
- **A build.** Props join one at a time until all are in: a `spread` of `build`, or the `ramp` cue. For the bars before a drop.
- **Rise and fall.** Bars climbing a tree and the house on a rising phrase, falling on the answer. On a group's row it travels from the arches up to the roofline.
- **Out from the center.** Rings, ripples, and bars with `motion` set to `split`, for a hit: the hit lands in the middle and the house answers outward.
- **Stillness.** Holding everything steady for two bars after a busy passage is a move too, and often the strongest one available.

## Where movement lives

- On the rhythm props first: arches, canes, stakes. That is what they are for.
- On the tree for the main, continuous movement of a section.
- On the outline last and least. A roofline that never stops chasing is tiring; let it hold or drift, and move only for the big sections.
- On a matrix, movement competes with the picture. If it is showing words, nothing else on it should move.

## Too much

- If everything moves all the time, nothing does. Give each section one prop group that leads the motion and let the rest support.
- Fast movement on a big bright prop is close to flashing: topic `safety`.
