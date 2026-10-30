# Releasing PixelFlow

How a version of PixelFlow becomes installers for macOS and Windows: what to check first, how to
cut the release, how signing is set up, and what the people who install it need to know.

> **PixelFlow is released under the GNU General Public License, version 3**, free of charge.
> About 10,000 lines of it are derived from xLights' source, which is under that licence, and
> are credited in [`NOTICE`](../NOTICE). A release must come with its source (a GitHub release
> does: the tagged source is attached beside the installers) and with `LICENSE`, `NOTICE`, and
> the third-party notices inside the app. What else the licences ask, and the decisions still
> open (the AAC encoder above all), are in the
> [distribution checklist](licensing/distribution-checklist.md). The release workflow only ever
> makes a **draft**; read the checklist before publishing one.

- [What a release holds](#what-a-release-holds)
- [Before you release](#before-you-release)
- [Cutting a release](#cutting-a-release)
- [Dry runs](#dry-runs)
- [Signing](#signing)
- [Installing an unsigned build](#installing-an-unsigned-build)
- [First launch: what each system asks](#first-launch-what-each-system-asks)
- [Upgrading and going back](#upgrading-and-going-back)
- [What has and hasn't been tried](#what-has-and-hasnt-been-tried)

## What a release holds

The [release workflow](../.github/workflows/release.yml) builds these from a clean checkout,
with the dependencies exactly as the lockfiles name them and the Rust version
`rust-toolchain.toml` names.

| File | What it is |
|---|---|
| `PixelFlow_X.Y.Z_universal.dmg` | The macOS app, for Apple silicon and Intel Macs in one. |
| `PixelFlow_X.Y.Z_x64-setup.exe` | The Windows installer most people should use. Installs for the current user, without an administrator. If the computer has no WebView2 (Windows 10 before it was added), the installer downloads it. |
| `PixelFlow_X.Y.Z_x64_en-US.msi` | The same app as a Windows Installer package, for computers that are managed centrally. Installs for all users and needs an administrator. |
| `THIRD-PARTY-NOTICES.txt` | The licences of everything PixelFlow is built from. The same file is inside the app. |
| `SHA256SUMS` | A SHA-256 checksum for each file above. |

The version is three numbers (`0.2.0`), and the tag is that with a `v` in front (`v0.2.0`).
Nothing follows the third number: the `.msi` cannot carry a label such as `-beta`. For a build
to hand to testers before a release, use a [dry run](#dry-runs).

## Before you release

Tick every row, or write down why not.

- [ ] **The licence travels with the release.** `LICENSE` and `NOTICE` are at the repo root, the
      built app holds `LICENSE.txt`, `NOTICE.txt`, and `THIRD-PARTY-NOTICES.txt`, and its About
      opens them. The open items in the
      [distribution checklist](licensing/distribution-checklist.md) have been read, and the one
      about the AAC encoder has been decided.
- [ ] **No H.264 encoder inside.** H.264 is decided: the app has no H.264 encoder of its own.
      It downloads Cisco's OpenH264 binary from Cisco when the user agrees, checks it, and loads
      it at run time ([distribution checklist](licensing/distribution-checklist.md), item 6,
      "H.264: decided"). `node scripts/encoder-check.mjs` (or `make encoder-check`) passes on
      the release commit: it fails if the app's builds would compile OpenH264 from source.
- [ ] **Real hardware:** someone ran the
      [hardware acceptance checklist](hardware-acceptance.md) against the build being released
      (a dry run's installers, not a development build) and kept the filled-in copy.
- [ ] **Security:** the [threat model](security/threat-model.md) has been read for this
      release: what is listed under "Not verified", and which recommendations are still open.
      New code that reads files or talks to devices since the review has been looked at.
- [ ] **CI is green on `main`** for Linux, macOS, and Windows, and the `audit` and
      `secret-scan` workflows are green.
- [ ] **The notices are up to date:** `node scripts/licenses.mjs --check` passes (run
      `make licenses` after any lockfile change).
- [ ] **The version is set** in every file: `make version` prints it.
- [ ] **Signing** is set up for both systems (the dry run's summary says "signed and notarised"
      and "signed"), or this release is only for testers who know it is unsigned.
- [ ] **A dry run from the release commit was installed** on a Mac and on a Windows computer
      that have never had PixelFlow, opened, and taken through
      [what each system asks](#first-launch-what-each-system-asks).
- [ ] **Upgrading was tried:** the new version installed over the last one, and a show and a
      sequence saved by the last one opened.

## Cutting a release

1. **Set the version** on a branch from `develop`:

   ```sh
   make bump VERSION=0.2.0 DRY=1   # says what would change
   make bump VERSION=0.2.0         # changes it
   make version                    # prints 0.2.0 if every file agrees
   ```

   This writes the number into `Cargo.toml` (which every engine crate uses),
   `app/src-tauri/Cargo.toml`, `app/src-tauri/tauri.conf.json`, `app/package.json`, and the
   entries for PixelFlow's own crates in both `Cargo.lock` files. Commit it
   (`chore: version 0.2.0`) and merge it into `develop` like any change.
2. **Merge `develop` into `main`** with a pull request (a merge commit, not a squash).
3. **Do a [dry run](#dry-runs) on `main`**, with `v0.2.0` in the tag box. Read the run's
   summary, download the installers, and install them.
4. **Tag `main`** and push the tag. This is the moment the release starts:

   ```sh
   git switch main && git pull
   git tag -a v0.2.0 -m "PixelFlow 0.2.0"
   git push origin v0.2.0
   ```

5. **The workflow runs.** It
   - checks that every file says `0.2.0` and that the tag is `v0.2.0`, and stops with a
     sentence naming the file if not;
   - checks that the tagged commit is on `main`, and stops if not;
   - builds the macOS app and the two Windows installers, signs them if the
     [signing secrets](#signing) exist, and checks each one (both processors in the macOS app,
     the right version, the notices inside, a valid signature);
   - writes `SHA256SUMS`;
   - makes a **draft** release named "PixelFlow 0.2.0" with the files, the table of
     checksums, whether each system's build is signed, and GitHub's list of what was merged
     since the last release.
6. **Read the draft** on the repository's Releases page. Rewrite the list of changes for people
   who use the app. Check the files against `SHA256SUMS`. If the summary says a build is
   unsigned, stop here unless this is for testers.
7. **Publish it** with the draft's Publish button. Nothing is public before this.
8. **To call it off instead,** delete the draft, then the tag
   (`git push origin :refs/tags/v0.2.0`). A tag that already has a release, draft or not, is
   refused by the workflow, so delete the draft before pushing the same tag again.

The workflow never publishes, never pushes, and never changes a tag. Its `draft` job is the only
one that may write to the repository, and only releases.

## Dry runs

A dry run builds everything a release would and keeps it as the run's artifacts, without a tag
or a release. Use it to try the installers, to check signing after changing a secret, or to
give a tester a build.

- On GitHub: **Actions**, **release**, **Run workflow**, pick the branch. The tag box is
  optional: with `v0.2.0` in it, the run also checks that the files say `0.2.0`.
- Or: `gh workflow run release.yml --ref main -f tag=v0.2.0`
- The run's summary page lists each file with its size and checksum, and says for each system
  whether the build is signed.
- The artifact `pixelflow-X.Y.Z` holds everything a release would (kept 30 days).
  `installers-macos` and `installers-windows` hold each system's own part (14 days).

GitHub only offers **Run workflow** for a workflow that is on the default branch (`develop`).

## Signing

Without signing, the installers work but each system warns people off them (see
[Installing an unsigned build](#installing-an-unsigned-build)). Signing is switched on by adding
repository secrets (**Settings**, **Secrets and variables**, **Actions**); nothing else changes.
The workflow never prints a secret, and none is ever committed.

Repository secrets can be read by any workflow run from a branch of this repository, so by
anyone who may push one. To narrow that, GitHub can keep secrets in an *environment* that needs
a named person's approval before a job may use them; the build job would then name that
environment. That is not set up.

### macOS

What it takes: membership of the Apple Developer Program (US$99 a year), and from it a
**Developer ID Application** certificate. Apple also has to *notarise* each build (scan it and
record that it did), or macOS still warns.

| Secret | What it is |
|---|---|
| `APPLE_CERTIFICATE` | The Developer ID Application certificate with its private key, as a `.p12` file, base64-encoded. |
| `APPLE_CERTIFICATE_PASSWORD` | The password chosen when the `.p12` was exported. |
| `APPLE_SIGNING_IDENTITY` | The certificate's name, exactly: `Developer ID Application: Your Name (TEAMID)`. |
| `APPLE_ID` | The Apple account's email address, for notarising. |
| `APPLE_PASSWORD` | An **app-specific password** for that account (made at account.apple.com, Sign-In and Security), not the account's own password. |
| `APPLE_TEAM_ID` | The ten-character team ID (Membership details, on developer.apple.com). |

To make the first three:

1. In Xcode (Settings, Accounts, Manage Certificates) or on developer.apple.com, create a
   **Developer ID Application** certificate. It lands in the login keychain.
2. In Keychain Access, under **My Certificates**, select it together with its private key,
   choose **Export**, save a `.p12`, and give it a password.
3. Encode it and store it without it ever being shown:

   ```sh
   base64 -i DeveloperID.p12 | gh secret set APPLE_CERTIFICATE
   gh secret set APPLE_CERTIFICATE_PASSWORD      # asks for the value
   security find-identity -v -p codesigning      # shows the identity's exact name
   gh secret set APPLE_SIGNING_IDENTITY
   ```

4. Delete the `.p12` file. Keep the certificate in the keychain of one computer you trust.

With the first three secrets the app is signed. With all six it is also notarised. With none,
the app is signed "ad hoc" (no identity at all), which is the least a Mac with Apple silicon
needs in order to run it.

### Windows

Windows trusts an installer signed with a code-signing certificate from a certificate
authority, and its SmartScreen filter also wants the signer to have built up a reputation.
The choices, roughly as they stood in 2026 (check prices and rules before buying):

| Option | Cost | Trust | Notes |
|---|---|---|---|
| Not signed | Free | SmartScreen says "Windows protected your PC" on every computer. Some virus scanners flag unsigned installers. | What the workflow builds with no secrets. |
| A standard (OV) certificate | About US$200 to US$500 a year | The publisher's name shows. SmartScreen still warns until enough people have installed it. | Since 2023 the key must live on a hardware token or in the seller's cloud service, so most cannot be exported as a file for CI. |
| An extended validation (EV) certificate | About US$300 to US$700 a year, and a registered business | As OV. It used to skip SmartScreen's warning at once; Microsoft has said it no longer does. | The same hardware or cloud-key rule. |
| Azure Trusted Signing (Microsoft's cloud signing) | About US$10 a month | Microsoft checks who you are; reputation still builds over time. | Only for some countries and kinds of account; check whether you qualify. Signs from CI without a key file. |
| SignPath Foundation | Free for open-source projects | A certificate in SignPath's name for your project. | Needs an open-source licence, so it waits on the licence decision. |

What the workflow does today: if `WINDOWS_CERTIFICATE` and `WINDOWS_CERTIFICATE_PASSWORD`
exist, it signs both installers and the app inside them, with a timestamp, and fails if a
signature doesn't verify.

| Secret or variable | What it is |
|---|---|
| `WINDOWS_CERTIFICATE` (secret) | A code-signing certificate with its private key, as a `.pfx` file, base64-encoded: `base64 -i certificate.pfx \| gh secret set WINDOWS_CERTIFICATE` |
| `WINDOWS_CERTIFICATE_PASSWORD` (secret) | The `.pfx` file's password. |
| `WINDOWS_TIMESTAMP_URL` (variable, optional) | The timestamp server to use. `http://timestamp.digicert.com` when not set. |

That fits a certificate you can hold as a file. A cloud service (Azure Trusted Signing, a
seller's cloud key, SignPath) signs through its own tool instead, which Tauri runs when
`bundle.windows.signCommand` is set in `tauri.conf.json`. That is not set up: whoever buys the
certificate adds the service's sign command and its secrets to the workflow's Windows build.

## Installing an unsigned build

For testers. Check the download first: `shasum -a 256 -c SHA256SUMS` on a Mac, or
`Get-FileHash PixelFlow_X.Y.Z_x64-setup.exe` in PowerShell on Windows, and compare with
`SHA256SUMS`.

### macOS

1. Open the `.dmg` and drag PixelFlow to Applications.
2. Open it. macOS says it could not verify the app, and offers no way to open it.
3. **macOS 15 and later:** open System Settings, **Privacy & Security**, scroll down to the
   line about PixelFlow, choose **Open Anyway**, and confirm with your password.
   **macOS 14 and earlier:** hold Control and click the app, choose **Open**, then **Open**
   again.
4. If macOS says the app "is damaged and can't be opened", the download's quarantine mark is
   in the way. Remove it in Terminal, then open the app:
   `xattr -dr com.apple.quarantine /Applications/PixelFlow.app`

This is needed once for each build. Every unsigned build counts as a different app to macOS, so
the Keychain asks again before a new build may read a saved AI key: choose **Always Allow**.

### Windows

1. Your browser may say the file isn't commonly downloaded. Choose to keep it.
2. Run `PixelFlow_X.Y.Z_x64-setup.exe`. SmartScreen shows "Windows protected your PC": choose
   **More info**, then **Run anyway**.
3. If the computer has no WebView2 (Windows 10 before it was added; Windows 11 has it), the
   installer downloads it, so it needs the internet once.

A virus scanner may hold an unsigned installer back. That is the scanner's caution about any
unsigned installer, not something it found.

## First launch: what each system asks

What PixelFlow does that an operating system asks about. This comes from what the code does;
see [what has and hasn't been tried](#what-has-and-hasnt-been-tried).

### macOS

- **Local Network.** The first time PixelFlow looks for controllers (**Find my controllers**,
  or **Scan network** on the Controllers screen) or
  talks to one (checking it, test output, playing, Send to FPP), macOS asks whether PixelFlow
  may find and connect to devices on the local network. The question carries PixelFlow's own
  sentence: "PixelFlow finds the light controllers on your network and sends them light data."
  **Allow it.** Refused, nothing is found and no light data arrives, with no error from macOS.
  To change the answer: System Settings, Privacy & Security, **Local Network**.
- **Files in Documents, Desktop, and Downloads.** A file chosen in a dialog needs no
  permission. When PixelFlow opens one on its own (a recent show, or the music a show names),
  macOS may ask once for each of those folders.
- **Keychain.** Saving an AI key asks nothing. Reading it back asks only when the app's
  signature changed, which is every build for an unsigned one.
- **Sound.** Playing music needs no permission. PixelFlow never uses the microphone or the
  camera.

### Windows

- **Firewall.** The first time you look for controllers (**Find my controllers**, or **Scan
  network** on the Controllers screen), PixelFlow listens for answers
  from FPP devices (UDP port 32320) and for the network's own device announcements (mDNS, UDP
  5353), and Windows Defender Firewall asks whether to allow it. **Allow it on private
  networks.** Refused, PixelFlow still finds controllers by trying addresses near your own, and
  still sends light data (both are outgoing), but controllers that only announce themselves are
  missed. To change the answer: Windows Security, Firewall & network protection, **Allow an app
  through firewall**.
- **Sound.** No question. Music plays on the default output device; with none, PixelFlow plays
  the lights alone and says so.
- **AI keys** go into Credential Manager without a question.
- **Files on a network share** that a show names are not opened until you choose one there
  yourself ([troubleshooting](troubleshooting.md#a-file-is-on-a-network-share)).

## Upgrading and going back

Installing a new version over an old one replaces the app and nothing else. Removing the app
leaves the data too (the Windows uninstaller offers to delete it, and doesn't unless asked).

### Where PixelFlow keeps things

Shows and sequences are the files you saved, wherever you saved them; a show's pictures are in
the `images` folder beside it. Everything else:

| What | macOS | Windows |
|---|---|---|
| **Data:** autosaved versions of each show (`history/<show>-…/`), unsaved sequences (`history/sequences/`), copies of controller setups taken before changing them (`device-setups/`), the downloaded speech model (`models/`) | `~/Library/Application Support/com.bradh11.pixelflow/` | `%APPDATA%\com.bradh11.pixelflow\` |
| **Settings:** recent shows (`recent.json`, `recent-thumbs/`), the folder each dialog was last in (`pickers.json`), vendor mappings (`vendor-mappings.json`), where the preview window was (`preview-window.json`) | the same folder | the same folder |
| **Caches:** what was worked out about each song (`audio/`), lyrics found (`lyrics/`) | `~/Library/Caches/com.bradh11.pixelflow/` | `%LOCALAPPDATA%\com.bradh11.pixelflow\` |
| **The window's own settings** (view choices, the setup checklist, the assistant's panel) | `~/Library/WebKit/com.bradh11.pixelflow/` | `%LOCALAPPDATA%\com.bradh11.pixelflow\EBWebView\` |
| **AI keys** | Keychain: service `com.bradh11.pixelflow`, accounts `anthropic` and `openai` | Credential Manager: `anthropic.com.bradh11.pixelflow` and `openai.com.bradh11.pixelflow` |

On Linux the three folders are `~/.local/share/`, `~/.config/`, and `~/.cache/`, each with
`com.bradh11.pixelflow` inside. The caches can be deleted at any time; PixelFlow works things
out again.

### File versions only go forward

A show file and a sequence file each carry a `schemaVersion`. A newer PixelFlow reads every
older version and brings it up to date in memory; the file on disk changes only when you save.
Once saved, it is in the new version, and an older PixelFlow refuses it: "the show file uses
schema version 13, but this version of PixelFlow only supports up to 12; update PixelFlow to
open it". There is no way to save in an older version.

So before an upgrade that raises a schema version (the release notes should say so):

- copy the show folder, or
- know that the autosaved versions in the data folder are as they were saved: each
  `history/<show>-…/` folder holds earlier copies of that show, in the version of the app that
  wrote them.

### Going back to an older version

1. Install the older installer over the newer one (both Windows installers allow that; on a
   Mac, replace the app in Applications).
2. Shows and sequences saved by the newer version won't open if it raised their schema
   version. Use the copies from before the upgrade.
3. Settings and caches from the newer version are read where they still make sense and
   otherwise ignored. If the older version misbehaves, quit it and move the settings files
   named above out of the way.

## What has and hasn't been tried

Tried in October 2026, by running the workflow on its own branch without a tag (version 0.1.0,
no signing secrets):

- **The version check.** With no tag, and with `v0.1.0` in the tag box, it went on to build.
  With `v9.9.9` it stopped before building: "The tag is v9.9.9, but the files say 0.1.0 (which
  would be the tag v0.1.0). Set the version first: node scripts/version.mjs set 9.9.9".
- **The macOS build**, in 16 to 22 minutes: `PixelFlow_0.1.0_universal.dmg`, 44 MB. The app
  inside holds both processors (`x86_64 arm64`), says version 0.1.0, carries the notices file
  byte for byte and the Local Network sentence, and is signed ad hoc with a signature that
  verifies. Notarising was skipped, as it should be with no secrets.
- **The Windows build**, in about 20 minutes: `PixelFlow_0.1.0_x64-setup.exe`, 15 MB, and
  `PixelFlow_0.1.0_x64_en-US.msi`, 22 MB. The setup program holds the notices file. Both are
  unsigned.
- **The summary and the artifacts:** `SHA256SUMS`, the notices, and a summary that said
  "macOS: unsigned. Windows: unsigned. THIS IS NOT A FULLY SIGNED BUILD."
- **CI on Windows:** the engine's and the desktop shell's lints and tests run there on every
  pull request, beside Linux and macOS.

Not tried, because each needs something only the owner has or may do:

- **Signing and notarising** on macOS, and signing on Windows: there are no certificates. The
  steps follow Tauri's documented way of signing, and the workflow fails if a build that was
  to be signed has no valid signature, but none of it has run.
- **The tag path:** the check that a tag is on `main`, and making the draft release. No tag was
  pushed and no release, draft or otherwise, was made.
- **The installers themselves.** None was installed or opened, so nothing under
  [Installing an unsigned build](#installing-an-unsigned-build) or
  [First launch](#first-launch-what-each-system-asks) was seen happen: those sections describe
  what each system does with an app that does what PixelFlow's code does. The oldest macOS and
  Windows versions the app runs on are not known either (the app declares macOS 10.13).
- **Byte-for-byte sameness.** The build starts from locked dependencies and a pinned
  toolchain, but whether two runs of one commit give identical installers was not checked
  (timestamps and signatures usually prevent it). `SHA256SUMS` identifies one run's files.
- **Cisco's OpenH264 in a built app.** The download, its check, loading it, and exporting a video
  with it were tried on a Mac with Apple silicon (`cargo run -p pf-video --example
  cisco_openh264 -- FOLDER`), not in a built app. A binary signed ad hoc with the hardened
  runtime was refused Cisco's library until it carried the entitlement in
  `app/src-tauri/Entitlements.plist`, then loaded it. Not tried: a signed and notarised app,
  an Intel Mac, Windows, and Linux. A dry run's app should export a video on each system it
  ships for, the first time asking to download the encoder.
- **The release workflow and the encoder check.** `.github/workflows/release.yml` and `ci.yml`
  don't run `node scripts/encoder-check.mjs` yet (the workflows are the owner's to change);
  run it by hand before a release, as the checklist says.
