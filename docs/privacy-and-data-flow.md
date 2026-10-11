# Your data: what stays on your computer and what PixelFlow sends

PixelFlow is a desktop app. Your shows, sequences, music, and photos are files on your computer,
and PixelFlow has no account, no cloud storage, and no servers of its own.

It does talk to two kinds of thing: the light controllers on your own network, and, only for the
features that need them, a few services on the internet. This page says exactly what goes where,
and when.

## The short version

- **No sign-up, no telemetry.** PixelFlow doesn't collect analytics, usage statistics, or crash
  reports, and it doesn't check in with anyone when it starts.
- **Your files stay where you put them.** Nothing is uploaded unless you use a feature below
  that says so.
- **It scans your network only when you ask.** "Find my controllers" looks at your local
  network; nothing is scanned in the background.
- **The assistant is off until you add your own key.** When you use it, parts of your show go to
  the AI company you chose (Anthropic or OpenAI), under your own account with them.
- **Your music is sent to OpenAI only if you say yes** to that, separately, when finding lyrics.

## What stays on your computer

| What | Where |
|---|---|
| Shows and sequences | The files you save, wherever you save them. PixelFlow also keeps autosaved copies and a history of earlier versions in its own data folder. |
| Music, photos, pictures, 3D house model | Your own files. A picture you add to a Picture effect is copied into an `images` folder next to the show. |
| Your AI key | Your operating system's secure store: the Keychain on macOS, Credential Manager on Windows, the Secret Service on Linux. It is never written into a show, a sequence, a log, or any other file, and once you've typed it in, the app's window can't read it back. If your computer has no secure store, you can choose to keep the key only until you quit. |
| What PixelFlow worked out about a song (beats, sections, loudness) | A cache folder on your computer, so it isn't worked out twice. |
| Lyrics it found for a song | The same cache, and the sequence you put them in. |
| Controllers it has found, your recent shows, your settings | PixelFlow's own settings on your computer. |
| A copy of a controller's setup, taken before PixelFlow changes it | PixelFlow's data folder, so you can put it back. For a WLED this copy is its whole configuration, which can include the names (not the passwords) of the Wi-Fi networks it knows. You can delete it with "Forget this copy". |

All sound analysis (beats, sections, levels) happens on your computer. So does working out when
each word is sung, if you turn on the on-device option (see "Lyrics").

## Your controllers and your network

Controllers (FPP, Falcon, WLED) are devices on your own network. PixelFlow talks to them
directly; that traffic doesn't leave your network unless you give a controller an address that
is somewhere else.

### Finding your controllers

When you choose **Find my controllers**, and only then, PixelFlow:

1. sends a short "who's there?" message that FPP-compatible devices answer. It carries the
   word "PixelFlow" and nothing about you or your computer;
2. asks your network, by the standard local-discovery method (mDNS), for WLED and FPP devices;
3. tries the web page of each address near your computer's own (up to 253 per network, on home
   and office address ranges only) to see whether a controller answers;
4. asks each FPP it finds which controllers it sends to, and checks those too.

It listens for answers for about three seconds. It only reads: nothing on any device is
changed. Devices that aren't controllers see one or two ordinary web requests.

If you're on an employer's network or a VPN, be aware that step 3 looks like a quick scan of the
nearby addresses.

### Reading and showing what a controller is doing

While you have the **Play** screen or a controller's page open, PixelFlow asks the FPPs it
already knows about what they're playing, every second or few. Opening the **Test** screen
checks whether each controller in your show answers. These are small requests on your own
network.

PixelFlow never reads the parts of a controller that hold passwords (an FPP's network settings,
for example), and never asks you for a controller's password.

### Changing a controller

PixelFlow changes something on a controller only when you click the button that says so, after
showing you what will change:

- **Play** and **Stop** on an FPP;
- **Send to FPP**: uploads the sequence file and its music to the FPP you chose;
- **Send setup**: writes pixel-output settings. Before the first change, PixelFlow keeps a copy
  of the controller's setup so you can put it back. It never writes Wi-Fi or network settings.

### Lighting the lights

When you start a test pattern or play a sequence, PixelFlow sends light data (sACN or DDP) to
the address of each controller in your show, many times a second, until you stop.

### A note on shows from other people

A show file holds its controllers' addresses. If you open a show someone else made, the
addresses in it are theirs. PixelFlow will check and, when you press Play or Send, send to those
addresses. Look at the controller list before you send anything from a show you didn't make.

