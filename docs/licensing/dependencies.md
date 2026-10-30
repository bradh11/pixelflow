# Dependency licences

What PixelFlow's third-party dependencies say their licences are, for the three places
dependencies come from:

- **(a)** the engine workspace at the repo root (`Cargo.toml`, `Cargo.lock`): the `pf-*` crates,
  the `pf-cli` tool, and the `pf-bench` benchmark;
- **(b)** the desktop app (`app/src-tauri/Cargo.toml` and its own `Cargo.lock`), which links the
  engine crates plus Tauri: this is what an installer contains;
- **(c)** the app's UI (`app/package.json`, `app/pnpm-lock.yaml`), production dependencies only:
  what Vite bundles into the window's JavaScript.

This is an inventory of what each package's own manifest states. It is **not legal advice**, and
nobody has read every licence text against PixelFlow's use. Where a licence has conditions that
matter when shipping a binary, the notes below say what the licence's text asks for and leave
what to do about it to the owner. See [distribution-checklist.md](distribution-checklist.md).

## How to regenerate

```sh
cd app && pnpm install     # the UI packages must be installed
node scripts/licenses.mjs  # or: make licenses
```

That rewrites everything below the "BEGIN GENERATED" line in this file, and
`app/src-tauri/resources/THIRD-PARTY-NOTICES.txt` (the notices the built app bundles).
`node scripts/licenses.mjs --check` changes nothing and exits 1 if either is out of date, if one
of PixelFlow's own manifests states a licence other than `GPL-3.0-only`, or if a shipped package
offers no licence compatible with it. The
script uses `cargo metadata` and the installed `node_modules`; it needs no other tool. Run it
whenever a lockfile changes, and before every release.

"Shipped" below means Cargo's dependency graph links the package into a binary for macOS,
Windows, or Linux (both x86-64 and ARM64 were resolved). It is deliberately the wide reading: a
package counts as shipped even if the linker later drops all of its code.

## What the inventory shows

The tables are below. In short:

- **No GPL, LGPL, or AGPL dependency is linked into the app.** `r-efi` offers LGPL as one of
  three choices, and is not used on any desktop target. (PixelFlow itself is under the GPL
  because of the code it derives from xLights, which is not a dependency: see
  [provenance.md](provenance.md).)
- **MPL-2.0 packages are shipped** (17 in the app, 13 in the engine): see the next section.
- **No package is missing a licence**, and none states one the script does not recognize.
- **40 shipped packages name a licence but include no licence file.** The notices file prints the
  standard text of the licence they name (from `scripts/license-texts/`, copied from the SPDX
  licence list, version 3.29.0), with the package's authors as stated in its manifest.
- **The UI's production packages are all MIT, ISC, 0BSD, or "Apache-2.0 OR MIT".**
- **PixelFlow's own packages all state `GPL-3.0-only`**, the licence in the `LICENSE` file at
  the repo root.
- **Every shipped package offers a licence that can be part of a GPL-3.0-only program.** For 18
  of them that rests on a permissive licence the Free Software Foundation's list does not name
  (`Unicode-3.0`, `CDLA-Permissive-2.0`, `0BSD`); they are listed apart, under "Shipped packages
  against PixelFlow's licence". The script fails its check if a shipped package ever offers no
  compatible licence.

## Notes on the flagged packages

### MPL-2.0 (Mozilla Public License 2.0)

| Package | Why it is there | Scope |
|---|---|---|
| `symphonia` and its 11 codec and format crates | Decoding music (MP3, AAC, FLAC, Vorbis, WAV, MP4), through `rodio` and directly in `pf-audio` | (a) and (b), all platforms |
| `triple_buffer` | Handing finished frames to the thread that sends them; a direct dependency of `pf-frame` | (a) and (b), all platforms |
| `cssparser`, `selectors`, `dtoa-short` | Tauri's HTML handling (`tauri-utils` through `dom_query`) | (b), all platforms |
| `cssparser-macros` | The same, at build time | (b), build only |
| `option-ext` | Tauri's folder lookup (`dirs`) | (b), all platforms |

What the licence's text asks of someone distributing an executable (section 3.2): make the
source form of the MPL-covered files available to recipients, tell them how to get it, and do not
restrict their rights in those files. It is a file-level licence: section 3.3 lets the covered
files be combined into a larger work under other terms. PixelFlow uses all of these unmodified
from crates.io, where their source is published. The notices file lists each with its source
URL and the MPL text. Each states plain `MPL-2.0` in its manifest, not
`MPL-2.0-no-copyleft-exception` (the identifier for code marked "Incompatible With Secondary
Licenses").

### Packages with terms of their own

| Package | Licence | What it is |
|---|---|---|
| `webpki-roots` | `CDLA-Permissive-2.0` | Mozilla's list of trusted certificate authorities, as data, used for HTTPS to the AI providers, LRCLIB, and Hugging Face. A permissive data licence; it asks that the licence text travel with the data. |
| `icu_*`, `zerovec`, `yoke`, `litemap`, `tinystr`, `writeable`, `potential_utf`, `zerofrom`, `zerotrie` (15 shipped) | `Unicode-3.0` | Unicode data and the ICU4X libraries, through `url` and `idna`. Permissive; asks for the copyright and permission notice. |
| `unicode-ident` | `(MIT OR Apache-2.0) AND Unicode-3.0` | Unicode tables. |
| `anymap3` | `BlueOak-1.0.0 OR MIT OR Apache-2.0` | A choice; MIT and Apache-2.0 are among the options. |
| `target-lexicon` | `Apache-2.0 WITH LLVM-exception` | Build time only. |

### Native code, compiled in or linked

Cargo's licence field describes the Rust package. Several packages also compile in, or link to,
code that is not Rust and has its own terms.

| Package | What it brings | Licence of that code | Platforms |
|---|---|---|---|
| `openh264-sys2` (feature `libloading`) | The app has no H.264 encoder of its own: it loads Cisco's OpenH264 binary at run time (`libloading`), which the user's copy downloads from Cisco when the user agrees (the next table). The crate's `source` feature, which compiles the C++ source inside it (`upstream/`), is turned on only for tests and developer tools (`pf-video`'s `encoder-from-source`); `node scripts/encoder-check.mjs` fails if the app's builds turn it on. | BSD-2-Clause. **Patents are a separate matter:** see the distribution checklist, item 6. | all |
| `zstd-sys` | The zstd library, compiled from the C source inside the crate | Offered under BSD-3-Clause **or** GPL-2.0, at the recipient's choice (`zstd/LICENSE`, `zstd/COPYING`). Both texts are in the notices, with a note. | all |
| `ring` | Cryptography, partly C and assembly derived from BoringSSL and OpenSSL | `Apache-2.0 AND ISC`, with the notices in its `LICENSE` files | all |
| `oxideav-aac` | An AAC-LC encoder, in Rust | MIT. Patents are a separate matter, as for H.264. | all |
| `alsa-sys`, `libdbus-sys`, the `gtk`, `gdk`, `glib`, `gio`, `pango`, `cairo`, `webkit2gtk`, `soup3` crates | Bindings to system libraries, linked at run time: ALSA, D-Bus, GTK 3, WebKitGTK, libsoup | The bindings are MIT. The system libraries are mostly LGPL (libdbus is AFL-2.1 or GPL-2.0-or-later) and are **not** in the binary. Whether they are in the installer depends on the package format: see the checklist. | Linux |
| `objc2-*`, `core-foundation*`, `coreaudio-rs` crates | Bindings to Apple's system frameworks | The bindings are MIT, Zlib, or Apache-2.0. The frameworks are part of macOS. | macOS |
| `webview2-com`, `windows-*` crates | Bindings to Windows and to Microsoft's WebView2 | The bindings are MIT or Apache-2.0. The WebView2 runtime is Microsoft's and is installed separately (by default, Tauri's installer downloads Microsoft's bootstrapper). | Windows |

`libdbus-sys` also contains a copy of libdbus's source (`vendor/dbus/`), used only with its
`vendored` feature, which PixelFlow's build does not turn on.

### Shipped without being a "dependency"

The inventory reads package manifests, so it cannot see these. They are listed by hand in
`scripts/licenses.mjs` so the notices include them.

| Item | Where | Licence | Shipped? |
|---|---|---|---|
| CMU Pronouncing Dictionary | `crates/pf-lexicon/data/cmudict.zst`, compiled into the binary | BSD-style, two clauses (`cmudict-LICENSE.txt` beside it) | yes |
| wav2vec2-base-960h speech model (ONNX conversion) | Downloaded from Hugging Face when the user turns on lyric alignment; URL and SHA-256 pinned in `crates/pf-align/src/store.rs` | Apache-2.0 | no: fetched by the user's copy of the app |
| Cisco's OpenH264 2.6.0 binary ("OpenH264 Video Codec provided by Cisco Systems, Inc.") | Downloaded from Cisco's server (`ciscobinary.openh264.org`) when the user agrees, for the first video export with the built-in encoder; URL and SHA-256 per platform pinned in `crates/pf-video/src/cisco.rs` | BSD-2-Clause, with Cisco's AVC/H.264 patent licence conditions. Cisco's `BINARY_LICENSE.txt` (`crates/pf-video/data/`) is reproduced in the notices and in Settings, as its condition 4 asks. | no: fetched by the user's copy of the app |
| Tailwind CSS | A development dependency, but the stylesheet it generates (including its "preflight" reset) is in the bundle | MIT | its output is |
| Vite and Rolldown | Development dependencies; a few lines of module-loading helper code end up in the bundle | MIT | a few lines |
| Lucide icons | In (c) as `lucide-react`; its licence file notes that some icons derive from Feather (MIT) | ISC, with an MIT part | yes |
| The Inter font | **Not bundled.** `app/src/styles.css` names it first in the font list; a machine without it uses the system font. | n/a | no |
| App icons | `app/src-tauri/icons/`, `app/app-icon.png` | PixelFlow's own | yes |
| The sample show | `app/src/api/sampleShow.json`, compiled in | PixelFlow's own | yes |

Code and data taken from xLights and other projects' **source** are a different subject: see
[provenance.md](provenance.md).

## Build time versus shipped

- **Shipped:** linked into the app binary (b) or bundled into its JavaScript (c). Their notices
  go in `THIRD-PARTY-NOTICES.txt`.
- **Build time only:** build scripts, procedural macros, the Tauri CLI, Vite, TypeScript,
  Tailwind's compiler, the test libraries. They run on the build machine and are not distributed.
  Their licences matter to whoever builds PixelFlow, not to whoever receives it.
- **Tests only:** `proptest`, `tempfile`, and similar. Not distributed.
- **(a) versus (b):** an installer contains (b) and (c). (a) adds what only `pf-cli` and
  `pf-bench` use (`clap` and its helpers); it matters if a command-line tool is ever distributed
  on its own, which would need its own notices.

<!-- BEGIN GENERATED (scripts/licenses.mjs): do not edit below this line by hand -->

## Counts

