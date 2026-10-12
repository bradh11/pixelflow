# Pulse

A whole prop swelling and sinking in brightness in time with a timing track's marks or with the music itself: the steady heartbeat under a section.

## What it draws

Every pixel in the same color at one brightness, and that brightness swings between `min` and `max`. There is no pattern and nothing moves: it is a level that keeps time.

- On a string, an arch, a matrix, or a tree: the whole prop glows and dims as one block.
- On the whole house (a group): every prop swells and sinks together.
- Following marks, it is brightest exactly on each mark and each mark takes the next palette color, so two colors take turns beat by beat on "Beats" and bar by bar on "Bars". The color goes by the mark's number in the track, not by where the effect starts, so Pulses on different rows agree. Following the music, it uses the first color only.
- It is see-through as it dims. Over another layer it swings between the look below and its own color; on an empty row it dims toward black.
- On the "Beats" track of a song at 120 beats a minute it swells twice a second, which drives. On "Bars" the same shape is stretched over two seconds: one slow breath a bar. With no timing track at all it pulses twice a second whatever the song is doing, so always give it one.

## Settings

- `source`: what it keeps time with.
  - `marks` (the default): one pulse for each mark of `timingTrack`. Exact, and the only source that uses `shape`. Reach for it whenever the sequence has the track.
  - `bass`: follows the low end, so it jumps on each kick drum and sinks between. On a song with a thump on every beat it was dark just before each beat and near full just after. Use it to pump with a kick pattern that isn't simply every beat, or before the song has a beat track.
  - `level`: follows how loud the music is. It rides a song's dynamics, low in a quiet verse and high in a loud chorus, but through an evenly loud passage it hardly moves: it sat at full all through a loud chorus. Use it for swells, intros, and music without a steady beat.
  - `onsets`: jumps for the instant a new sound starts, then falls. Each jump is one frame long, so it never made a clear pulse in the frames: a dim, even glow at the default `attack` and `release`, a brighter one that sags slowly with an `attack` near 0 and a `release` of 500 to 1000. Treat it as a level that shows how busy the music is. For a pulse on particular notes use a timing track of them.
- `timingTrack`: the track whose marks it pulses on; the music sources ignore it. "Beats" for a pulse a beat, "Bars" for slow breathing, a track of sung words for a pulse on each word. Each pulse lasts until the next mark starts, however far off: it follows a changing tempo, and on a track of sung lines one pulse fills the whole wait for the next line. Before the first mark, and with a track the sequence doesn't have, it stays at `min`.
- `shape`: the pulse between one mark and the next, for `marks` only (the times here are for 120 beats a minute, four to the bar). `sine` is smooth: full on the mark, lowest halfway, back up for the next. `saw` is full on the mark and fades evenly until the next: a hit and fade on "Beats", a two-second fall on "Bars". `square` is full for the first half and lowest for the second: a hard blink on "Beats", a second on and a second off on "Bars". `heartbeat` is two quick thumps, a sharp one on the mark and a softer one just after a quarter of the way, then rest: on "Bars" the second lands about half a second after the first.
- `min` and `max`: the lowest and highest brightness. A `min` of 0.1 to 0.2 keeps the prop from going black; 0.4 and up turns the pulse into a gentle swell on a prop that stays lit. Lower `max` to hold a pulsing prop back behind the others. A `ramp` curve on `max` makes the pulse grow through a build.
- `attack` and `release`: for the music sources only. `attack` is how fast it rises when the music jumps: 0 to 20 ms is instant; at 200 a thump is over before the rise is done, so the peak is lower and softer; at 1000 it no longer pulses and sits at a middling level. `release` is how slowly it falls back: at 60 ms `bass` snaps dark after each thump, a hard flash; at 250 it falls most of the way between beats; at 500 it sags to a little over half, a throb that never rests; 1000 and up is nearly steady.

## When to use it

- The base of a rhythmic section: Pulse on a group's row, a moving effect (`bars`, a `chase`) on the layer above.
- Over a steady `on` in another color it tints the prop back and forth instead of dimming it.
- `stage_cue` with `breathe` lays this effect for you: with the bass, or on a `track`, from a low of 0.15 up to a high set by `intensity`, in warm colors. Use it for a quiet stretch. Place Pulse yourself for a `shape` other than smooth, colors taking turns, your own `min` and `max`, the other sources, or a pulse on some props only.
- Flash safety: on "Beats" it flashes once a beat, which passes three a second above 180 beats a minute; use "Bars" there, or raise `min` so it swells rather than flashes. `square` is the harshest shape and `sine` with a raised `min` the gentlest. `heartbeat` is two flashes a mark: put it on "Bars", never on "Beats" across a whole house. With `bass` or `onsets`, a `release` under 100 makes every thump a hard flash, and busy drums can pass three a second: keep it at 250 or more on large props.
- The music sources need the sequence to have its song; without it Pulse stays at `min`.
- For one hit that fades use `impact`. For a pulse on any other effect, put a curve on its `brightness` (topic `curves`).

## Looks

```look
{"name": "Slow breath on the bars", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "Warm white swelling on each bar line and sinking to a low glow between: a calm base for a verse.",
 "effect": {"kind": "pulse", "settings": {"timingTrack": "Bars", "shape": "sine", "min": 0.15, "max": 0.8}, "colors": ["#ffd9a0"]}}
```

```look
{"name": "Beat pulse, red and green taking turns", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "Full on every beat and fading until the next, red on one beat and green on the following one.",
 "effect": {"kind": "pulse", "settings": {"timingTrack": "Beats", "shape": "saw", "min": 0.1}, "colors": ["#ff2020", "#00c040"]}}
```

```look
{"name": "Pumping with the bass", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "Blue that jumps with each kick and settles back between: needs the song's audio.",
 "effect": {"kind": "pulse", "settings": {"source": "bass", "attack": 30, "release": 300, "min": 0.1}, "colors": ["#2a6bff"]}}
```

```look
{"name": "Heartbeat on the bars", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "Two red thumps at the start of each bar, then rest.",
 "effect": {"kind": "pulse", "settings": {"timingTrack": "Bars", "shape": "heartbeat", "min": 0.1}, "colors": ["#ff1010"]}}
```

```look
{"name": "Riding the song's loudness", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "Gold that rises slowly as the music gets louder and sinks as it quiets: lay it across a whole song or a long swell.",
 "effect": {"kind": "pulse", "settings": {"source": "level", "attack": 150, "release": 1200, "min": 0.15}, "colors": ["#ffc040"]}}
```