A show or a sequence also names its files: music, a photo, pictures. If one of those names is a
place on a network share (`\\server\share\…`), PixelFlow does not look there on the file's say-so.
On Windows, even looking at such a place signs in to that server, and a file from someone else
could name a server of theirs. The file is listed with the missing ones, with a sentence saying
it is on a network share. Once you choose a file in that folder yourself (Locate…, or any Open
dialog), PixelFlow reads the rest of that folder for as long as the app stays open. If you keep
your own music on a network share, giving the share a drive letter in Windows avoids the
question altogether.

## The assistant (AI)

The assistant does nothing, and sends nothing, until you add your own key from Anthropic or
OpenAI in **Settings → AI**. Requests go straight from your computer to that company, over an
encrypted connection, using your key. PixelFlow has no server in between and never sees them.

When you send the assistant a message, the AI company receives:

- **your message**, and the rest of that conversation;
- **a little about what you're looking at**: which screen, the names of the props you've
  selected, the open sequence's name and length;
- **whatever the assistant looks up to answer you.** It can read the show and sequence you have
  open, so, depending on what you ask, this can include:
  - the names, shapes, positions, and wiring of your props and groups;
  - your controllers' names and their network addresses;
  - your playlist, including where those files are on your computer (which usually shows your
    account name);
  - the open sequence: its effects and timing marks, **including lyrics** if it has them;
  - the names (not the contents) of pictures in your show's `images` folder;
  - PixelFlow's own analysis of the song (where the beats and sections are).

It does **not** receive your key (except as the sign-in for the request itself), the audio of
your music, the contents of your photos or pictures, your controllers' passwords, or anything
from outside the open show.

Opening the AI settings also asks the company for its list of models, which sends nothing but
your key.

**The assistant can't change anything by itself.** It prepares a proposal; you see what it
would change, and nothing happens unless you press **Apply**. It has no way to read other
files, browse the web, or talk to your controllers.

What the AI company does with what it receives is governed by your agreement with them, not by
PixelFlow. PixelFlow asks OpenAI not to store the conversation. If you switch from one company
to the other in the middle of a chat, the conversation so far is sent to the new one.

## Lyrics

**Find lyrics** looks up a song's words and works out when each is sung. It runs only when you
click it, and it needs the assistant to be set up.

1. **Looking the song up.** PixelFlow sends the song's artist, title, album, and length (from the
   file's tags, or the file's name if it has none) to [LRCLIB](https://lrclib.net), a free,
   public lyrics library. It says it is PixelFlow and which version. It doesn't send the music.
2. **Listening to the song (optional).** If your key is from OpenAI, PixelFlow can send the song
   to OpenAI's speech recognition to hear the words and their timing. **It asks you first**:
   you can continue, use the published lyrics only, or cancel. (If you tick "Don't ask again",
   it stops asking for later songs.) If you continue, the audio is sent: the file as it is when
   it's under 25 MB, otherwise a smaller mono copy in parts, along with the file's name, the
   language, and a hint made of the title, artist, and first line.
3. **Timing the words on your computer (optional).** With on-device alignment turned on, a
   speech model on your own computer lines the words up with the music. Nothing is sent for
   this step. The model is a one-time download (next section).

What comes back is kept in PixelFlow's cache, so the same song isn't looked up or sent twice,
and is added to your sequence as timing marks you can undo.

## Downloads PixelFlow makes

| What | From | When | What is sent |
|---|---|---|---|
| The speech model for on-device word timing (about 190 MB) | Hugging Face | Once, when you turn the feature on and confirm the download | Only the request for the file. PixelFlow checks the file is exactly the one it expects before using it. |
| The list of FPP software releases, to tell you whether your FPP is up to date | GitHub | When an FPP you've found answers; at most every six hours | Only the request for that public list. |

As with any internet request, the service sees your computer's public internet address.

PixelFlow doesn't update itself and doesn't check for new versions of itself.

## If you want to keep everything on your own network

Don't add an AI key. Without one, the assistant and Find lyrics are off, and the only requests
that leave your network are the FPP release check (when you have an FPP) and the model download
(only if you ask for it).

## Questions or concerns

Security problems: see [SECURITY.md](../SECURITY.md). The technical detail behind this page,
with file and line references, is in [security/threat-model.md](security/threat-model.md).