| Scope | Third-party packages | Shipped | Build-time only | Tests only | Not used on desktop targets |
| --- | --- | --- | --- | --- | --- |
| (a) Engine workspace (`Cargo.toml`) | 431 | 304 | 41 | 35 | 51 |
| (b) Desktop app (`app/src-tauri/Cargo.toml`) | 642 | 460 | 97 | 1 | 84 |
| (c) App UI, production (`app/package.json`) | 34 | 34 | 0 | 0 | 0 |

"Shipped" means linked into a binary for macOS, Windows, or Linux. "Build-time only" covers build scripts and procedural macros, which run while compiling and are not in the binary. "Not used on desktop targets" are packages the lockfile lists for other platforms (Android, iOS, WebAssembly).

## PixelFlow's own packages

All 25 of PixelFlow's own packages state `GPL-3.0-only`: the GNU General Public License, version 3 (the `LICENSE` file at the repo root).

## Shipped packages against PixelFlow's licence (`GPL-3.0-only`)

Of the 510 packages shipped (engine, app, and UI together), 492 offer a licence the Free Software Foundation's licence list names as compatible with the GNU GPL, version 3. Where a package offers a choice, one compatible option is enough.

**Shipped under a permissive licence that list does not name (read, and nothing in it limits distribution under the GPL; not confirmed by anyone with authority)** (18):

- `icu_collections` 2.3.0: `Unicode-3.0`
- `icu_locale_core` 2.3.0: `Unicode-3.0`
- `icu_normalizer` 2.3.0: `Unicode-3.0`
- `icu_normalizer_data` 2.3.0: `Unicode-3.0`
- `icu_properties` 2.3.0: `Unicode-3.0`
- `icu_properties_data` 2.3.0: `Unicode-3.0`
- `icu_provider` 2.3.1: `Unicode-3.0`
- `litemap` 0.8.3: `Unicode-3.0`
- `potential_utf` 0.1.6: `Unicode-3.0`
- `tinystr` 0.8.4: `Unicode-3.0`
- `tslib` 2.8.1: `0BSD`
- `unicode-ident` 1.0.26: `(MIT OR Apache-2.0) AND Unicode-3.0`
- `webpki-roots` 1.0.11: `CDLA-Permissive-2.0`
- `writeable` 0.6.4: `Unicode-3.0`
- `yoke` 0.8.3: `Unicode-3.0`
- `zerofrom` 0.1.8: `Unicode-3.0`
- `zerotrie` 0.2.5: `Unicode-3.0`
- `zerovec` 0.11.8: `Unicode-3.0`

**Shipped with no licence known to be compatible:** none.

## (a) Engine workspace: shipped packages by licence

| Licence (as the package states it) | Packages | Flag |
| --- | --- | --- |
| `Apache-2.0 OR MIT` | 206 |  |
| `MIT` | 35 |  |
| `MPL-2.0` | 14 | copyleft |
| `Apache-2.0 OR MIT OR Zlib` | 13 |  |
| `MIT OR Unlicense` | 7 |  |
| `BSD-3-Clause` | 4 |  |
| `Apache-2.0 OR BSD-2-Clause OR MIT` | 3 |  |
| `ISC` | 3 |  |
| `Apache-2.0` | 2 |  |
| `Apache-2.0 OR Apache-2.0 WITH LLVM-exception OR MIT` | 2 |  |
| `Apache-2.0 OR BSD-3-Clause` | 2 |  |
| `BSD-2-Clause` | 2 |  |
| `Zlib` | 2 |  |
| `(Apache-2.0 OR MIT) AND BSD-3-Clause` | 1 |  |
| `(MIT OR Apache-2.0) AND Unicode-3.0` | 1 | own terms |
| `0BSD OR Apache-2.0 OR MIT` | 1 |  |
| `Apache-2.0 AND ISC` | 1 |  |
| `Apache-2.0 OR BlueOak-1.0.0 OR MIT` | 1 |  |
| `Apache-2.0 OR ISC OR MIT` | 1 |  |
| `BSD-3-Clause OR MIT` | 1 |  |
| `CDLA-Permissive-2.0` | 1 | own terms |
| `GPL-3.0-only OR MIT` | 1 | copyleft is one option |

## (b) Desktop app: shipped packages by licence

| Licence (as the package states it) | Packages | Flag |
| --- | --- | --- |
| `Apache-2.0 OR MIT` | 277 |  |
| `MIT` | 82 |  |
| `Apache-2.0 OR MIT OR Zlib` | 20 |  |
| `MPL-2.0` | 18 | copyleft |
| `Unicode-3.0` | 15 | own terms |
| `MIT OR Unlicense` | 11 |  |
| `BSD-3-Clause` | 6 |  |
| `ISC` | 4 |  |
| `Apache-2.0` | 3 |  |
| `Apache-2.0 OR BSD-2-Clause OR MIT` | 3 |  |
| `Apache-2.0 OR Apache-2.0 WITH LLVM-exception OR MIT` | 2 |  |
| `Apache-2.0 OR BSD-3-Clause` | 2 |  |
| `BSD-2-Clause` | 2 |  |
| `BSD-3-Clause OR MIT` | 2 |  |
| `Zlib` | 2 |  |
| `(Apache-2.0 OR MIT) AND BSD-3-Clause` | 1 |  |
| `(MIT OR Apache-2.0) AND Unicode-3.0` | 1 | own terms |
| `0BSD OR Apache-2.0 OR MIT` | 1 |  |
| `Apache-2.0 AND ISC` | 1 |  |
| `Apache-2.0 AND MIT` | 1 |  |
| `Apache-2.0 OR BlueOak-1.0.0 OR MIT` | 1 |  |
| `Apache-2.0 OR CC0-1.0 OR MIT-0` | 1 |  |
| `Apache-2.0 OR ISC OR MIT` | 1 |  |
| `BSD-3-Clause AND MIT` | 1 |  |
| `CDLA-Permissive-2.0` | 1 | own terms |
| `GPL-3.0-only OR MIT` | 1 | copyleft is one option |

## (c) App UI: production packages by licence

| Licence (as the package states it) | Packages | Flag |
| --- | --- | --- |
| `MIT` | 30 |  |
| `Apache-2.0 OR MIT` | 2 |  |
| `0BSD` | 1 |  |
| `ISC` | 1 |  |

## Flags

Across both Cargo workspaces (every use, not only shipped) and the app UI's production packages.

**Copyleft licence applies** (19):

- `cssparser` 0.37.0: `MPL-2.0` (shipped: all)
- `cssparser-macros` 0.7.1: `MPL-2.0` (build)
- `dtoa-short` 0.3.5: `MPL-2.0` (shipped: all)
- `option-ext` 0.2.0: `MPL-2.0` (shipped: all)
- `selectors` 0.38.0: `MPL-2.0` (shipped: all)
- `serialport` 4.10.1: `MPL-2.0` (shipped: all)
- `symphonia` 0.5.5: `MPL-2.0` (shipped: all)
- `symphonia-bundle-flac` 0.5.5: `MPL-2.0` (shipped: all)
- `symphonia-bundle-mp3` 0.5.5: `MPL-2.0` (shipped: all)
- `symphonia-codec-aac` 0.5.5: `MPL-2.0` (shipped: all)
- `symphonia-codec-pcm` 0.5.5: `MPL-2.0` (shipped: all)
- `symphonia-codec-vorbis` 0.5.5: `MPL-2.0` (shipped: all)
- `symphonia-core` 0.5.5: `MPL-2.0` (shipped: all)
- `symphonia-format-isomp4` 0.5.5: `MPL-2.0` (shipped: all)
- `symphonia-format-ogg` 0.5.5: `MPL-2.0` (shipped: all)
- `symphonia-format-riff` 0.5.5: `MPL-2.0` (shipped: all)
- `symphonia-metadata` 0.5.5: `MPL-2.0` (shipped: all)
- `symphonia-utils-xiph` 0.5.5: `MPL-2.0` (shipped: all)
- `triple_buffer` 9.0.0: `MPL-2.0` (shipped: all)

**Copyleft is one of several options (a permissive option is offered)** (3):

- `r-efi` 5.3.0: `MIT OR Apache-2.0 OR LGPL-2.1-or-later` (not used on desktop targets)
- `r-efi` 6.0.0: `MIT OR Apache-2.0 OR LGPL-2.1-or-later` (not used on desktop targets)
- `unescaper` 0.1.10: `MIT OR GPL-3.0-only` (shipped: Linux)

**Permissive, with terms of their own** (20):

- `icu_collections` 2.3.0: `Unicode-3.0` (shipped: all)
- `icu_locale_core` 2.3.0: `Unicode-3.0` (shipped: all)
- `icu_normalizer` 2.3.0: `Unicode-3.0` (shipped: all)
- `icu_normalizer_data` 2.3.0: `Unicode-3.0` (shipped: all)
- `icu_properties` 2.3.0: `Unicode-3.0` (shipped: all)
- `icu_properties_data` 2.3.0: `Unicode-3.0` (shipped: all)
- `icu_provider` 2.3.1: `Unicode-3.0` (shipped: all)
- `litemap` 0.8.3: `Unicode-3.0` (shipped: all)
- `potential_utf` 0.1.6: `Unicode-3.0` (shipped: all)
- `tinystr` 0.8.4: `Unicode-3.0` (shipped: all)
- `unicode-ident` 1.0.26: `(MIT OR Apache-2.0) AND Unicode-3.0` (shipped: all)
- `webpki-roots` 1.0.11: `CDLA-Permissive-2.0` (shipped: all)
- `writeable` 0.6.4: `Unicode-3.0` (shipped: all)
- `yoke` 0.8.3: `Unicode-3.0` (shipped: all)
- `yoke-derive` 0.8.4: `Unicode-3.0` (build)
- `zerofrom` 0.1.8: `Unicode-3.0` (shipped: all)
- `zerofrom-derive` 0.1.8: `Unicode-3.0` (build)
- `zerotrie` 0.2.5: `Unicode-3.0` (shipped: all)
- `zerovec` 0.11.8: `Unicode-3.0` (shipped: all)
- `zerovec-derive` 0.11.6: `Unicode-3.0` (build)

**Unknown, missing, or unrecognized licence:** none.

**Shipped in the app with no licence file in the package (the notices use the standard text of the stated licence)** (40):

