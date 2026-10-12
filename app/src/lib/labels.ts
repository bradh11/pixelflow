// Wording the app uses in more than one place, so a setting has one name wherever it appears.

import type { MusicLevelSource } from "../api/sequence";

/** The per-sequence setting that moves the lights against the music (Play screen, playlist drawer). */
export const LIGHT_TIMING = "Light timing";
export const LIGHT_TIMING_HINT = "Move the lights earlier or later against the music.";
/** What Light timing does and does not reach (the engine applies it to the preview and to live output alike). */
export const LIGHT_TIMING_SCOPE =
  "Applies to this sequence only, in the preview and on your controllers when it plays here. It isn't saved into exported files or Send to FPP, and it's separate from Preview sync in Settings.";

/** The per-sequence setting for which channel of the music is how loud it is (Sequence settings). */
export const MUSIC_LEVEL = "Loudness from";
export const MUSIC_LEVEL_SOURCES: { value: MusicLevelSource; label: string }[] = [
  { value: "louderChannel", label: "The louder channel" },
  { value: "leftChannel", label: "The left channel" },
];
export const MUSIC_LEVEL_HINT =
  "How loud the music is to the effects, sparkles and curves that follow it. Sequences imported from xLights use the left channel, so they play as they did there.";

/** The Settings section that shifts only the on-screen preview. */
export const PREVIEW_SYNC = "Preview sync";
export const PREVIEW_SYNC_SCOPE =
  "Moves only the on-screen preview and the timeline's playhead, on this computer. It doesn't change what's sent to your lights, Light timing, or exported files.";

/** The toggle that sends playback to the real controllers; one name on Play and on Sequence. */
export const SHOW_ON_LIGHTS = "Show on my lights";
export const SHOW_ON_LIGHTS_HINT =
  "When on, playing here also sends each frame to your controllers live, so your real lights show the sequence. When off, it plays only in the preview.";

/** Why Send setup is off for a Falcon: PixelFlow reads its setup but can't write it. */
export const FALCON_READ_ONLY = "PixelFlow can read a Falcon's setup (use Compare) but can't send one yet. Set it on the Falcon's own page.";

/** The controller reachability check only asks whether its web page replies. */
export const WEB_PAGE_ANSWERING = "Web page answering";
export const WEB_PAGE_NOT_ANSWERING = "Web page not answering";
export const WEB_PAGE_HINT = "This checks the controller's web page, not that light data is arriving.";
