# Distribution checklist

What giving someone PixelFlow involves, as far as its own licence and the licences of what is
inside it are concerned. These are facts about the code and about what the licence texts say. It
is **not legal advice**, and its author is not a lawyer. Where a question needs a lawyer, it says
so.

Background: [provenance.md](provenance.md) (what is derived from other projects' source) and
[dependencies.md](dependencies.md) (what the dependencies' licences are).

## Where things stand

- **PixelFlow is free, open-source software under the GNU General Public License, version 3**
  (`GPL-3.0-only`). The licence text is [`LICENSE`](../../LICENSE) at the repo root. Every Cargo
  manifest and `app/package.json` state it, and so does the app's bundle metadata.
- **It is free of charge.** People who want to support it can donate
  ([README](../../README.md#support-the-project)). Donations are gifts to the project; nobody
  pays for a copy or for a licence.
- **Credit to xLights** is in [`NOTICE`](../../NOTICE), which the README links to, the app's
  About shows in short, and the notices bundled with the app carry word for word.
- **xLights is licensed under the GNU GPL, version 3** (`License.txt` in its repository, which
  each of its source files points to). About 33 PixelFlow files are ports of xLights source
  files, 12 more contain a ported function group, and about 30 take constants or tables from it.
  The list, with line counts, is in [provenance.md](provenance.md).
- One table (the mouth-shape mapping) is identical to an xLights file whose own header puts it
  under the GNU GPL, version 2 or later (it came to xLights from Papagayo).
- The repository is public on GitHub. Until the commit that added `LICENSE`, it carried the
  xLights-derived code with no licence of its own.
- No installer has been published.

## 1. The licence

### Why the GPL, and why version 3 only

PixelFlow contains code derived from xLights, and xLights is under the GPL. The GPL lets anyone
build on a program it covers, on the condition that the result is offered under the same licence
(section 5). Offering PixelFlow under the GPL meets that condition for every file, however each
one came about, so no file has to be argued over.

Which version was settled from what xLights publishes:

- xLights' `License.txt` is the text of the GNU General Public License, version 3. No statement
  beside it adds "or any later version".
- Its `README.txt` says nothing about the licence. GitHub reports the repository as `GPL-3.0`.
- The earlier reading of xLights' source for [provenance.md](provenance.md) recorded that each
  source file's header points to `License.txt`, and that none says "or any later version".

So nothing public offers xLights under a later version, and the files derived from it can be
relied on under version 3 only. PixelFlow uses `GPL-3.0-only` for the whole project, so one
statement is true of every file. The Papagayo-derived mapping says version 2 "or (at your option)
any later version", which includes version 3.

The owner could later offer the files that are wholly PixelFlow's own under "version 3 or later"
as well; only a copyright holder can widen a licence. That cannot be done for the derived files
unless xLights' authors do it for xLights.

### What the GPL's text asks for

In summary, for someone who gives PixelFlow to others:

- **Source.** Everyone who receives a binary must be able to get the complete corresponding
  source, under the GPL (section 6): shipped with it, or by a written offer, or from a network
  location stated alongside the download.
- **The same licence for the whole.** The whole program is conveyed under the GPL (section 5).
  Recipients may change it and pass it on, for free or for money, and may not be stopped from
  doing so.
- **Notices.** Keep the copyright and licence notices; say that the work was changed and when;
  include the licence text (sections 4 and 5).
- **In the app.** A program with an interactive interface shows a copyright notice, that there
  is no warranty, that it may be passed on under the GPL, and how to see the licence (section 5,
  "Appropriate Legal Notices").

Facts that bear on it:

- **Charging is allowed, and so are donations.** The GPL does not restrict the price of a copy.
  What it rules out is stopping a recipient from sharing their copy or its source. PixelFlow
  does not charge.
- **The dependencies are compatible**, with two kinds reported apart because the Free Software
  Foundation's licence list does not name them: see item 4.
- **Patent-licensed codecs** are a separate matter from copyright: see item 6.
- **App stores.** GPL software has been pulled from Apple's App Store in the past over a
  conflict between the store's terms and the GPL. Direct downloads are not affected.

### What is in place

- [x] `LICENSE` at the repo root: the GPL, version 3, unmodified.
- [x] `license = "GPL-3.0-only"` in the workspace manifest (which every engine crate inherits),
      in `app/src-tauri/Cargo.toml`, and in `app/package.json`. `scripts/licenses.mjs --check`
      fails if one of them says anything else.
- [x] `NOTICE`: the copyright line, the licence statement, the warranty disclaimer, where the
      source is, and the credit to xLights, Papagayo, and wxWidgets.
- [x] The app bundles `LICENSE.txt`, `NOTICE.txt`, and `THIRD-PARTY-NOTICES.txt` (which opens
      with the notice), and its bundle metadata names the licence.
- [x] The app's About shows the copyright line, that there is no warranty, the licence and how
      to read it, where the source is, and the credit.
- [x] Each file derived from xLights names the xLights file it follows in its opening comment.
      When each was written and changed is in the repository's history; the xLights commit the
      inventory compared against is in [provenance.md](provenance.md).

### Still to do

- [ ] **Source for each release.** A GitHub release carries the tagged source beside the
      installers, which states where the source is. The corresponding source also includes the
      packages the app is built from, at the versions the lockfiles name. They are public on
      crates.io and npm; a distributor who wants not to depend on those staying available
      attaches an archive of them to the release (`cargo vendor`, and the pnpm store).
- [ ] **The xLights copyright line.** `NOTICE` credits "the xLights authors and contributors".
      xLights' own source headers were not read for this, so the exact wording of their
      copyright line is not reproduced. Someone who may read them should copy it in.
- [ ] If `pf-cli` is ever distributed, it needs the licence, the notice, and notices of its own
      (item 3).

## 2. Credit, the inventory, and rewrites

The owner's intent is that PixelFlow be an independent project inspired by xLights, not a copy
of it. Both halves are stated where people will see them:

- PixelFlow as a product, and most of its code, is its own: the engine, output, audio analysis,
  the assistant, camera mapping, video export, the desktop shell, nearly all of the UI, and
  about two thirds of the renderer.
- The files [provenance.md](provenance.md) lists as direct ports, and the function groups and
  tables it lists beside them, are derived from xLights' source. They are credited as derived,
  not as inspired.

What the inventory is for now:

- **Credit.** It is the record `NOTICE` points to: which files, following which xLights files,
  and how closely.
- **Anyone who wants to know.** Someone reusing a PixelFlow file elsewhere can see whether
  xLights' authors also hold copyright in it.
- **Choosing what to rewrite.** Its effort column says what replacing each piece would take.

Rewrites are done for quality, not to get out from under the licence. Where PixelFlow can do a
part better with a design of its own (faster, simpler, or closer to what xLights draws), the part
is rewritten from a written description of what it shows, and its row moves from "direct port" to
"behaviour-matching". Pinwheel and Shockwave have been through that; Plasma is next. The method
and what it showed are in [provenance.md](provenance.md), "Rebuilt from a description of its
behaviour". A part that works well as it is stays, credited.

Two things are different matters and should not be mixed up:

- **Reading and writing xLights' files, and talking to controllers.** The importer needs
  xLights' attribute names, setting keys, and file layouts; the device code needs FPP's and
  WLED's endpoints and packet formats. Those are the "format reading" rows in the provenance
  inventory. Code written independently, and code that only interoperates, is a different
  question from code translated from the source.
- **Ported algorithms.** The "direct port" rows follow xLights' C++ step for step, in another
  language. Whether a translation of GPL source is a work "based on" it, and how much of what
  the "behaviour-matching" rows took (constants, tables, defaults) is protected at all, are
  legal questions. Under the GPL PixelFlow does not need them answered: the whole is offered
  under the licence that covers the derived parts.

### If the question of another licence ever comes up

PixelFlow can be offered under a licence other than the GPL only if nothing in it is derived from
GPL code. That is not planned. For the record, what it would take:

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
- The repository's history still contains the ported code, whatever is rewritten later. What
  that means for the published history is a question for a lawyer.
- The file-format and protocol code stays as it is.

### Questions a lawyer could still answer

None of these blocks distributing PixelFlow under the GPL. They are here so they are not lost.

1. Which of the "direct port" files are derivative works of xLights, given they are
   translations into another language with their structure kept? (Under the GPL the answer
   changes nothing PixelFlow does; it matters for the credit's wording and for any later
   relicensing.)
2. For "behaviour-matching" files: are default values, ranges, lookup tables, and short
   formulae taken from the source protected, or are they facts and functional requirements?
3. Does it matter that the model geometry must reproduce xLights' output exactly for imported
   shows to work (interoperability)?
4. The 39-entry mouth-shape table: is it protectable, and under which terms does it arrive?
   (PixelFlow treats it as GPL, version 2 or later, used under version 3, and credits it.)
5. The code was public without a licence for a time. Does that leave anything to put right
   beyond adding the licence and the credit?
6. How do the H.264 and AAC patent situations sit with GPL version 3, sections 11 and 12, if a
   distributor ever takes a patent licence? (Item 6.)
7. If another licence were ever wanted: what process and record-keeping would make a rewrite
   defensible?
8. The names "xLights", "FPP", "Falcon", and "WLED" appear in the UI and the README. xLights'
   own README marks its name as a trademark. PixelFlow uses the names to say what it imports and
   talks to, says it is not affiliated, and uses nobody's logo. Is that use fine?

## 3. Third-party notices in the app

Nearly every dependency (MIT, BSD, ISC, Apache-2.0, Zlib, Unicode, MPL-2.0) asks that its
copyright notice and licence text accompany a binary distribution. Apache-2.0 also asks that the
contents of a package's `NOTICE` file be carried along.

What is in place:

- `scripts/licenses.mjs` writes `app/src-tauri/resources/THIRD-PARTY-NOTICES.txt`. It opens with
  PixelFlow's own [`NOTICE`](../../NOTICE), word for word, and then lists every package linked
  into the app or bundled into its JavaScript (485 at the time of writing), each with its
  licence, source URL, and the licence and notice files from the package itself. Where a package
  ships no licence file, the standard text of the licence it names is printed.
- `app/src-tauri/tauri.conf.json` lists that file, `LICENSE`, and `NOTICE` under
  `bundle.resources`, so Tauri copies them into the app (as `THIRD-PARTY-NOTICES.txt`,
  `LICENSE.txt`, and `NOTICE.txt`). The notices file was checked by building a macOS app bundle
  (`pnpm tauri build --debug --bundles app`): it is at
  `PixelFlow.app/Contents/Resources/THIRD-PARTY-NOTICES.txt`, identical to the source. On
  Windows, Tauri puts resources beside the executable, and on Linux in the app's folder under
  `/usr/lib/`; those were not built here. `LICENSE.txt` and `NOTICE.txt` were added later and
  have not been checked in a built bundle.
- **People can find them.** The app's About (Settings, the command palette, and the app menu on
  macOS) has buttons that open the licence, the credits, and the third-party notices from the
  copies inside the app.

Still to do:

- [ ] Regenerate before every release (`make licenses`), and make
      `node scripts/licenses.mjs --check` part of the release steps. The file is committed, so a
      lockfile change without a regenerate leaves it stale. Nothing in CI checks this yet.
- [ ] If `pf-cli` is ever distributed, it needs notices of its own (it links `clap` and other
      packages the app does not).
- [ ] Check the built macOS, Windows, and Linux packages contain all three files, and that the
      About's buttons open them.

## 4. The dependencies against the GPL

Every shipped package's licence was held against the GNU GPL, version 3. The generated part of
[dependencies.md](dependencies.md) has the list ("Shipped packages against PixelFlow's licence"),
and `scripts/licenses.mjs --check` fails if a shipped package ever offers no compatible licence.

- **Compatible, by the Free Software Foundation's licence list:** MIT, the BSD licences, ISC,
  Zlib, Apache-2.0, the Unlicense, CC0, and MPL-2.0. Apache-2.0 is compatible with version 3 of
  the GPL and not with version 2, which is one more reason the project is not offered under
  version 2. That covers all but 18 of the roughly 500 shipped packages.
- **Unclear only in that the list does not name them** (18 packages): `Unicode-3.0` (16: the
  ICU4X libraries and Unicode tables), `CDLA-Permissive-2.0` (`webpki-roots`: the list of trusted
  certificate authorities, as data), and `0BSD` (`tslib`). Each is a short permissive licence.
  The FSF lists the earlier Unicode licence as compatible; version 3.0 of it, the CDLA, and 0BSD
  are not on the list either way. Reading them, nothing limits distribution under the GPL. A
  lawyer or the FSF could confirm it.
- **Incompatible:** none found.
- **MPL-2.0** packages (item 7) state plain `MPL-2.0`, not the variant marked "Incompatible
  With Secondary Licenses", so the MPL's own section 3.3 allows them in a GPL program.
- **Native code that is not a package** (the table in [dependencies.md](dependencies.md),
  "Native code, compiled in or linked"): OpenH264 is BSD-2-Clause; zstd is taken under its BSD
  option; `ring` carries Apache-2.0 and ISC notices, and its licence files hold no OpenSSL or
  SSLeay terms (the old advertising clause, which the GPL does not allow, is gone from the
  version shipped).
- **System libraries.** macOS's frameworks and Windows' own libraries are what the GPL calls
  System Libraries, which it leaves out of the source that must be offered. On Linux the app
  links GTK 3, WebKitGTK, libsoup, ALSA, and D-Bus, which are LGPL (libdbus offers GPL version 2
  or later as one choice): all compatible. Microsoft's WebView2 runtime is installed separately
  by Microsoft's own bootstrapper; putting a copy of it inside PixelFlow's installer would be
  distributing a proprietary component alongside a GPL program, and should be asked about first.

## 5. Data and models

- [x] **CMU Pronouncing Dictionary.** Compiled into the app. Its licence (BSD-style, two
      clauses, compatible with the GPL) asks that binary redistributions reproduce its copyright
      notice, conditions, and disclaimer "in the documentation and/or other materials provided
      with the distribution". The notices file carries it.
- [ ] **wav2vec2 speech model.** Apache-2.0. PixelFlow does not ship it: the user's copy of the
      app downloads it from Hugging Face (a pinned revision, checked against a SHA-256) when the
      user asks. The notices file names it and its source. If it is ever put in the installer,
      Apache-2.0's redistribution terms apply to PixelFlow directly. Hugging Face's own terms of
      service cover the download.
- [x] **Mouth-shape mapping.** From xLights' `phoneme_mapping`, which came from Papagayo
      (copyright 2005 Mike Clifton, GPL version 2 or any later version). "Or any later version"
      includes version 3, so it can be part of a GPL-3.0-only program. What the GPL asks is that
      its copyright notice be kept and its licence stated: `NOTICE` does both.
- [x] **Tables and outlines taken from xLights** (shape outlines, snowflake and swag patterns,
      the cube and window-frame tables, effect defaults and ranges, value-curve formulae, the
      blur box sizes, Falcon tables: provenance.md, "Borrowed data"). They arrive under xLights'
      licence and are covered by the same credit as the code.
- [x] **Picture resizing that follows wxWidgets** (`pf-render/src/picture/resample.rs`, about 45
      lines). The wxWindows Library Licence 3.1 is on the FSF's list as compatible with the GPL.
      `NOTICE` credits it.
- [ ] **FPP limits and checks, WLED codes** (per PixelFlow's own comments; their source was not
      read). FPP's licences are LGPL 2.1 or later and GPL 2 or later, both of which include
      version 3. WLED's is EUPL 1.2, which names the GPL among the licences its code may be
      combined under. What was taken are limits and numeric codes a client needs in order to
      talk to the device. No notice has been added for them; a credit line would cost nothing.
- [ ] **The pixel font** in `pf-render/src/text.rs` ("the classic 5 x 7 one"): its origin is not
      recorded. The GPL does not settle this: PixelFlow can only license what it has the right
      to. Find the source and its terms, or redraw it (95 glyphs).
- [ ] **Fire constants** in `pf-render/src/effects.rs` resemble the FastLED "Fire2012" demo
      (MIT). Not verified. If they came from it, MIT asks that its copyright notice be kept; MIT
      is compatible with the GPL.
- [ ] **Lyrics.** "Find lyrics" fetches lyric text from LRCLIB, a community database, at the
      user's request, and stores it in the user's own sequence. Song lyrics are generally
      copyrighted by their publishers. PixelFlow distributes none; whether offering the lookup
      raises anything is a question for a lawyer.
- [x] **Device fixtures** (`crates/pf-devices/fixtures/`): recorded from the owner's own devices
      and used only in tests. The prop names, the sequence name, and the controller name they
      carried have been replaced with made-up ones.

## 6. Patent-licensed codecs

A software licence (BSD, MIT, MPL, GPL) covers copyright. It does not grant rights under patents
that someone else holds over a format. PixelFlow contains:

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

### What this means for an open-source distribution

- **The copyright side is fine.** The encoders' code is BSD-2-Clause and MIT, both compatible
  with the GPL. Nothing in the GPL forbids shipping an implementation of a patented format.
- **The patent side does not go away because the program is free.** Patent pools license
  products, whether or not money is charged for them. Being open source is not an exemption,
  and neither is being given away.
- **It is common all the same.** Well-known GPL programs ship H.264 and AAC encoders in their
  own downloads (OBS Studio, HandBrake, and VLC among them), and many Linux distributions carry
  them. Others are more careful: Fedora leaves most patent-encumbered codecs out and takes
  H.264 only as Cisco's binary. That some projects do it is not a statement that it is safe for
  this one.
- **Where the risk falls.** On whoever distributes the binary: the owner, for official
  downloads, and anyone who redistributes them. Whether a pool pursues a free hobby program is
  a different question from whether it could.
- **The GPL's own patent terms** (version 3, sections 11 and 12) come into play if a distributor
  takes a patent licence. Section 12 says that someone who accepts terms that stop recipients
  from using their rights under the GPL (a licence that covers only the distributor's own
  copies, say) may not distribute at all. A distributor who has taken no such licence has made
  no such promise. This is the one place where patents and the GPL meet, and a lawyer should be
  asked before any patent licence is signed.

What a distributor can do, without a recommendation:

- [ ] Ship as it is, as many open-source projects do, knowing the above.
- [ ] Use Cisco's OpenH264 binary under its conditions (covers H.264 only, not AAC), fetched by
      the user's copy of the app, not put in the installer.
- [ ] Encode with the operating system's own encoders (macOS VideoToolbox, Windows Media
      Foundation), which the OS vendor has licensed. Not available the same way on Linux.
- [ ] Export a format without these pools (for example VP9 or AV1 video with Opus sound).
- [ ] Leave the built-in encoders out of distributed builds, leaving export through the user's
      own ffmpeg.
- [ ] Take the patent licences that apply, after asking how that sits with the GPL.
- [ ] Get advice, particularly on AAC, which has no equivalent of Cisco's arrangement.

## 7. MPL-2.0 packages

`symphonia` (12 crates), `triple_buffer`, and four packages Tauri uses are under the Mozilla
Public License 2.0. For an executable, section 3.2 asks that recipients be told how to get the
source of those files and that their rights in them are not restricted. Section 3.3 allows them
to be part of a larger work under the GPL.

- [x] The notices file lists each with its source URL and the MPL text. PixelFlow uses them
      unmodified.
- [ ] If any of them is ever patched for PixelFlow, the patched files must be published.

## 8. Packaging, per platform

- [ ] **Linux.** A `.deb` or `.rpm` names GTK 3, WebKitGTK, libsoup, ALSA, and D-Bus as
      dependencies and contains none of them. An **AppImage contains copies of them**. Most are
      LGPL, which then applies to PixelFlow as their distributor: carry their licence texts and
      make their source available. The notices file does not cover them, because they are not
      Cargo packages. `tauri.conf.json` has `"targets": "all"`, which builds an AppImage.
- [ ] **Windows.** The app needs Microsoft's WebView2 runtime. By default Tauri's installer
      downloads Microsoft's bootstrapper at install time; embedding the runtime in the installer
      is governed by Microsoft's redistribution terms (and see item 4).
- [ ] **macOS.** Uses the system's WebKit; nothing extra is redistributed.
- [ ] **zstd.** Compiled in. It is offered under BSD or GPL-2.0 at the recipient's choice; both
      texts are in the notices file with a note saying so.
- [ ] **App stores.** See item 1: ask before submitting a GPL app to Apple's.

## 9. Services the app talks to

Not redistribution, but part of offering the app. Details are in
[../privacy-and-data-flow.md](../privacy-and-data-flow.md).

- [ ] **OpenAI and Anthropic.** Users bring their own API keys, and their use is under their own
      agreement with the provider.
- [ ] **LRCLIB, Hugging Face, GitHub's API.** Each has terms of use for automated clients.
      PixelFlow identifies itself to LRCLIB and GitHub with a `User-Agent`.

## 10. Donations

- The GPL has nothing against asking for or accepting money. A donation buys nothing: the app,
  its source, and every feature are the same for everyone.
- The links live in two files: `.github/FUNDING.yml` (the Sponsor button on the repository) and
  `app/support.json` (the app's **Support PixelFlow** entry). An entry left empty is not shown
  anywhere. See [CONTRIBUTING.md](../../CONTRIBUTING.md#donation-links).
- What each donation service asks of the person receiving the money, and how donations are
  taxed, are the owner's own matters and outside this document.

## Summary of open decisions

1. What to do about H.264 and AAC (item 6).
2. Whether to ship an AppImage (item 8).
3. Whether to attach the dependencies' source to each release (item 1).
4. The pixel font's origin (item 5).