- `alloc-stdlib` 0.3.0: `BSD-3-Clause`
- `block2` 0.6.2: `MIT`
- `dasp_sample` 0.11.0: `MIT OR Apache-2.0`
- `dispatch2` 0.3.1: `Zlib OR Apache-2.0 OR MIT`
- `dlopen2` 0.8.2: `MIT`
- `libappindicator-sys` 0.9.0: `Apache-2.0 OR MIT`
- `nom-language` 0.1.0: `MIT`
- `objc2` 0.6.4: `MIT`
- `objc2-app-kit` 0.3.2: `Zlib OR Apache-2.0 OR MIT`
- `objc2-audio-toolbox` 0.3.2: `Zlib OR Apache-2.0 OR MIT`
- `objc2-core-audio` 0.3.2: `Zlib OR Apache-2.0 OR MIT`
- `objc2-core-audio-types` 0.3.2: `Zlib OR Apache-2.0 OR MIT`
- `objc2-core-foundation` 0.3.2: `Zlib OR Apache-2.0 OR MIT`
- `objc2-core-graphics` 0.3.2: `Zlib OR Apache-2.0 OR MIT`
- `objc2-encode` 4.1.0: `MIT`
- `objc2-exception-helper` 0.1.1: `Zlib OR Apache-2.0 OR MIT`
- `objc2-foundation` 0.3.2: `MIT`
- `objc2-io-surface` 0.3.2: `Zlib OR Apache-2.0 OR MIT`
- `objc2-quartz-core` 0.3.2: `Zlib OR Apache-2.0 OR MIT`
- `objc2-web-kit` 0.3.2: `Zlib OR Apache-2.0 OR MIT`
- `openh264` 0.9.8: `BSD-2-Clause`
- `realfft` 3.5.0: `MIT`
- `selectors` 0.38.0: `MPL-2.0`
- `symphonia` 0.5.5: `MPL-2.0`
- `symphonia-bundle-flac` 0.5.5: `MPL-2.0`
- `symphonia-bundle-mp3` 0.5.5: `MPL-2.0`
- `symphonia-codec-aac` 0.5.5: `MPL-2.0`
- `symphonia-codec-pcm` 0.5.5: `MPL-2.0`
- `symphonia-codec-vorbis` 0.5.5: `MPL-2.0`
- `symphonia-core` 0.5.5: `MPL-2.0`
- `symphonia-format-isomp4` 0.5.5: `MPL-2.0`
- `symphonia-format-ogg` 0.5.5: `MPL-2.0`
- `symphonia-format-riff` 0.5.5: `MPL-2.0`
- `symphonia-metadata` 0.5.5: `MPL-2.0`
- `symphonia-utils-xiph` 0.5.5: `MPL-2.0`
- `tract-extra` 0.23.8: `MIT OR Apache-2.0`
- `tract-transformers` 0.23.8: `MIT OR Apache-2.0`
- `webview2-com` 0.39.1: `MIT`
- `webview2-com-sys` 0.39.1: `MIT`
- `react-remove-scroll-bar` 2.3.8: `MIT`

## Every Cargo package

