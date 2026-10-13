# Roles for props

What each kind of prop is for in a show, which effects suit it, and how to give every prop a part without giving every prop the same one.

## The cast

`list_props` gives each prop a role. Treat it as casting.

- **Outline** (rooflines, eaves, fence lines, path edges; a single run of pixels): the structure. It draws the shape of the house, so it should be the steadiest thing in the show. Slow color, gentle bars or chases along it, twinkle. When the outline flashes, the house flashes: save that for hits.
- **Window** (frames): structure too, but in pieces. Good for call and response (left windows, right windows), for holding the section's second color, and for blinking on the beat while the outline holds steady.
- **Matrix**: the picture. Words, pictures, dancers, fire, snow, fireworks, clouds of `plasma`, meters: anything that needs columns and rows. Give it at least one showpiece in every song, not only words and a face (topic `showpieces`). One matrix doing something worth reading beats four props doing the same stripes. Keep its brightness down beside thin props (`brightness` 0.5 to 0.7): it has more pixels than the rest of the house put together.
- **Tree** (mega trees, spiral trees): the focal point. Spirals, rising bars, garlands, fire, falling snow and meteors, sweeps of color. It usually carries the song's main movement and gets the first and biggest share of a hit.
- **Arch** and canes: the rhythm section. Chases that travel from arch to arch, bars along them, wipes and fills that run across the row on a drum fill, comets, garlands filling end to end, bands running out from the middle. They are the props that most obviously move in time.
- **Round** (stars, wreaths, snowflakes, spinners): accents and punctuation. A star on a tree is the top of every upward sweep. Pinwheels and fans on spinners.
- A prop with a face is a singer first: whatever else it does, it sings when there are words (topic `vocals`).
- Anything whose role is "other": look at its shape and its name, and cast it as the nearest of these.

## What suits what

A single run of pixels is one row tall. Effects that draw pictures on a grid show only a one-pixel slice of them there. Some cannot work on an outline or an arch at all: words, pictures, dancers, a singing face, Life, and a fan (its blades show as dashes). Others (fire, snow, plasma, rings) work only in the one-row form their guide gives. When in doubt, use the effects that run along a string.

Effects with a ready-made look for each kind of target, from their guides:

{{looks-by-target}}

Read an effect's own guide before using it: each says what it really draws on a string, an arch, a matrix, a tree, and the whole house.

## Groups and single props

- A group's row draws one picture across all its members: a wipe across the whole house, bars rising from the arches to the roofline. Use it for the base and for anything that should travel across the yard.
- A prop's own row is for what that prop does differently. Put it below its group's row in the list and it covers the group's look only where it plays.
- The same effect on every arch, each in step, is a group with the `perModelDefault` render style, or `place_effects` on each arch's row (topic `layouts`).

## Sharing out a section

- In a quiet section, two or three roles play and the rest are dark or dim. Outline and tree is a classic verse. Matrix alone is a classic bridge.
- In a loud section, every role plays, but not the same thing: outline steady, arches chasing, tree spiralling, matrix showing the picture, stars twinkling.
- Rotate who leads. If the tree led verse one, let the arches lead verse two.
- Left and right matter. A house reads as two halves and a middle; mirror what the halves do, and let things travel from one side to the other on purpose (topic `motion`).

## Every prop a part, every prop a rest

Before proposing, run down the list of props: each should play somewhere and be dark somewhere. A prop that never lights looks broken. A prop that never rests is wallpaper. `review_draft` names both.
