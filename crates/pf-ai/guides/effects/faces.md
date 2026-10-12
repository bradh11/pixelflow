# Faces

Makes a prop that has a face sing: its mouth takes the shape of each sound on a lyrics track and its eyes blink; on a prop with no face it draws nothing.

## What it draws

A face is something the prop owns, not something this effect makes: some of the prop's pixels marked as an outline, eyes open, eyes closed, and a mouth for each of ten mouth shapes. `list_props` says which props have one. The effect lights the mouth for the sound being sung at that moment, the eyes, and the outline if asked. Every other pixel of the prop is left clear, so a layer below shows round the features.

- On a matrix (or a custom singing prop) that has a face: two eyes and a mouth. On the faces tried, a wide vowel was a tall, wide block of mouth; between words it dropped to a short thin line, the resting mouth; other sounds gave shapes in between. How good it looks is down to how the face was drawn on the prop.
- On a string, an arch, a tree, or a matrix with no face: nothing at all. No error, just dark. For those use `sing`.
- On the whole house (a group): every member that has a face sings, each on its own pixels and in step, and every other member stays dark. One effect on a group of singing faces drives the whole choir.
- It has no speed of its own. Fast lyrics give a fast mouth; a held note holds one shape.

## Settings

- `timingTrack`: what the mouth follows. The kind of track decides how well.
  - A phonemes track ("Lyrics (phonemes)"): each mark is one mouth shape, shown for as long as it is sung, so held vowels stay wide open. This is the one to use.
  - A words track ("Lyrics (words)"): the shapes are worked out from how each word is said and given equal time. The mouth still rests between words, but late in a held word it had already closed to a thin shape while the phonemes track still showed the wide vowel. Use it when there is no phonemes track.
  - A lines track ("Lyrics"): all the sounds of the line shared evenly over the whole line. The mouth keeps moving right through the gaps between words and never rests until the line ends, so it drifts off the singing. A last resort.
  - Anything else (syllables, beats, bars, sections, marks of your own): the mouth stays on its resting shape the whole time. So does a face with no track at all: eyes and a shut mouth, which is a face that is there but not singing.
- `face`: which of the prop's faces to use, by name. Leave it blank for the prop's first face, which is right for nearly every prop. A name the prop doesn't have draws nothing.
- `eyes`: `auto` keeps them open with a short blink every three to five seconds, which is what makes it look alive. `open` never blinks. `closed` shows the face's closed eyes (thin lines on the faces tried): for a soft line, or a face asleep before its entrance.
- `colors`: `face` uses the colors the face was made with, and the palette when it has none. `palette` always uses the effect's palette: the first color for the mouth, the second for the eyes, the third for the outline. A shorter palette repeats its last color (one color paints everything), and no palette is white.
- `outline`: also lights the face's outline. Off, only the mouth and eyes show: a face floating in the dark.

## When to use it

- Whenever there are lyrics and a prop with a face. One long effect over a whole sung section is enough: the mouth rests by itself between words and the eyes keep blinking, so the face never goes dead. Topic `timing` says where the lyrics tracks come from; `stage_cue` with the `sing` cue places faces for you.
- Faces or `sing`? Faces is for the prop that has a face, and shows real mouth shapes. `sing` is for everything else: it makes an arch, a tree, or the whole house pulse, widen, or fill along with the same words. Use both together: the face is the singer, and `sing` on the outline or arches is the house joining in.
- Keep the face's prop clear of busy effects while it sings: give its row the top layer, with at most a dim `on` under it as a backdrop.
- Before the first line and in instrumental breaks, leave it running, or use one with no track: a resting face reads better than a dark prop.
- Several faces: the lead on one and the backing lines on the others, or all on one group row for a choir.
- Don't give it the syllables track (the mouth won't move), and don't put it on a group hoping the arches will sing: they have no face.

## Looks

```look
{"name": "A face singing the lead", "suits": ["matrix", "house"],
 "note": "The mouth making the shape of every sound and resting between words, eyes blinking. Red mouth and white eyes unless the face has colors of its own.",
 "effect": {"kind": "faces", "settings": {"timingTrack": "Lyrics (phonemes)"}, "colors": ["#ff3030", "#ffffff"]}}
```

```look
{"name": "A face in show colors with its outline lit", "suits": ["matrix", "house"],
 "note": "Red mouth, white eyes, and a green outline round the face, whatever colors it was made with.",
 "effect": {"kind": "faces", "settings": {"timingTrack": "Lyrics (phonemes)", "colors": "palette", "outline": true}, "colors": ["#ff3030", "#ffffff", "#20c040"]}}
```

```look
{"name": "A face waiting to sing", "suits": ["matrix", "house"],
 "note": "Mouth shut and eyes blinking in a dim warm amber: for an intro or an instrumental break, so the face is not dark.",
 "effect": {"kind": "faces", "settings": {"eyes": "auto"}, "colors": ["#ffb060"], "brightness": 0.6}}
```

```look
{"name": "Singing from the words track", "suits": ["matrix", "house"],
 "note": "The same singing face when the sequence has words but no mouth-shape track: a little stiffer on held notes.",
 "effect": {"kind": "faces", "settings": {"timingTrack": "Lyrics (words)"}, "colors": ["#ffffff", "#60c0ff"]}}
```
