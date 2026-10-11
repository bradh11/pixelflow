# Distribution checklist

What giving someone a PixelFlow binary (free or paid) involves, as far as the licences of what
is inside it are concerned. These are facts about the code and about what the licence texts say.
It is **not legal advice**, it makes no decision, and its author is not a lawyer. Where a
question needs a lawyer, it says so.

Background: [provenance.md](provenance.md) (what follows other projects' source) and
[dependencies.md](dependencies.md) (what the dependencies' licences are).

## Where things stand

- The repository is public on GitHub and has **no `LICENSE` file**. No Cargo manifest has a
  `license` field, and `app/package.json` has none. With no licence, nobody outside has been
  given permission to copy, change, or redistribute PixelFlow's own code.
- **xLights is licensed under the GNU GPL, version 3** (`License.txt` in its repository, which
  each of its source files points to). About 31 PixelFlow files are ports of xLights source
  files, 10 more contain a ported function group, and about 27 take constants or tables from it.
  The list, with line counts, is in [provenance.md](provenance.md).
- One table (the mouth-shape mapping) is identical to an xLights file whose own header puts it
  under the GNU GPL, version 2 or later (it came to xLights from Papagayo).
- That code is already published, in source form, in this repository and its history.
- No installer has been published.

## 1. The GPL question

This is the one item that decides what licence PixelFlow itself can have, so it comes first. The
owner has to choose; the realistic options are below, without a recommendation.

Two things are different matters and should not be mixed up:

- **Reading and writing xLights' files, and talking to controllers.** The importer needs
  xLights' attribute names, setting keys, and file layouts; the device code needs FPP's and
  WLED's endpoints and packet formats. Those are the "format reading" rows in the provenance
  inventory. Code written independently, and code that only interoperates, is a different
  question from code translated from the source.
- **Ported algorithms.** The "direct port" rows follow xLights' C++ step for step, in another
  language. Whether a translation of GPL source is a work "based on" it, and how much of what
  the "behaviour-matching" rows took (constants, tables, defaults) is protected at all, are
  legal questions. This document does not answer them.

### Option A: release PixelFlow under the GPL

License PixelFlow's own code under GPL-3.0-or-later and distribute the app under GPL version 3.

What the GPL's text asks for, in summary:

- Everyone who receives a binary must be able to get the complete corresponding source, under
  the GPL (section 6): shipped with it, or by a written offer, or from a network location stated
  alongside the download.
- The whole program is conveyed under the GPL (section 5). Recipients may change it and pass it
  on, for free or for money, and may not be stopped from doing so.
- Keep the copyright and licence notices; state changes; include the licence text.

Facts that bear on it:

- **Selling is allowed.** The GPL does not restrict the price of a copy. What it rules out is
  stopping a buyer from sharing their copy or its source. A paid GPL product relies on
  convenience, support, signed installers, or services, not on exclusivity.
- **The dependencies appear compatible.** The shipped dependencies are MIT, BSD, ISC, Zlib,
  Apache-2.0, Unicode, and MPL-2.0 licensed. The FSF lists MIT (Expat), the modified BSD
  licences, ISC, zlib, Apache-2.0, and MPL-2.0 as compatible with GPL version 3.
  `webpki-roots` (CDLA-Permissive-2.0, a data licence) is not on that list; ask.
- xLights' files state version 3 through `License.txt` and do not say "or any later version".
  The Papagayo-derived mapping says version 2 "or (at your option) any later version".
- **Patent-licensed codecs interact with GPL version 3.** Sections 11 and 12 have terms about
  conveying a program while relying on a patent licence that does not reach recipients. With
  H.264 and AAC encoders compiled in (item 5), this is a question for a lawyer.
- **App stores.** GPL software has been pulled from Apple's App Store in the past over a
  conflict between the store's terms and the GPL. Direct downloads are not affected.
- It would need: a `LICENSE` file, `license` fields in the manifests, a notice in each file or
  one covering them, credit to xLights in the ported files, and a source link in the app.

### Option B: clean-room reimplementation

Rewrite the xLights-derived modules without reference to xLights source, so that PixelFlow can
be offered under some other licence.

- The list of what would be rewritten, with effort per module, is the last table in
  [provenance.md](provenance.md). In round numbers: the model geometry (large), VU Meter, Shape,
  and Tendril (large each), about seven more effects (days each), value curves and the effect
  settings tables (days each), and a set of small pieces.
- A clean room, as the term is normally used, separates two roles: one person describes the
  behaviour from the outside (running xLights, its manual, files it saves), and a different
  person, who has not read the xLights source, writes code from that description alone. A
  rewrite by someone who has read the source is not that.
- The mouth-shape table would be replaced with one derived independently (the Preston Blair
  mouth set is described in his book; the table has 39 entries).
- No test compares PixelFlow's output with xLights' own. Tests do assert particular numbers
  worked out from xLights' formulas, most strictly for the model geometry, where exact positions
  are the point (see provenance.md, "Tests"). A rewrite would re-derive those from behaviour.
- Until it is done, and afterwards in the history, the repository still contains the ported
  code. What that means for the published history is a question for a lawyer.
- The file-format and protocol code stays as it is.

### Option C: ask a lawyer which modules are actually derivative

Take the provenance inventory to counsel who knows software copyright and open-source licences,
before choosing A or B, or to narrow B. Questions to bring:

1. Which of the "direct port" files are derivative works of xLights, given they are
   translations into another language with their structure kept?
2. For "behaviour-matching" files: are default values, ranges, lookup tables, and short
   formulae taken from the source protected, or are they facts and functional requirements?
3. Does it matter that the model geometry must reproduce xLights' output exactly for imported
   shows to work (interoperability)?
4. The 39-entry mouth-shape table: is it protectable, and under which terms does it arrive?
5. The code is already public without a licence. Does that change anything, and should the
   history be dealt with?
6. If Option A: how do the H.264 and AAC patent situations sit with GPL version 3, sections 11
   and 12?
7. If Option B: what process and record-keeping would make the rewrite defensible?
8. The names "xLights", "FPP", "Falcon", and "WLED" appear in the UI and the README ("a fresh
   take on xLights"). Is that use fine for a paid product?

## 2. PixelFlow's own licence

- [ ] Decide the licence (after item 1). Add `LICENSE` at the repo root, and `license` to the
      workspace manifest, `app/src-tauri/Cargo.toml`, and `app/package.json`.
- [ ] If the app is sold under terms other than an open-source licence, it needs those terms
      (an end-user licence), and they must not contradict the third-party licences below.

## 3. Third-party notices in the app

Nearly every dependency (MIT, BSD, ISC, Apache-2.0, Zlib, Unicode, MPL-2.0) asks that its
copyright notice and licence text accompany a binary distribution. Apache-2.0 also asks that the
contents of a package's `NOTICE` file be carried along.

What is in place:

- `scripts/licenses.mjs` writes `app/src-tauri/resources/THIRD-PARTY-NOTICES.txt`: every
  package linked into the app or bundled into its JavaScript (485 at the time of writing), each
  with its licence, source URL, and the licence and notice files from the package itself. Where
  a package ships no licence file, the standard text of the licence it names is printed.
- `app/src-tauri/tauri.conf.json` lists that file under `bundle.resources`, so Tauri copies it
  into the app. Checked by building a macOS app bundle (`pnpm tauri build --debug --bundles
  app`): the file is at `PixelFlow.app/Contents/Resources/THIRD-PARTY-NOTICES.txt`, identical to
  the source. On Windows, Tauri puts resources beside the executable, and on Linux in the app's
  folder under `/usr/lib/`; those were not built here.

Still to do:

- [ ] Regenerate before every release (`make licenses`), and make
      `node scripts/licenses.mjs --check` part of the release steps. The file is committed, so a
      lockfile change without a regenerate leaves it stale. Nothing in CI checks this yet.
- [ ] Let people find it. Proposed: an "About PixelFlow" item with a "Third-party notices"
      button that opens the bundled file (the shell resolves it with Tauri's `resource_dir`).
      That is a UI change and is not built.
- [ ] If `pf-cli` is ever distributed, it needs notices of its own (it links `clap` and other
      packages the app does not).
- [ ] Check the built Windows and Linux packages contain the file.

## 4. Data and models

- [x] **CMU Pronouncing Dictionary.** Compiled into the app. Its licence asks that binary
      redistributions reproduce its copyright notice, conditions, and disclaimer "in the
      documentation and/or other materials provided with the distribution". The notices file now
      carries it; before, the built app had the data without the notice.
- [ ] **wav2vec2 speech model.** Apache-2.0. PixelFlow does not ship it: the user's copy of the
      app downloads it from Hugging Face (a pinned revision, checked against a SHA-256) when the
      user asks. The notices file names it and its source. If it is ever put in the installer,
      Apache-2.0's redistribution terms apply to PixelFlow directly. Hugging Face's own terms of
      service cover the download.
- [ ] **Mouth-shape mapping.** See item 1.
- [ ] **The pixel font** in `pf-render/src/text.rs` ("the classic 5 x 7 one"): its origin is not
      recorded. Find the source and its terms, or redraw it.
- [ ] **Lyrics.** "Find lyrics" fetches lyric text from LRCLIB, a community database, at the
      user's request, and stores it in the user's own sequence. Song lyrics are generally
      copyrighted by their publishers. Whether a paid product may offer this is a question for a
      lawyer.

## 5. Patent-licensed codecs

A software licence (BSD, MIT, MPL) covers copyright. It does not grant rights under patents that
cover a format. PixelFlow contains:

| Codec | Where | Used for |
|---|---|---|
| H.264 (AVC) encoder | OpenH264, compiled from source by `openh264-sys2`, used by `pf-video` | Video export |
| AAC-LC encoder | `oxideav-aac`, used by `pf-video` | The sound in exported videos |
| AAC-LC decoder | `symphonia-codec-aac` | Playing and analysing `.m4a` music |
| MP3 decoder | `symphonia-bundle-mp3` | Playing and analysing `.mp3` music |

Facts:

- **OpenH264.** Cisco pays the AVC patent pool's royalties only for the binary Cisco itself
  distributes, and only under conditions. Its FAQ says: "a team can choose to use the source
  code, in which case the team is responsible for paying all applicable license fees, or the
  team can use the binary module distributed by Cisco, in which case Cisco will cover the MPEG
  LA licensing fees", and, asked whether Cisco covers a product built from the source, "No."
  PixelFlow builds OpenH264 from source, so Cisco's arrangement does not cover it.
- Cisco's conditions for its binary (from its `BINARY_LICENSE.txt`): the binary is downloaded
  separately to the user's device, not bundled into the product beforehand; the user can turn it
  off; the product shows "OpenH264 Video Codec provided by Cisco Systems, Inc." where that
  control is; and the licence text is reproduced with the product's licensing information. The
  `openh264` crate has a `libloading` feature for using that binary in place of the compiled-in
  source.
- **AVC and AAC** patents are licensed through pools run by Via Licensing Alliance (which now
  includes MPEG LA). Whether a given distributor needs a licence, and what it costs at a given
  volume, are for that organisation and a lawyer to say.
- **MP3.** Fraunhofer and Technicolor ended their MP3 licensing programme on 23 April 2017; the
  patents it covered are widely reported to have expired.
- If the user picks ffmpeg for a video export, PixelFlow runs the user's own ffmpeg, which is
  not distributed with PixelFlow.

Options, without a recommendation:

- [ ] Take the patent licences that apply.
- [ ] Use Cisco's OpenH264 binary under its conditions (covers H.264 only, not AAC).
- [ ] Encode with the operating system's own encoders (macOS VideoToolbox, Windows Media
      Foundation), which the OS vendor has licensed. Not available the same way on Linux.
- [ ] Export a format without these pools (for example VP9 or AV1 video with Opus sound).
- [ ] Leave video export out of distributed builds.
- [ ] Get advice, particularly on AAC decoding, which has no equivalent of Cisco's arrangement.

## 6. MPL-2.0 packages

`symphonia` (12 crates), `triple_buffer`, and four packages Tauri uses are under the Mozilla
Public License 2.0. For an executable, section 3.2 asks that recipients be told how to get the
source of those files and that their rights in them are not restricted.

- [x] The notices file lists each with its source URL and the MPL text. PixelFlow uses them
      unmodified.
- [ ] If any of them is ever patched for PixelFlow, the patched files must be published.

## 7. Packaging, per platform

- [ ] **Linux.** A `.deb` or `.rpm` names GTK 3, WebKitGTK, libsoup, ALSA, and D-Bus as
      dependencies and contains none of them. An **AppImage contains copies of them**. Most are
      LGPL, which then applies to PixelFlow as their distributor: carry their licence texts and
      make their source available. The notices file does not cover them, because they are not
      Cargo packages. `tauri.conf.json` has `"targets": "all"`, which builds an AppImage.
- [ ] **Windows.** The app needs Microsoft's WebView2 runtime. By default Tauri's installer
      downloads Microsoft's bootstrapper at install time; embedding the runtime in the installer
      is governed by Microsoft's redistribution terms.
- [ ] **macOS.** Uses the system's WebKit; nothing extra is redistributed.
- [ ] **zstd.** Compiled in. It is offered under BSD or GPL-2.0 at the recipient's choice; both
      texts are in the notices file with a note saying so.

## 8. Services the app talks to

Not redistribution, but part of offering the product. Details are in
[../privacy-and-data-flow.md](../privacy-and-data-flow.md).

- [ ] **OpenAI and Anthropic.** Users bring their own API keys, and their use is under their own
      agreement with the provider.
- [ ] **LRCLIB, Hugging Face, GitHub's API.** Each has terms of use for automated clients.
      PixelFlow identifies itself to LRCLIB and GitHub with a `User-Agent`.

## Summary of open decisions

1. Option A, B, or C for the xLights-derived code. Everything else waits on it.
2. PixelFlow's own licence.
3. What to do about H.264 and AAC.
4. Whether to ship an AppImage.
5. Where the notices appear in the app.
