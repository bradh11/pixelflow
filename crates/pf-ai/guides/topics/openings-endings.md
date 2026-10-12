# Beginnings and endings

The first bars and the last chord: what the audience sees first and what they are left with.

## Before the first note

A sequence starts dark. Keep it dark until the music gives a reason, even if that is only half a second. Lights that are already on when the song starts make the opening look like a mistake.

## Openings

Match the way the song starts.

- **A quiet intro** (a piano, a voice, bells): one prop, or one small group, in one color, fading in over a bar or two (`fadeInMs`, or `on` with a low start level). Add a second prop every two or four bars. The tree's star, the roofline alone, a slow `twinkle`: something small enough that the audience leans in.
- **A cold open** (the band is in from the first beat): the whole house lands on beat one. A hit with `stage_cue`, then straight into the first section's look. No fade.
- **A count-in or pickup** (drum clicks, a spoken count, a riff before the downbeat): a prop a click, building to the downbeat, then the look. `place_effects` with a `spread` of `build` on the "Beats" track.
- **A long build from nothing**: props join one at a time from the edges in, or from the ground up, over the whole intro; brightness climbs with a `ramp` curve; the first full look arrives exactly where the song does.
- **A spoken or sound-effect intro**: let the matrix or the face carry it alone and keep the rest of the house dark until the music starts.

Whatever the opening, establish the palette in it. The first color the audience sees should be one the first section uses.

## Endings

Match the way the song ends.

- **A held last chord**: one steady look on the whole house, held for as long as the chord rings, then a slow fade to black (`fadeOutMs` of one to three seconds, or the `sustain` cue). Stillness after a busy song reads as an ending.
- **A hard stop** (the band hits the last note and is gone): a hit on the last note, then black, at once. No fade. A sparkle on the hit's tail is the only decoration it wants.
- **A fade-out** (the recording fades): thin the house out with the music. Take a prop group away every two or four bars, slow what is left, and let the last prop (the star, the tree) fade last.
- **A big finish** (final chorus into a ritard and a last hit): the full house through the last chorus, a `blackout` in the gap before the last note, the biggest hit of the song on it, and a hold that fades.
- **A quiet close after a loud song**: come all the way down to the opening's look. Ending where you began is satisfying.

After the last light goes out, leave it out. A sequence that pops back on for its last half second looks broken.

## The last chorus

It is the top of the show; plan it before you plan anything else. Take the chorus look and add the things you have been saving: white in the palette, sparkles, the matrix's best picture, a hit on every bar line of the last four bars. Then make sure nothing earlier in the song was as big.

## Check both ends

- Is anything lit in the silence before the music starts or after it ends? There shouldn't be.
- Does the first light arrive with a sound?
- Does the last light leave with the last sound, or fade with it?
- If the song loops in a show, does the dark at the end give the dark at the start room to breathe? A second of black at each end is plenty.