| Package | Version | Licence | (a) Engine | (b) App |
| --- | --- | --- | --- | --- |
| `adler2` | 2.0.1 | `0BSD OR MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `aes` | 0.9.3 | `MIT OR Apache-2.0` | shipped (Linux) | shipped (Linux) |
| `ahash` | 0.8.12 | `MIT OR Apache-2.0` | tests |  |
| `aho-corasick` | 1.1.5 | `Unlicense OR MIT` | shipped (all) | shipped (all) |
| `alloc-no-stdlib` | 3.0.0 | `BSD-3-Clause` |  | shipped (all) |
| `alloc-stdlib` | 0.3.0 | `BSD-3-Clause` |  | shipped (all) |
| `allocator-api2` | 0.2.21 | `MIT OR Apache-2.0` | shipped (all) |  |
| `alsa` | 0.11.0 | `Apache-2.0 OR MIT` | shipped (Linux) | shipped (Linux) |
| `alsa-sys` | 0.4.0 | `MIT` | shipped (Linux) | shipped (Linux) |
| `android_system_properties` | 0.1.6 | `MIT OR Apache-2.0` |  | not on desktop |
| `anstream` | 1.0.0 | `MIT OR Apache-2.0` | shipped (all) |  |
| `anstyle` | 1.0.14 | `MIT OR Apache-2.0` | shipped (all) |  |
| `anstyle-parse` | 1.0.0 | `MIT OR Apache-2.0` | shipped (all) |  |
| `anstyle-query` | 1.1.5 | `MIT OR Apache-2.0` | shipped (all) |  |
| `anstyle-wincon` | 3.0.11 | `MIT OR Apache-2.0` | shipped (Windows) |  |
| `anyhow` | 1.0.104 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `anymap3` | 1.1.0 | `BlueOak-1.0.0 OR MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `apple-native-keyring-store` | 1.0.2 | `MIT OR Apache-2.0` | shipped (macOS) | shipped (macOS) |
| `arrayvec` | 0.7.8 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `async-broadcast` | 0.7.2 | `MIT OR Apache-2.0` | shipped (Linux) | shipped (Linux) |
| `async-channel` | 2.5.0 | `Apache-2.0 OR MIT` | shipped (Linux) | shipped (Linux) |
| `async-executor` | 1.14.0 | `Apache-2.0 OR MIT` | shipped (Linux) | shipped (Linux) |
| `async-io` | 2.6.0 | `Apache-2.0 OR MIT` | shipped (Linux) | shipped (Linux) |
| `async-lock` | 3.4.2 | `Apache-2.0 OR MIT` | shipped (Linux) | shipped (Linux) |
| `async-process` | 2.5.0 | `Apache-2.0 OR MIT` | shipped (Linux) | shipped (Linux) |
| `async-recursion` | 1.2.0 | `MIT OR Apache-2.0` | build | build |
| `async-signal` | 0.2.14 | `Apache-2.0 OR MIT` | shipped (Linux) | shipped (Linux) |
| `async-task` | 4.7.1 | `Apache-2.0 OR MIT` | shipped (Linux) | shipped (Linux) |
| `async-trait` | 0.1.92 | `MIT OR Apache-2.0` | build | build |
| `atk` | 0.18.2 | `MIT` |  | shipped (Linux) |
| `atk-sys` | 0.18.2 | `MIT` |  | shipped (Linux) |
| `atomic-waker` | 1.1.2 | `Apache-2.0 OR MIT` | shipped (Linux) | shipped (Linux) |
| `autocfg` | 1.5.1 | `Apache-2.0 OR MIT` | build | build |
| `base64` | 0.21.7 | `MIT OR Apache-2.0` |  | shipped (macOS) |
| `base64` | 0.22.1 | `MIT OR Apache-2.0` |  | build |
| `base64` | 0.23.1 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `bit-set` | 0.11.1 | `Apache-2.0 OR MIT` | shipped (all) | shipped (all) |
| `bit-set` | 0.8.0 | `Apache-2.0 OR MIT` | tests | shipped (all) |
| `bit-vec` | 0.10.1 | `Apache-2.0 OR MIT` | shipped (all) | shipped (all) |
| `bit-vec` | 0.8.0 | `Apache-2.0 OR MIT` | tests | shipped (all) |
| `bitflags` | 1.3.2 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `bitflags` | 2.13.2 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `block-buffer` | 0.10.4 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `block-buffer` | 0.12.1 | `MIT OR Apache-2.0` | shipped (Linux) | shipped (Linux) |
| `block-padding` | 0.4.2 | `MIT OR Apache-2.0` | shipped (Linux) | shipped (Linux) |
| `block2` | 0.6.2 | `MIT` | shipped (macOS) | shipped (macOS) |
| `blocking` | 1.7.0 | `Apache-2.0 OR MIT` | shipped (Linux) | shipped (Linux) |
| `borrow-or-share` | 0.2.4 | `MIT-0` | tests |  |
| `borsh` | 1.8.1 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `borsh-derive` | 1.8.1 | `Apache-2.0` | build | build |
| `brotli` | 9.0.0 | `BSD-3-Clause AND MIT` |  | shipped (all) |
| `brotli-decompressor` | 6.0.1 | `BSD-3-Clause OR MIT` |  | shipped (all) |
| `bs58` | 0.5.1 | `MIT OR Apache-2.0` |  | shipped (all) |
| `bumpalo` | 3.20.3 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `bytecount` | 0.6.9 | `Apache-2.0 OR MIT` | tests |  |
| `bytemuck` | 1.25.2 | `Zlib OR Apache-2.0 OR MIT` | shipped (all) | shipped (all) |
| `byteorder` | 1.5.0 | `Unlicense OR MIT` | shipped (all) | shipped (all) |
| `byteorder-lite` | 0.1.0 | `Unlicense OR MIT` | shipped (all) | shipped (all) |
| `bytes` | 1.12.1 | `MIT` | shipped (all) | shipped (all) |
| `bzip2-rs` | 0.1.2 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `cairo-rs` | 0.18.5 | `MIT` |  | shipped (Linux) |
| `cairo-sys-rs` | 0.18.2 | `MIT` |  | shipped (Linux) |
| `camino` | 1.2.6 | `MIT OR Apache-2.0` |  | shipped (all) |
| `cargo-platform` | 0.1.9 | `MIT OR Apache-2.0` |  | shipped (all) |
| `cargo_metadata` | 0.19.2 | `MIT` |  | shipped (all) |
| `cargo_toml` | 1.0.1 | `Apache-2.0 OR MIT` |  | build |
| `cbc` | 0.2.1 | `MIT OR Apache-2.0` | shipped (Linux) | shipped (Linux) |
| `cc` | 1.6.0 | `MIT OR Apache-2.0` | build | build |
| `cesu8` | 1.1.0 | `Apache-2.0 OR MIT` | not on desktop | not on desktop |
| `cfb` | 0.14.0 | `MIT` |  | shipped (all) |
| `cfg-expr` | 0.15.8 | `MIT OR Apache-2.0` |  | build |
| `cfg-if` | 1.0.5 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `cfg_aliases` | 0.2.2 | `MIT` | build | build |
| `chacha20` | 0.10.2 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `chrono` | 0.4.45 | `MIT OR Apache-2.0` |  | shipped (all) |
| `cipher` | 0.5.2 | `MIT OR Apache-2.0` | shipped (Linux) | shipped (Linux) |
| `clap` | 4.6.7 | `MIT OR Apache-2.0` | shipped (all) |  |
| `clap_builder` | 4.6.7 | `MIT OR Apache-2.0` | shipped (all) |  |
| `clap_derive` | 4.6.7 | `MIT OR Apache-2.0` | build |  |
| `clap_lex` | 1.1.1 | `MIT OR Apache-2.0` | shipped (all) |  |
| `cmov` | 0.5.4 | `Apache-2.0 OR MIT` | shipped (Linux) | shipped (Linux) |
| `color_quant` | 1.1.0 | `MIT` | shipped (all) | shipped (all) |
| `colorchoice` | 1.0.5 | `MIT OR Apache-2.0` | shipped (all) |  |
| `combine` | 4.6.8 | `MIT` | not on desktop | not on desktop |
| `concurrent-queue` | 2.5.0 | `Apache-2.0 OR MIT` | shipped (Linux) | shipped (Linux) |
| `const-oid` | 0.10.2 | `Apache-2.0 OR MIT` | shipped (Linux) | shipped (Linux) |
| `cookie` | 0.18.2 | `MIT OR Apache-2.0` |  | shipped (all) |
| `core-foundation` | 0.10.1 | `MIT OR Apache-2.0` | shipped (macOS) | shipped (macOS) |
| `core-foundation-sys` | 0.8.7 | `MIT OR Apache-2.0` | shipped (macOS) | shipped (macOS) |
| `core-graphics` | 0.25.0 | `MIT OR Apache-2.0` |  | shipped (macOS) |
| `core-graphics-types` | 0.2.0 | `MIT OR Apache-2.0` |  | shipped (macOS) |
| `core_detect` | 1.0.0 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `coreaudio-rs` | 0.14.2 | `MIT OR Apache-2.0` | shipped (macOS) | shipped (macOS) |
| `cpal` | 0.17.3 | `Apache-2.0` | shipped (all) | shipped (all) |
| `cpubits` | 0.1.1 | `MIT OR Apache-2.0` | shipped (Linux) | shipped (Linux) |
| `cpufeatures` | 0.2.17 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `cpufeatures` | 0.3.1 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `crc32fast` | 1.5.2 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `crossbeam-channel` | 0.5.17 | `MIT OR Apache-2.0` |  | shipped (all) |
| `crossbeam-deque` | 0.8.8 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `crossbeam-epoch` | 0.9.21 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `crossbeam-utils` | 0.8.23 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `crunchy` | 0.2.4 | `MIT` | not on desktop | not on desktop |
| `crypto-common` | 0.1.7 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `crypto-common` | 0.2.2 | `MIT OR Apache-2.0` | shipped (Linux) | shipped (Linux) |
| `cssparser` | 0.37.0 | `MPL-2.0` |  | shipped (all) |
| `cssparser-macros` | 0.7.1 | `MPL-2.0` |  | build |
| `ctor` | 1.0.13 | `Apache-2.0 OR MIT` |  | shipped (all) |
| `ctrlc` | 3.5.2 | `MIT OR Apache-2.0` | shipped (all) |  |
| `ctutils` | 0.4.7 | `Apache-2.0 OR MIT` | shipped (Linux) | shipped (Linux) |
| `darling` | 0.24.1 | `MIT` |  | build |
| `darling_core` | 0.24.1 | `MIT` |  | build |
| `darling_macro` | 0.24.1 | `MIT` |  | build |
| `dasp_sample` | 0.11.0 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `data-encoding` | 2.11.1 | `MIT` | tests |  |
| `dbus` | 0.9.12 | `Apache-2.0 OR MIT` |  | shipped (Linux) |
| `defmt` | 1.1.1 | `MIT OR Apache-2.0` |  | shipped (all) |
| `defmt-macros` | 1.1.1 | `MIT OR Apache-2.0` |  | build |
| `defmt-parser` | 1.0.0 | `MIT OR Apache-2.0` |  | build |
| `deranged` | 0.5.8 | `MIT OR Apache-2.0` |  | shipped (all) |
| `derive-new` | 0.7.0 | `MIT` | build | build |
| `derive_more` | 2.1.1 | `MIT` |  | shipped (all) |
| `derive_more-impl` | 2.1.1 | `MIT` |  | build |
| `digest` | 0.10.7 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `digest` | 0.11.3 | `MIT OR Apache-2.0` | shipped (Linux) | shipped (Linux) |
| `dirs` | 7.0.0 | `MIT OR Apache-2.0` |  | shipped (all) |
| `dirs-sys` | 0.5.0 | `MIT OR Apache-2.0` |  | shipped (all) |
| `dispatch2` | 0.3.1 | `Zlib OR Apache-2.0 OR MIT` | shipped (macOS) | shipped (macOS) |
| `displaydoc` | 0.2.7 | `MIT OR Apache-2.0` |  | build |
| `dlopen2` | 0.8.2 | `MIT` |  | shipped (Linux) |
| `dlopen2_derive` | 0.4.3 | `MIT` |  | build |
| `dom_query` | 0.28.0 | `MIT` |  | shipped (all) |
| `downcast-rs` | 2.0.2 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `dpi` | 0.1.2 | `Apache-2.0 AND MIT` |  | shipped (all) |
| `dtoa` | 1.0.11 | `MIT OR Apache-2.0` |  | shipped (all) |
| `dtoa-short` | 0.3.5 | `MPL-2.0` |  | shipped (all) |
| `dunce` | 1.0.5 | `CC0-1.0 OR MIT-0 OR Apache-2.0` |  | shipped (all) |
| `dyn-clone` | 1.0.20 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `dyn-hash` | 1.0.0 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `either` | 1.19.0 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `email_address` | 0.2.9 | `MIT` | tests |  |
| `embed-resource` | 3.0.11 | `MIT` |  | build |
| `embed_plist` | 1.2.2 | `MIT OR Apache-2.0` |  | shipped (macOS) |
| `encoding_rs` | 0.8.42 | `(Apache-2.0 OR MIT) AND BSD-3-Clause` | shipped (all) | shipped (all) |
| `endi` | 1.1.1 | `MIT` | shipped (Linux) | shipped (Linux) |
| `enumflags2` | 0.7.12 | `MIT OR Apache-2.0` | shipped (Linux) | shipped (Linux) |
| `enumflags2_derive` | 0.7.12 | `MIT OR Apache-2.0` | build | build |
| `equivalent` | 1.0.2 | `Apache-2.0 OR MIT` | shipped (all) | shipped (all) |
| `erased-serde` | 0.4.10 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `errno` | 0.3.14 | `MIT OR Apache-2.0` | shipped (macOS, Linux) | shipped (macOS, Linux) |
| `event-listener` | 5.4.2 | `Apache-2.0 OR MIT` | shipped (Linux) | shipped (Linux) |
| `event-listener-strategy` | 0.5.4 | `Apache-2.0 OR MIT` | shipped (Linux) | shipped (Linux) |
| `extended` | 0.1.0 | `MIT` | shipped (all) | shipped (all) |
| `fancy-regex` | 0.19.2 | `MIT` | tests |  |
| `fastrand` | 2.5.0 | `Apache-2.0 OR MIT` | shipped (all) | shipped (all) |
| `fdeflate` | 0.3.7 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `field-offset` | 0.3.6 | `MIT OR Apache-2.0` |  | shipped (Linux) |
| `filetime` | 0.2.29 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `find-msvc-tools` | 0.1.14 | `MIT OR Apache-2.0` | build | build |
| `flate2` | 1.1.10 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `float-ord` | 0.3.2 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `fluent-uri` | 0.4.1 | `MIT` | tests |  |
| `flume` | 0.12.0 | `Apache-2.0 OR MIT` | shipped (all) | shipped (all) |
| `fnv` | 1.0.7 | `Apache-2.0 OR MIT` | tests | shipped (all) |
| `foldhash` | 0.2.0 | `Zlib` | shipped (all) | shipped (all) |
| `foreign-types` | 0.5.0 | `MIT OR Apache-2.0` |  | shipped (macOS) |
| `foreign-types-macros` | 0.2.4 | `MIT OR Apache-2.0` |  | build |
| `foreign-types-shared` | 0.3.1 | `MIT OR Apache-2.0` |  | shipped (macOS) |
| `form_urlencoded` | 1.2.2 | `MIT OR Apache-2.0` |  | shipped (all) |
| `fraction` | 0.17.0 | `MIT OR Apache-2.0` | tests |  |
| `futures-channel` | 0.3.34 | `MIT OR Apache-2.0` |  | shipped (Linux) |
| `futures-core` | 0.3.34 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `futures-executor` | 0.3.34 | `MIT OR Apache-2.0` |  | shipped (Linux) |
| `futures-io` | 0.3.34 | `MIT OR Apache-2.0` | shipped (Linux) | shipped (Linux) |
| `futures-lite` | 2.6.1 | `Apache-2.0 OR MIT` | shipped (Linux) | shipped (Linux) |
| `futures-macro` | 0.3.34 | `MIT OR Apache-2.0` | build | build |
| `futures-sink` | 0.3.34 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `futures-task` | 0.3.34 | `MIT OR Apache-2.0` | shipped (Linux) | shipped (Linux) |
| `futures-util` | 0.3.34 | `MIT OR Apache-2.0` | shipped (Linux) | shipped (Linux) |
| `gdk` | 0.18.2 | `MIT` |  | shipped (Linux) |
| `gdk-pixbuf` | 0.18.5 | `MIT` |  | shipped (Linux) |
| `gdk-pixbuf-sys` | 0.18.0 | `MIT` |  | shipped (Linux) |
| `gdk-sys` | 0.18.2 | `MIT` |  | shipped (Linux) |
| `gdkwayland-sys` | 0.18.2 | `MIT` |  | shipped (Linux) |
| `gdkx11` | 0.18.2 | `MIT` |  | shipped (Linux) |
| `gdkx11-sys` | 0.18.2 | `MIT` |  | shipped (Linux) |
| `generic-array` | 0.14.7 | `MIT` | shipped (all) | shipped (all) |
| `getrandom` | 0.2.17 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `getrandom` | 0.3.4 | `MIT OR Apache-2.0` | tests | shipped (all) |
| `getrandom` | 0.4.3 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `gif` | 0.14.2 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `gio` | 0.18.4 | `MIT` |  | shipped (Linux) |
| `gio-sys` | 0.18.1 | `MIT` |  | shipped (Linux) |
| `glib` | 0.18.5 | `MIT` |  | shipped (Linux) |
| `glib-macros` | 0.18.5 | `MIT` |  | build |
| `glib-sys` | 0.18.1 | `MIT` |  | shipped (Linux) |
| `glob` | 0.3.4 | `MIT OR Apache-2.0` |  | shipped (all) |
| `gobject-sys` | 0.18.0 | `MIT` |  | shipped (Linux) |
| `gtk` | 0.18.2 | `MIT` |  | shipped (Linux) |
| `gtk-sys` | 0.18.2 | `MIT` |  | shipped (Linux) |
| `gtk3-macros` | 0.18.2 | `MIT` |  | build |
| `half` | 2.7.1 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `hashbrown` | 0.12.3 | `MIT OR Apache-2.0` |  | shipped (all) |
| `hashbrown` | 0.16.1 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `hashbrown` | 0.17.1 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `heck` | 0.4.1 | `MIT OR Apache-2.0` |  | build |
| `heck` | 0.5.0 | `MIT OR Apache-2.0` | build | shipped (all) |
| `hermit-abi` | 0.5.3 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `hex` | 0.4.3 | `MIT OR Apache-2.0` | shipped (Linux) | shipped (all) |
| `hkdf` | 0.13.0 | `MIT OR Apache-2.0` | shipped (Linux) | shipped (Linux) |
| `hmac` | 0.13.0 | `MIT OR Apache-2.0` | shipped (Linux) | shipped (Linux) |
| `html5ever` | 0.39.0 | `MIT OR Apache-2.0` |  | shipped (all) |
| `http` | 1.5.0 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `http-body` | 1.1.0 | `MIT` |  | not on desktop |
| `http-body-util` | 0.1.5 | `MIT` |  | not on desktop |
| `httparse` | 1.10.1 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `hybrid-array` | 0.4.15 | `MIT OR Apache-2.0` | shipped (Linux) | shipped (Linux) |
| `hyper` | 1.11.1 | `MIT` |  | not on desktop |
| `hyper-util` | 0.1.21 | `MIT` |  | not on desktop |
| `iana-time-zone` | 0.1.65 | `MIT OR Apache-2.0` |  | shipped (macOS, Linux) |
| `iana-time-zone-haiku` | 0.1.2 | `MIT OR Apache-2.0` |  | not on desktop |
| `ico` | 0.5.0 | `MIT` |  | build |
| `icu_collections` | 2.3.0 | `Unicode-3.0` |  | shipped (all) |
| `icu_locale_core` | 2.3.0 | `Unicode-3.0` |  | shipped (all) |
| `icu_normalizer` | 2.3.0 | `Unicode-3.0` |  | shipped (all) |
| `icu_normalizer_data` | 2.3.0 | `Unicode-3.0` |  | shipped (all) |
| `icu_properties` | 2.3.0 | `Unicode-3.0` |  | shipped (all) |
| `icu_properties_data` | 2.3.0 | `Unicode-3.0` |  | shipped (all) |
| `icu_provider` | 2.3.1 | `Unicode-3.0` |  | shipped (all) |
| `ident_case` | 1.0.1 | `MIT OR Apache-2.0` |  | build |
| `idna` | 1.1.0 | `MIT OR Apache-2.0` |  | shipped (all) |
| `idna_adapter` | 1.2.2 | `Apache-2.0 OR MIT` |  | shipped (all) |
| `if-addrs` | 0.15.0 | `MIT OR BSD-3-Clause` | shipped (all) | shipped (all) |
| `image` | 0.25.10 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `image-webp` | 0.2.4 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `indexmap` | 1.9.3 | `Apache-2.0 OR MIT` |  | shipped (all) |
| `indexmap` | 2.14.2 | `Apache-2.0 OR MIT` | shipped (all) | shipped (all) |
| `infer` | 0.22.0 | `MIT` |  | shipped (all) |
| `inout` | 0.2.2 | `MIT OR Apache-2.0` | shipped (Linux) | shipped (Linux) |
| `inventory` | 0.3.25 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `io-kit-sys` | 0.4.1 | `MIT OR Apache-2.0` | shipped (macOS) | shipped (macOS) |
| `ipnet` | 2.12.2 | `MIT OR Apache-2.0` |  | not on desktop |
| `is_terminal_polyfill` | 1.70.2 | `MIT OR Apache-2.0` | shipped (all) |  |
| `itertools` | 0.14.0 | `MIT OR Apache-2.0` | build | build |
| `itertools` | 0.15.0 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `itoa` | 1.0.18 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `javascriptcore-rs` | 1.1.2 | `MIT` |  | shipped (Linux) |
| `javascriptcore-rs-sys` | 1.1.1 | `MIT` |  | shipped (Linux) |
| `jiff` | 0.2.37 | `Unlicense OR MIT` |  | shipped (all) |
| `jiff-core` | 0.1.1 | `Unlicense OR MIT` |  | shipped (all) |
| `jiff-static` | 0.2.37 | `Unlicense OR MIT` |  | not on desktop |
| `jiff-tzdb` | 0.1.8 | `Unlicense OR MIT` |  | shipped (Windows) |
| `jiff-tzdb-platform` | 0.1.3 | `Unlicense OR MIT` |  | shipped (Windows) |
| `jni` | 0.21.1 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `jni-sys` | 0.3.1 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `jni-sys` | 0.4.1 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `jni-sys-macros` | 0.4.1 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `jobserver` | 0.1.35 | `MIT OR Apache-2.0` | build | build |
| `js-sys` | 0.3.106 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `json-patch` | 4.2.0 | `MIT OR Apache-2.0` |  | shipped (all) |
| `jsonptr` | 0.7.1 | `MIT OR Apache-2.0` |  | shipped (all) |
| `jsonschema` | 0.58.5 | `MIT` | tests |  |
| `jsonschema-regex` | 0.58.5 | `MIT` | tests |  |
| `jsonschema-value` | 0.58.5 | `MIT` | tests |  |
| `keyboard-types` | 0.8.3 | `MIT OR Apache-2.0` |  | shipped (all) |
| `keyring` | 4.2.0 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `keyring-core` | 1.0.0 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `lazy_static` | 1.5.1 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `libappindicator` | 0.9.0 | `Apache-2.0 OR MIT` |  | shipped (Linux) |
| `libappindicator-sys` | 0.9.0 | `Apache-2.0 OR MIT` |  | shipped (Linux) |
| `libc` | 0.2.190 | `MIT OR Apache-2.0` | shipped (macOS, Linux) | shipped (all) |
| `libdbus-sys` | 0.2.7 | `Apache-2.0 OR MIT` |  | shipped (Linux) |
| `libloading` | 0.7.4 | `ISC` |  | shipped (Linux) |
| `libloading` | 0.8.9 | `ISC` | shipped (all) | shipped (all) |
| `libm` | 0.2.16 | `MIT` | shipped (all) | shipped (all) |
| `libredox` | 0.1.25 | `MIT` |  | not on desktop |
| `linux-raw-sys` | 0.12.1 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | shipped (Linux) | shipped (Linux) |
| `litemap` | 0.8.3 | `Unicode-3.0` |  | shipped (all) |
| `lock_api` | 0.4.14 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `log` | 0.4.34 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `mach2` | 0.4.3 | `BSD-2-Clause OR MIT OR Apache-2.0` | shipped (macOS) | shipped (macOS) |
| `mach2` | 0.5.0 | `BSD-2-Clause OR MIT OR Apache-2.0` | shipped (macOS) | shipped (macOS) |
| `maplit` | 1.0.2 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `markup5ever` | 0.39.0 | `MIT OR Apache-2.0` |  | shipped (all) |
| `matrixmultiply` | 0.3.11 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `mdns-sd` | 0.21.4 | `Apache-2.0 OR MIT` | shipped (all) | shipped (all) |
| `memchr` | 2.8.3 | `Unlicense OR MIT` | shipped (all) | shipped (all) |
| `memmap2` | 0.9.11 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `memo-map` | 0.3.4 | `Apache-2.0` | build | build |
| `memoffset` | 0.9.1 | `MIT` | not on desktop | shipped (Linux) |
| `micromap` | 0.3.0 | `MIT` | tests |  |
| `mime` | 0.3.17 | `MIT OR Apache-2.0` |  | shipped (all) |
| `minijinja` | 2.24.0 | `Apache-2.0` | build | build |
| `miniz_oxide` | 0.8.9 | `MIT OR Zlib OR Apache-2.0` | shipped (all) | shipped (all) |
| `miniz_oxide` | 0.9.1 | `MIT OR Zlib OR Apache-2.0` | shipped (all) | shipped (all) |
| `mio` | 1.2.4 | `MIT` | shipped (all) | shipped (all) |
| `moxcms` | 0.8.1 | `BSD-3-Clause OR Apache-2.0` | shipped (all) | shipped (all) |
| `muda` | 0.20.0 | `Apache-2.0 OR MIT` |  | shipped (all) |
| `multiversion_no_op` | 1.0.0 | `Apache-2.0 OR MIT` | build | build |
| `nasm-rs` | 0.3.2 | `MIT OR Apache-2.0` | build | build |
| `ndarray` | 0.17.2 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `ndk` | 0.9.0 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `ndk-context` | 0.1.1 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `ndk-sys` | 0.6.0+11769913 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `new_debug_unreachable` | 1.0.6 | `MIT` |  | shipped (all) |
| `nix` | 0.26.4 | `MIT` | shipped (macOS, Linux) | shipped (macOS, Linux) |
| `nix` | 0.31.3 | `MIT` | shipped (macOS, Linux) | shipped (macOS) |
| `nom` | 8.0.0 | `MIT` | shipped (all) | shipped (all) |
| `nom-language` | 0.1.0 | `MIT` | shipped (all) | shipped (all) |
| `num` | 0.4.3 | `MIT OR Apache-2.0` | shipped (Linux) | shipped (Linux) |
| `num-bigint` | 0.4.8 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `num-cmp` | 0.1.0 | `MIT OR Apache-2.0` | tests |  |
| `num-complex` | 0.4.6 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `num-conv` | 0.2.2 | `MIT OR Apache-2.0` |  | shipped (all) |
| `num-derive` | 0.4.2 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `num-integer` | 0.1.47 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `num-iter` | 0.1.46 | `MIT OR Apache-2.0` | shipped (Linux) | shipped (Linux) |
| `num-rational` | 0.4.2 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `num-traits` | 0.2.19 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `num_enum` | 0.7.6 | `BSD-3-Clause OR MIT OR Apache-2.0` | not on desktop | not on desktop |
| `num_enum_derive` | 0.7.6 | `BSD-3-Clause OR MIT OR Apache-2.0` | not on desktop | not on desktop |
| `objc2` | 0.6.4 | `MIT` | shipped (macOS) | shipped (macOS) |
| `objc2-app-kit` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` |  | shipped (macOS) |
| `objc2-audio-toolbox` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | shipped (macOS) | shipped (macOS) |
| `objc2-avf-audio` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | not on desktop | not on desktop |
| `objc2-cloud-kit` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` |  | not on desktop |
| `objc2-core-audio` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | shipped (macOS) | shipped (macOS) |
| `objc2-core-audio-types` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | shipped (macOS) | shipped (macOS) |
| `objc2-core-data` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` |  | not on desktop |
| `objc2-core-foundation` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | shipped (macOS) | shipped (macOS) |
| `objc2-core-graphics` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` |  | shipped (macOS) |
| `objc2-core-image` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` |  | not on desktop |
| `objc2-core-location` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` |  | not on desktop |
| `objc2-core-text` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` |  | not on desktop |
| `objc2-encode` | 4.1.0 | `MIT` | shipped (macOS) | shipped (macOS) |
| `objc2-exception-helper` | 0.1.1 | `Zlib OR Apache-2.0 OR MIT` |  | shipped (macOS) |
| `objc2-foundation` | 0.3.2 | `MIT` | shipped (macOS) | shipped (macOS) |
| `objc2-io-surface` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` |  | shipped (macOS) |
| `objc2-quartz-core` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` |  | shipped (macOS) |
| `objc2-ui-kit` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` |  | not on desktop |
| `objc2-user-notifications` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` |  | not on desktop |
| `objc2-web-kit` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` |  | shipped (macOS) |
| `once_cell` | 1.21.4 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `once_cell_polyfill` | 1.70.2 | `MIT OR Apache-2.0` | shipped (Windows) |  |
| `openh264` | 0.9.8 | `BSD-2-Clause` | shipped (all) | shipped (all) |
| `openh264-sys2` | 0.9.8 | `BSD-2-Clause` | shipped (all) | shipped (all) |
| `option-ext` | 0.2.0 | `MPL-2.0` |  | shipped (all) |
| `ordered-stream` | 0.2.0 | `MIT OR Apache-2.0` | shipped (Linux) | shipped (Linux) |
| `outref` | 0.5.2 | `MIT` | tests |  |
| `oxideav-aac` | 0.1.7 | `MIT` | shipped (all) | shipped (all) |
| `oxideav-core` | 0.1.39 | `MIT` | shipped (all) | shipped (all) |
| `pango` | 0.18.3 | `MIT` |  | shipped (Linux) |
| `pango-sys` | 0.18.0 | `MIT` |  | shipped (Linux) |
| `parking` | 2.2.1 | `Apache-2.0 OR MIT` | shipped (Linux) | shipped (Linux) |
| `parking_lot` | 0.12.5 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `parking_lot_core` | 0.9.12 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `pastey` | 0.2.3 | `MIT OR Apache-2.0` | build | build |
| `percent-encoding` | 2.3.2 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `phf` | 0.13.1 | `MIT` |  | shipped (all) |
| `phf_codegen` | 0.13.1 | `MIT` |  | build |
| `phf_generator` | 0.13.1 | `MIT` |  | build |
| `phf_macros` | 0.13.1 | `MIT` |  | build |
| `phf_shared` | 0.13.1 | `MIT` |  | shipped (all) |
| `pin-project-lite` | 0.2.17 | `Apache-2.0 OR MIT` | shipped (Linux) | shipped (all) |
| `piper` | 0.2.5 | `MIT OR Apache-2.0` | shipped (Linux) | shipped (Linux) |
| `pkg-config` | 0.3.34 | `MIT OR Apache-2.0` | build | build |
| `plist` | 1.10.1 | `MIT` |  | shipped (all) |
| `png` | 0.17.16 | `MIT OR Apache-2.0` |  | build |
| `png` | 0.18.1 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `polling` | 3.11.0 | `Apache-2.0 OR MIT` | shipped (Linux) | shipped (Linux) |
| `portable-atomic` | 1.15.0 | `Apache-2.0 OR MIT` | not on desktop | not on desktop |
| `portable-atomic-util` | 0.2.8 | `Apache-2.0 OR MIT` | not on desktop | not on desktop |
| `potential_utf` | 0.1.6 | `Unicode-3.0` |  | shipped (all) |
| `powerfmt` | 0.2.0 | `MIT OR Apache-2.0` |  | shipped (all) |
| `ppv-lite86` | 0.2.21 | `MIT OR Apache-2.0` | tests |  |
| `precomputed-hash` | 0.1.1 | `MIT` |  | shipped (all) |
| `primal-check` | 0.3.4 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `proc-macro-crate` | 1.3.1 | `MIT OR Apache-2.0` |  | build |
| `proc-macro-crate` | 2.0.2 | `MIT OR Apache-2.0` |  | build |
| `proc-macro-crate` | 3.5.0 | `MIT OR Apache-2.0` | build | build |
| `proc-macro-error` | 1.0.4 | `MIT OR Apache-2.0` |  | build |
| `proc-macro-error-attr` | 1.0.4 | `MIT OR Apache-2.0` |  | build |
| `proc-macro2` | 1.0.107 | `MIT OR Apache-2.0` | shipped (Linux) | shipped (all) |
| `proptest` | 1.11.0 | `MIT OR Apache-2.0` | tests |  |
| `prost` | 0.14.4 | `Apache-2.0` | shipped (all) | shipped (all) |
| `prost-derive` | 0.14.4 | `Apache-2.0` | build | build |
| `pxfm` | 0.1.30 | `BSD-3-Clause OR Apache-2.0` | shipped (all) | shipped (all) |
| `quick-error` | 1.2.3 | `MIT OR Apache-2.0` | tests |  |
| `quick-error` | 2.0.1 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `quick-xml` | 0.42.0 | `MIT` |  | shipped (all) |
| `quote` | 1.0.47 | `MIT OR Apache-2.0` | shipped (Linux) | shipped (all) |
| `r-efi` | 5.3.0 | `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | not on desktop | not on desktop |
| `r-efi` | 6.0.0 | `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | not on desktop | not on desktop |
| `rand` | 0.10.3 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `rand` | 0.9.5 | `MIT OR Apache-2.0` | tests |  |
| `rand_chacha` | 0.9.0 | `MIT OR Apache-2.0` | tests |  |
| `rand_core` | 0.10.1 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `rand_core` | 0.9.5 | `MIT OR Apache-2.0` | tests |  |
| `rand_distr` | 0.6.0 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `rand_xorshift` | 0.4.0 | `MIT OR Apache-2.0` | tests |  |
| `raw-window-handle` | 0.6.2 | `MIT OR Apache-2.0 OR Zlib` |  | shipped (all) |
| `rawpointer` | 0.2.1 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `rayon` | 1.12.0 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `rayon-core` | 1.13.0 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `realfft` | 3.5.0 | `MIT` | shipped (all) | shipped (all) |
| `redox_syscall` | 0.5.18 | `MIT` | not on desktop | not on desktop |
| `redox_users` | 0.5.3 | `MIT` |  | not on desktop |
| `ref-cast` | 1.0.27 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `ref-cast-impl` | 1.0.27 | `MIT OR Apache-2.0` | build | build |
| `referencing` | 0.58.5 | `MIT` | tests |  |
| `regex` | 1.13.1 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `regex-automata` | 0.4.18 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `regex-syntax` | 0.8.11 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `reqwest` | 0.13.5 | `MIT OR Apache-2.0` |  | not on desktop |
| `rfd` | 0.16.0 | `MIT` |  | shipped (all) |
| `ring` | 0.17.14 | `Apache-2.0 AND ISC` | shipped (all) | shipped (all) |
| `rodio` | 0.22.2 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `roxmltree` | 0.21.1 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `rustc-hash` | 2.1.3 | `Apache-2.0 OR MIT` |  | shipped (all) |
| `rustc_version` | 0.4.1 | `MIT OR Apache-2.0` |  | build |
| `rustfft` | 6.4.1 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `rustix` | 1.1.5 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | shipped (macOS, Linux) | shipped (macOS, Linux) |
| `rustls` | 0.23.45 | `Apache-2.0 OR ISC OR MIT` | shipped (all) | shipped (all) |
| `rustls-pki-types` | 1.15.1 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `rustls-webpki` | 0.103.16 | `ISC` | shipped (all) | shipped (all) |
| `rustversion` | 1.0.23 | `MIT OR Apache-2.0` | build | build |
| `rusty-fork` | 0.3.1 | `MIT OR Apache-2.0` | tests |  |
| `safe_arch` | 1.2.0 | `Zlib OR Apache-2.0 OR MIT` | shipped (all) | shipped (all) |
| `same-file` | 1.0.6 | `Unlicense OR MIT` | shipped (all) | shipped (all) |
| `schemars` | 0.8.22 | `MIT` |  | shipped (all) |
| `schemars` | 0.9.0 | `MIT` |  | shipped (all) |
| `schemars` | 1.2.2 | `MIT` | shipped (all) | shipped (all) |
| `schemars_derive` | 0.8.22 | `MIT` |  | build |
| `schemars_derive` | 1.2.2 | `MIT` | build | build |
| `scopeguard` | 1.2.0 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `secret-service` | 5.2.0 | `MIT OR Apache-2.0` | shipped (Linux) | shipped (Linux) |
| `security-framework` | 3.7.0 | `MIT OR Apache-2.0` | shipped (macOS) | shipped (macOS) |
| `security-framework-sys` | 2.17.0 | `MIT OR Apache-2.0` | shipped (macOS) | shipped (macOS) |
| `selectors` | 0.38.0 | `MPL-2.0` |  | shipped (all) |
| `semver` | 1.0.28 | `MIT OR Apache-2.0` |  | shipped (all) |
| `serde` | 1.0.229 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `serde-untagged` | 0.1.9 | `MIT OR Apache-2.0` |  | shipped (all) |
| `serde_core` | 1.0.229 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `serde_derive` | 1.0.229 | `MIT OR Apache-2.0` | build | build |
| `serde_derive_internals` | 0.29.1 | `MIT OR Apache-2.0` |  | build |
| `serde_derive_internals` | 0.30.0 | `MIT OR Apache-2.0` | build | build |
| `serde_json` | 1.0.151 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `serde_repr` | 0.1.21 | `MIT OR Apache-2.0` | build | build |
| `serde_spanned` | 0.6.9 | `MIT OR Apache-2.0` |  | build |
| `serde_spanned` | 1.1.1 | `MIT OR Apache-2.0` |  | shipped (all) |
| `serde_with` | 3.24.0 | `MIT OR Apache-2.0` |  | shipped (all) |
| `serde_with_macros` | 3.24.0 | `MIT OR Apache-2.0` |  | build |
| `serialize-to-javascript` | 0.1.2 | `MIT OR Apache-2.0` |  | shipped (all) |
| `serialize-to-javascript-impl` | 0.1.2 | `MIT OR Apache-2.0` |  | build |
| `serialport` | 4.10.1 | `MPL-2.0` | shipped (all) | shipped (all) |
| `servo_arc` | 0.4.3 | `MIT OR Apache-2.0` |  | shipped (all) |
| `sha2` | 0.10.9 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `sha2` | 0.11.0 | `MIT OR Apache-2.0` | shipped (Linux) | shipped (Linux) |
| `shlex` | 2.0.1 | `MIT OR Apache-2.0` | build | build |
| `signal-hook-registry` | 1.4.8 | `MIT OR Apache-2.0` | shipped (Linux) | shipped (Linux) |
| `simd-adler32` | 0.3.10 | `MIT` | shipped (all) | shipped (all) |
| `simdutf8` | 0.1.5 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `siphasher` | 1.0.4 | `MIT OR Apache-2.0` |  | shipped (all) |
| `slab` | 0.4.12 | `MIT` | shipped (Linux) | shipped (Linux) |
| `smallvec` | 1.16.2 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `socket-pktinfo` | 0.4.1 | `MIT` | shipped (all) | shipped (all) |
| `socket2` | 0.6.5 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `softbuffer` | 0.4.8 | `MIT OR Apache-2.0` |  | shipped (Windows) |
| `soup3` | 0.5.0 | `MIT` |  | shipped (Linux) |
| `soup3-sys` | 0.5.0 | `MIT` |  | shipped (Linux) |
| `spin` | 0.9.9 | `MIT` | shipped (all) | shipped (all) |
| `stable_deref_trait` | 1.2.1 | `MIT OR Apache-2.0` |  | shipped (all) |
| `strength_reduce` | 0.2.4 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `string-interner` | 0.20.0 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `string_cache` | 0.9.0 | `MIT OR Apache-2.0` |  | shipped (all) |
| `string_cache_codegen` | 0.6.1 | `MIT OR Apache-2.0` |  | build |
| `strsim` | 0.11.1 | `MIT` | shipped (all) | build |
| `strum` | 0.28.0 | `MIT` | tests |  |
| `strum_macros` | 0.28.0 | `MIT` | tests |  |
| `subtle` | 2.6.1 | `BSD-3-Clause` | shipped (all) | shipped (all) |
| `swift-rs` | 1.0.8 | `MIT OR Apache-2.0` |  | shipped (macOS) |
| `symphonia` | 0.5.5 | `MPL-2.0` | shipped (all) | shipped (all) |
| `symphonia-bundle-flac` | 0.5.5 | `MPL-2.0` | shipped (all) | shipped (all) |
| `symphonia-bundle-mp3` | 0.5.5 | `MPL-2.0` | shipped (all) | shipped (all) |
| `symphonia-codec-aac` | 0.5.5 | `MPL-2.0` | shipped (all) | shipped (all) |
| `symphonia-codec-pcm` | 0.5.5 | `MPL-2.0` | shipped (all) | shipped (all) |
| `symphonia-codec-vorbis` | 0.5.5 | `MPL-2.0` | shipped (all) | shipped (all) |
| `symphonia-core` | 0.5.5 | `MPL-2.0` | shipped (all) | shipped (all) |
| `symphonia-format-isomp4` | 0.5.5 | `MPL-2.0` | shipped (all) | shipped (all) |
| `symphonia-format-ogg` | 0.5.5 | `MPL-2.0` | shipped (all) | shipped (all) |
| `symphonia-format-riff` | 0.5.5 | `MPL-2.0` | shipped (all) | shipped (all) |
| `symphonia-metadata` | 0.5.5 | `MPL-2.0` | shipped (all) | shipped (all) |
| `symphonia-utils-xiph` | 0.5.5 | `MPL-2.0` | shipped (all) | shipped (all) |
| `syn` | 1.0.109 | `MIT OR Apache-2.0` |  | build |
| `syn` | 2.0.119 | `MIT OR Apache-2.0` | build | build |
| `syn` | 3.0.6 | `MIT OR Apache-2.0` | shipped (Linux) | shipped (Linux) |
| `sync_wrapper` | 1.0.2 | `Apache-2.0` |  | not on desktop |
| `synstructure` | 0.14.0 | `MIT` |  | build |
| `system-deps` | 6.2.2 | `MIT OR Apache-2.0` |  | build |
| `tao` | 0.37.1 | `Apache-2.0` |  | shipped (all) |
| `tao-macros` | 0.1.4 | `MIT OR Apache-2.0` |  | not on desktop |
| `tar` | 0.4.46 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `target-lexicon` | 0.12.16 | `Apache-2.0 WITH LLVM-exception` |  | build |
| `tauri` | 2.12.1 | `Apache-2.0 OR MIT` |  | shipped (all) |
| `tauri-build` | 2.7.1 | `Apache-2.0 OR MIT` |  | build |
| `tauri-codegen` | 2.7.1 | `Apache-2.0 OR MIT` |  | build |
| `tauri-macros` | 2.7.1 | `Apache-2.0 OR MIT` |  | build |
| `tauri-plugin` | 2.7.1 | `Apache-2.0 OR MIT` |  | build |
| `tauri-plugin-dialog` | 2.8.1 | `Apache-2.0 OR MIT` |  | shipped (all) |
| `tauri-plugin-fs` | 2.6.0 | `Apache-2.0 OR MIT` |  | shipped (all) |
| `tauri-runtime` | 2.12.1 | `Apache-2.0 OR MIT` |  | shipped (all) |
| `tauri-runtime-wry` | 2.12.1 | `Apache-2.0 OR MIT` |  | shipped (all) |
| `tauri-utils` | 2.10.1 | `Apache-2.0 OR MIT` |  | shipped (all) |
| `tauri-winres` | 0.3.6 | `MIT` |  | build |
| `tempfile` | 3.27.0 | `MIT OR Apache-2.0` | tests | tests |
| `tendril` | 0.5.1 | `MIT OR Apache-2.0` |  | shipped (all) |
| `thiserror` | 1.0.69 | `MIT OR Apache-2.0` | not on desktop | shipped (Linux) |
| `thiserror` | 2.0.21 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `thiserror-impl` | 1.0.69 | `MIT OR Apache-2.0` | not on desktop | build |
| `thiserror-impl` | 2.0.21 | `MIT OR Apache-2.0` | build | build |
| `time` | 0.3.55 | `MIT OR Apache-2.0` |  | shipped (all) |
| `time-core` | 0.1.9 | `MIT OR Apache-2.0` |  | shipped (all) |
| `time-macros` | 0.2.32 | `MIT OR Apache-2.0` |  | build |
| `tinystr` | 0.8.4 | `Unicode-3.0` |  | shipped (all) |
| `tinyvec` | 1.13.3 | `Zlib OR Apache-2.0 OR MIT` | shipped (all) | shipped (all) |
| `tokio` | 1.53.2 | `MIT` | not on desktop | shipped (all) |
| `tokio-util` | 0.7.19 | `MIT` |  | not on desktop |
| `toml` | 0.8.2 | `MIT OR Apache-2.0` |  | build |
| `toml` | 1.1.6+spec-1.1.0 | `MIT OR Apache-2.0` |  | shipped (all) |
| `toml_datetime` | 0.6.3 | `MIT OR Apache-2.0` |  | build |
| `toml_datetime` | 1.1.1+spec-1.1.0 | `MIT OR Apache-2.0` | build | shipped (all) |
| `toml_edit` | 0.19.15 | `MIT OR Apache-2.0` |  | build |
| `toml_edit` | 0.20.2 | `MIT OR Apache-2.0` |  | build |
| `toml_edit` | 0.25.15+spec-1.1.0 | `MIT OR Apache-2.0` | build | build |
| `toml_parser` | 1.1.3+spec-1.1.0 | `MIT OR Apache-2.0` | build | shipped (all) |
| `toml_writer` | 1.1.2+spec-1.1.0 | `MIT OR Apache-2.0` |  | shipped (all) |
| `tower` | 0.5.3 | `MIT` |  | not on desktop |
| `tower-http` | 0.6.11 | `MIT` |  | not on desktop |
| `tower-layer` | 0.3.3 | `MIT` |  | not on desktop |
| `tower-service` | 0.3.3 | `MIT` |  | not on desktop |
| `tracing` | 0.1.44 | `MIT` | shipped (Linux) | shipped (Windows, Linux) |
| `tracing-attributes` | 0.1.31 | `MIT` | build | build |
| `tracing-core` | 0.1.36 | `MIT` | shipped (Linux) | shipped (Windows, Linux) |
| `tract-core` | 0.23.8 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `tract-data` | 0.23.8 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `tract-extra` | 0.23.8 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `tract-hir` | 0.23.8 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `tract-linalg` | 0.23.8 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `tract-nnef` | 0.23.8 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `tract-onnx` | 0.23.8 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `tract-onnx-opl` | 0.23.8 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `tract-transformers` | 0.23.8 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `transpose` | 0.2.3 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `tray-icon` | 0.25.1 | `MIT OR Apache-2.0` |  | shipped (all) |
| `triple_buffer` | 9.0.0 | `MPL-2.0` | shipped (all) | shipped (all) |
| `try-lock` | 0.2.5 | `MIT` |  | not on desktop |
| `typed-path` | 0.13.0 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `typeid` | 1.0.3 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `typenum` | 1.20.1 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `uds_windows` | 1.2.1 | `MIT` | not on desktop | not on desktop |
| `unarray` | 0.1.4 | `MIT OR Apache-2.0` | tests |  |
| `unescaper` | 0.1.10 | `MIT OR GPL-3.0-only` | shipped (Linux) | shipped (Linux) |
| `unicode-general-category` | 1.1.0 | `Apache-2.0` | tests |  |
| `unicode-ident` | 1.0.26 | `(MIT OR Apache-2.0) AND Unicode-3.0` | shipped (Linux) | shipped (all) |
| `unicode-normalization` | 0.1.25 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `unicode-segmentation` | 1.13.3 | `MIT OR Apache-2.0` |  | shipped (Windows) |
| `untrusted` | 0.9.0 | `ISC` | shipped (all) | shipped (all) |
| `ureq` | 3.4.2 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `ureq-proto` | 0.6.4 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `url` | 2.5.8 | `MIT OR Apache-2.0` |  | shipped (all) |
| `urlpattern` | 0.6.0 | `MIT` |  | shipped (all) |
| `utf8-zero` | 0.8.1 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `utf8_iter` | 1.0.4 | `Apache-2.0 OR MIT` |  | shipped (all) |
| `utf8parse` | 0.2.2 | `Apache-2.0 OR MIT` | shipped (all) |  |
| `uuid` | 1.27.0 | `Apache-2.0 OR MIT` | shipped (all) | shipped (all) |
| `uuid-simd` | 0.8.0 | `MIT` | tests |  |
| `version-compare` | 0.2.1 | `MIT` |  | build |
| `version_check` | 0.9.5 | `MIT OR Apache-2.0` | build | build |
| `vsimd` | 0.8.0 | `MIT` | tests |  |
| `vswhom` | 0.1.0 | `MIT` |  | build |
| `vswhom-sys` | 0.1.3 | `MIT` |  | build |
| `wait-timeout` | 0.2.1 | `MIT OR Apache-2.0` | tests |  |
| `walkdir` | 2.5.0 | `Unlicense OR MIT` | shipped (all) | shipped (all) |
| `want` | 0.3.1 | `MIT` |  | not on desktop |
| `wasi` | 0.11.1+wasi-snapshot-preview1 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | not on desktop | not on desktop |
| `wasip2` | 1.0.4+wasi-0.2.12 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | not on desktop | not on desktop |
| `wasm-bindgen` | 0.2.129 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `wasm-bindgen-futures` | 0.4.79 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `wasm-bindgen-macro` | 0.2.129 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `wasm-bindgen-macro-support` | 0.2.129 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `wasm-bindgen-shared` | 0.2.129 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `wasm-streams` | 0.5.0 | `MIT OR Apache-2.0` |  | not on desktop |
| `web-sys` | 0.3.106 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `web-time` | 1.1.0 | `MIT OR Apache-2.0` |  | shipped (all) |
| `web_atoms` | 0.2.6 | `MIT OR Apache-2.0` |  | shipped (all) |
| `webkit2gtk` | 2.0.2 | `MIT` |  | shipped (Linux) |
| `webkit2gtk-sys` | 2.0.2 | `MIT` |  | shipped (Linux) |
| `webpki-roots` | 1.0.11 | `CDLA-Permissive-2.0` | shipped (all) | shipped (all) |
| `webview2-com` | 0.39.1 | `MIT` |  | shipped (Windows) |
| `webview2-com-macros` | 0.8.1 | `MIT` |  | build |
| `webview2-com-sys` | 0.39.1 | `MIT` |  | shipped (Windows) |
| `weezl` | 0.1.12 | `MIT OR Apache-2.0` | shipped (all) | shipped (all) |
| `wide` | 1.7.1 | `Zlib OR Apache-2.0 OR MIT` | shipped (all) | shipped (all) |
| `winapi` | 0.3.9 | `MIT OR Apache-2.0` |  | not on desktop |
| `winapi-i686-pc-windows-gnu` | 0.4.0 | `MIT OR Apache-2.0` |  | not on desktop |
| `winapi-util` | 0.1.11 | `Unlicense OR MIT` | shipped (Windows) | shipped (Windows) |
| `winapi-x86_64-pc-windows-gnu` | 0.4.0 | `MIT OR Apache-2.0` |  | not on desktop |
| `window-vibrancy` | 0.8.1 | `Apache-2.0 OR MIT` |  | shipped (macOS, Windows) |
| `windows` | 0.62.2 | `MIT OR Apache-2.0` | shipped (Windows) | shipped (Windows) |
| `windows-collections` | 0.3.2 | `MIT OR Apache-2.0` | shipped (Windows) | shipped (Windows) |
| `windows-core` | 0.62.2 | `MIT OR Apache-2.0` | shipped (Windows) | shipped (Windows) |
| `windows-future` | 0.3.2 | `MIT OR Apache-2.0` | shipped (Windows) | shipped (Windows) |
| `windows-implement` | 0.60.2 | `MIT OR Apache-2.0` | build | build |
| `windows-interface` | 0.59.3 | `MIT OR Apache-2.0` | build | build |
| `windows-link` | 0.2.1 | `MIT OR Apache-2.0` | shipped (Windows) | shipped (Windows) |
| `windows-native-keyring-store` | 1.1.0 | `MIT OR Apache-2.0` | shipped (Windows) | shipped (Windows) |
| `windows-numerics` | 0.3.1 | `MIT OR Apache-2.0` | shipped (Windows) | shipped (Windows) |
| `windows-result` | 0.4.1 | `MIT OR Apache-2.0` | shipped (Windows) | shipped (Windows) |
| `windows-strings` | 0.5.1 | `MIT OR Apache-2.0` | shipped (Windows) | shipped (Windows) |
| `windows-sys` | 0.45.0 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `windows-sys` | 0.52.0 | `MIT OR Apache-2.0` | shipped (Windows) | shipped (Windows) |
| `windows-sys` | 0.59.0 | `MIT OR Apache-2.0` |  | build |
| `windows-sys` | 0.60.2 | `MIT OR Apache-2.0` |  | shipped (Windows) |
| `windows-sys` | 0.61.2 | `MIT OR Apache-2.0` | shipped (Windows) | shipped (Windows) |
| `windows-targets` | 0.42.2 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `windows-targets` | 0.52.6 | `MIT OR Apache-2.0` | shipped (Windows) | shipped (Windows) |
| `windows-targets` | 0.53.5 | `MIT OR Apache-2.0` |  | shipped (Windows) |
| `windows-threading` | 0.2.1 | `MIT OR Apache-2.0` | shipped (Windows) | shipped (Windows) |
| `windows-version` | 0.1.7 | `MIT OR Apache-2.0` |  | shipped (Windows) |
| `windows_aarch64_gnullvm` | 0.42.2 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `windows_aarch64_gnullvm` | 0.52.6 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `windows_aarch64_gnullvm` | 0.53.1 | `MIT OR Apache-2.0` |  | not on desktop |
| `windows_aarch64_msvc` | 0.42.2 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `windows_aarch64_msvc` | 0.52.6 | `MIT OR Apache-2.0` | shipped (Windows) | shipped (Windows) |
| `windows_aarch64_msvc` | 0.53.1 | `MIT OR Apache-2.0` |  | shipped (Windows) |
| `windows_i686_gnu` | 0.42.2 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `windows_i686_gnu` | 0.52.6 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `windows_i686_gnu` | 0.53.1 | `MIT OR Apache-2.0` |  | not on desktop |
| `windows_i686_gnullvm` | 0.52.6 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `windows_i686_gnullvm` | 0.53.1 | `MIT OR Apache-2.0` |  | not on desktop |
| `windows_i686_msvc` | 0.42.2 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `windows_i686_msvc` | 0.52.6 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `windows_i686_msvc` | 0.53.1 | `MIT OR Apache-2.0` |  | not on desktop |
| `windows_x86_64_gnu` | 0.42.2 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `windows_x86_64_gnu` | 0.52.6 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `windows_x86_64_gnu` | 0.53.1 | `MIT OR Apache-2.0` |  | not on desktop |
| `windows_x86_64_gnullvm` | 0.42.2 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `windows_x86_64_gnullvm` | 0.52.6 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `windows_x86_64_gnullvm` | 0.53.1 | `MIT OR Apache-2.0` |  | not on desktop |
| `windows_x86_64_msvc` | 0.42.2 | `MIT OR Apache-2.0` | not on desktop | not on desktop |
| `windows_x86_64_msvc` | 0.52.6 | `MIT OR Apache-2.0` | shipped (Windows) | shipped (Windows) |
| `windows_x86_64_msvc` | 0.53.1 | `MIT OR Apache-2.0` |  | shipped (Windows) |
| `winnow` | 0.5.40 | `MIT` |  | build |
| `winnow` | 1.0.4 | `MIT` | shipped (Linux) | shipped (all) |
| `winreg` | 0.55.0 | `MIT` |  | build |
| `wit-bindgen` | 0.57.1 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | not on desktop | not on desktop |
| `writeable` | 0.6.4 | `Unicode-3.0` |  | shipped (all) |
| `wry` | 0.57.0 | `Apache-2.0 OR MIT` |  | shipped (all) |
| `x11` | 2.21.0 | `MIT` |  | shipped (Linux) |
| `x11-dl` | 2.21.0 | `MIT` |  | shipped (Linux) |
| `xattr` | 1.6.1 | `MIT OR Apache-2.0` | shipped (macOS, Linux) | shipped (macOS, Linux) |
| `yoke` | 0.8.3 | `Unicode-3.0` |  | shipped (all) |
| `yoke-derive` | 0.8.4 | `Unicode-3.0` |  | build |
| `zbus` | 5.19.0 | `MIT` | shipped (Linux) | shipped (Linux) |
| `zbus-secret-service-keyring-store` | 1.0.1 | `MIT OR Apache-2.0` | shipped (Linux) | shipped (Linux) |
| `zbus_macros` | 5.19.0 | `MIT` | build | build |
| `zbus_names` | 4.3.4 | `MIT` | shipped (Linux) | shipped (Linux) |
| `zcheapstr` | 1.1.0 | `MIT` | shipped (Linux) | shipped (Linux) |
| `zerocopy` | 0.8.59 | `BSD-2-Clause OR Apache-2.0 OR MIT` | shipped (all) |  |
| `zerocopy` | 0.8.62 | `BSD-2-Clause OR Apache-2.0 OR MIT` |  | shipped (all) |
| `zerocopy-derive` | 0.8.59 | `BSD-2-Clause OR Apache-2.0 OR MIT` | build |  |
| `zerocopy-derive` | 0.8.62 | `BSD-2-Clause OR Apache-2.0 OR MIT` |  | build |
| `zerofrom` | 0.1.8 | `Unicode-3.0` |  | shipped (all) |
| `zerofrom-derive` | 0.1.8 | `Unicode-3.0` |  | build |
| `zeroize` | 1.9.0 | `Apache-2.0 OR MIT` | shipped (all) | shipped (all) |
| `zerotrie` | 0.2.5 | `Unicode-3.0` |  | shipped (all) |
| `zerovec` | 0.11.8 | `Unicode-3.0` |  | shipped (all) |
| `zerovec-derive` | 0.11.6 | `Unicode-3.0` |  | build |
| `zip` | 9.1.3 | `MIT` | shipped (all) | shipped (all) |
| `zlib-rs` | 0.6.8 | `Zlib` | shipped (all) | shipped (all) |
| `zmij` | 1.0.23 | `MIT` | shipped (all) | shipped (all) |
| `zstd` | 0.14.3 | `BSD-3-Clause` | shipped (all) | shipped (all) |
| `zstd-safe` | 8.2.0 | `BSD-3-Clause` | shipped (all) | shipped (all) |
| `zstd-sys` | 2.1.2+zstd.1.5.7 | `BSD-3-Clause` | shipped (all) | shipped (all) |
| `zune-core` | 0.5.3 | `MIT OR Apache-2.0 OR Zlib` | shipped (all) | shipped (all) |
| `zune-jpeg` | 0.5.15 | `MIT OR Apache-2.0 OR Zlib` | shipped (all) | shipped (all) |
| `zvariant` | 5.15.0 | `MIT` | shipped (Linux) | shipped (Linux) |
| `zvariant_derive` | 5.15.0 | `MIT` | build | build |
| `zvariant_utils` | 4.2.0 | `MIT` | shipped (Linux) | shipped (Linux) |

## Every production npm package

| Package | Version | Licence | Listed in `app/package.json` |
| --- | --- | --- | --- |
| `@radix-ui/primitive` | 1.1.7 | `MIT` |  |
| `@radix-ui/react-compose-refs` | 1.1.5 | `MIT` |  |
| `@radix-ui/react-context` | 1.2.2 | `MIT` |  |
| `@radix-ui/react-dialog` | 1.1.23 | `MIT` |  |
| `@radix-ui/react-dismissable-layer` | 1.1.19 | `MIT` |  |
| `@radix-ui/react-focus-guards` | 1.1.6 | `MIT` |  |
| `@radix-ui/react-focus-scope` | 1.1.16 | `MIT` |  |
| `@radix-ui/react-id` | 1.1.4 | `MIT` |  |
| `@radix-ui/react-portal` | 1.1.17 | `MIT` |  |
| `@radix-ui/react-presence` | 1.1.10 | `MIT` |  |
| `@radix-ui/react-primitive` | 2.1.10 | `MIT` |  |
| `@radix-ui/react-slot` | 1.3.3 | `MIT` |  |
| `@radix-ui/react-use-callback-ref` | 1.1.4 | `MIT` |  |
| `@radix-ui/react-use-controllable-state` | 1.2.6 | `MIT` |  |
| `@radix-ui/react-use-effect-event` | 0.0.5 | `MIT` |  |
| `@radix-ui/react-use-layout-effect` | 1.1.4 | `MIT` |  |
| `@tauri-apps/api` | 2.12.1 | `Apache-2.0 OR MIT` | yes |
| `@tauri-apps/plugin-dialog` | 2.8.1 | `MIT OR Apache-2.0` | yes |
| `aria-hidden` | 1.2.6 | `MIT` |  |
| `cmdk` | 1.1.1 | `MIT` | yes |
| `detect-node-es` | 1.1.0 | `MIT` |  |
| `get-nonce` | 1.0.1 | `MIT` |  |
| `lucide-react` | 1.51.0 | `ISC` | yes |
| `react` | 19.3.0 | `MIT` | yes |
| `react-dom` | 19.3.0 | `MIT` | yes |
| `react-remove-scroll` | 2.7.2 | `MIT` |  |
| `react-remove-scroll-bar` | 2.3.8 | `MIT` |  |
| `react-style-singleton` | 2.2.3 | `MIT` |  |
| `scheduler` | 0.28.0 | `MIT` |  |
| `three` | 0.186.1 | `MIT` | yes |
| `tslib` | 2.8.1 | `0BSD` |  |
| `use-callback-ref` | 1.3.3 | `MIT` |  |
| `use-sidecar` | 1.1.3 | `MIT` |  |
| `zustand` | 5.0.15 | `MIT` | yes |

<!-- END GENERATED -->
