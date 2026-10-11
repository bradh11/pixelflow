# Your first show

This guide takes you from nothing to lights playing music. It uses the names of buttons and
screens exactly as they appear in PixelFlow. If something doesn't look the way this guide says,
see [Troubleshooting](troubleshooting.md).

## Words you will meet

- **Prop:** one thing you light, such as an arch, a tree, or a window outline. PixelFlow draws it
  as a shape on the **Layout** screen.
- **Pixel:** one light on a string. A prop is made of pixels.
- **Controller:** the box that powers the strings and tells each pixel what color to be. It can
  be an FPP (a Raspberry Pi or BeagleBone running Falcon Player), a Falcon board, or a WLED
  controller.
- **Port:** one socket on the controller where a string plugs in. Ports are numbered on the
  controller. PixelFlow uses the same numbers.
- **DDP and sACN:** two ways of sending light data over your network. Use **DDP** if you aren't
  sure. It needs no extra setup. **sACN** (also called E1.31) splits the data into
  **universes**, which are numbered blocks of 512 channels. You only need to think about
  universes if your controller is set up to listen for them.
- **FPP:** Falcon Player. It plays a show on its own, without your computer.
- **FSEQ (`.fseq`):** a finished, frame-by-frame recording of a sequence. FPP plays these.
- **Null pixels:** pixels on a string that stay dark. You use them to skip a stretch of wire
  that has no lights, such as a long lead to reach a prop.
- **Color order:** the order a string expects red, green, and blue in, such as RGB or GRB.
  If it is wrong, the colors come out swapped.

## Two ways to start

- **Route A: "I have an xLights show."** PixelFlow reads your xLights show folder and brings
  your props, groups, and controllers across. Go to [Route A](#route-a-i-have-an-xlights-show).
- **Route B: "Starting fresh."** You build the layout, add your controller, and wire it up
  yourself. Go to [Route B](#route-b-starting-fresh).

Both routes end at the same place. Finish with [Play to the lights](#b9-play-to-the-lights) and
[Send to FPP](#b10-send-to-fpp-for-unattended-playback).

## What each screen is for

The sidebar lists the screens in the order you will use them.

| Screen | What it is for |
|---|---|
| **Layout** | Draw your props over a photo of your house. Move, resize, and group them. Set a prop's color order. |
| **Controllers** | Find the controllers on your network and add them to your show. Open an FPP to see what it is playing. |
| **Wiring** | Say which prop plugs into which port, and in what order along the string. Add a controller by hand. |
| **Test** | Send a simple test pattern to real lights to check wiring and colors. |
| **Sequence** | Build a light show timed to a song: rows, effects, beats, lyrics. Send it to an FPP or export it. |
| **Play** | Show time. Plays a sequence full screen, with the playlist, the music, and what is on your FPP. |
| **History** | Automatic backups of your show. Put the show back as it was at an earlier time. |
| **Settings** (the gear at the foot of the sidebar) | Theme, the assistant's AI key, looping, Preview sync, and Layout options. |

At the foot of the sidebar, **Set up your show** is a checklist: **Draw your props**, **Find your
controllers**, **Wire your props**, **Test your lights**, **Make a sequence**, and **Add it to the
playlist**. Each step is a button that takes you to the right screen, and it ticks itself off as
you go. Put it away with its close button. To bring it back, press ⌘K (Ctrl+K on Windows and
Linux) and choose **Show the setup checklist**.

## Route A: I have an xLights show

### A1. Import the show folder

1. On the welcome screen, choose **Import from xLights…**. If a show is already open, use ⌘K and
   choose **Import from xLights…**.
2. Pick your xLights show folder. It is the folder that holds `xlights_rgbeffects.xml`.
3. Wait for the **Imported** window.

**You should see:** a window titled "Imported" followed by your show's name. It lists how many
props, pixels, controllers, wired props, and groups came across.

**If it doesn't:** see [A sequence import shows many "approximate"
notes](troubleshooting.md#a-sequence-import-shows-many-approximate-notes).

### A2. Read the import notes

Under the counts, the window says either "Everything came in exactly as it is in xLights." or
"These weren't imported exactly:" followed by a list of notes. Read each note once. Most say
that a shape or a wiring detail was approximated, and why.

*Approximate* means PixelFlow drew something close to what xLights has, not an exact copy. A
note names what was changed so you can check that prop in the layout. For how close PixelFlow
gets, see [Measuring xLights import fidelity](fidelity.md).

Then choose **Save show…** to keep the show. Choosing **Later** leaves it unsaved.

### A3. Check your props, groups, and controllers

1. Open **Layout**. Check that each prop is where it should be.
2. Open **Controllers**. Check that each controller you expect is listed in your show.
3. Open **Wiring**. Check that each controller has its ports and that each port holds the props
   that plug into it. A red box headed "N things to fix" lists anything wrong, with a suggested
   fix beside each item.

**You should see:** every prop wired. The **Set up your show** checklist shows **Wire your
props** ticked when none is left unwired.

**If it doesn't:** see [Props light in the wrong place or
reversed](troubleshooting.md#props-light-in-the-wrong-place-or-reversed).

### A4. Test before you go further

Do [B5. Test your lights](#b5-test-your-lights) now. Checking the real lights first saves a lot
of guessing later.

### A5. Import a sequence

1. Open **Sequence**.
2. Choose **Import from xLights…** in the toolbar. Pick an xLights sequence (`.xsq`) or a
   vendor's package (`.zip`).
   To import from a folder a package was unzipped to, press ⌘K and choose **Import xLights
   sequence from a folder…**.
3. Read the report that opens.

**You should see:** "Imported" followed by the sequence name. The counts show rows, effects,
timing tracks, and marks. A second line shows how many effects came in as *exact*,
*approximated*, or *placeholder*. A placeholder is a stand-in, used where PixelFlow has no
matching effect yet. The sequence opens on the **Sequence** screen as an unsaved sequence. Choose
**Save** to keep it.

**If it doesn't:** see [A sequence import shows many "approximate"
notes](troubleshooting.md#a-sequence-import-shows-many-approximate-notes).

### A6. Map a vendor's sequence onto your props

A sequence you bought or downloaded was made for someone else's props. PixelFlow doesn't know
which of your props matches which of theirs. It asks you.

1. After you choose the file, a window titled "Map *song name* onto your props" opens. The left
   side lists the vendor's models. The right side, headed "Goes to, in your show", shows your
   props they will go to.
2. Choose **Auto-map**. PixelFlow matches by name, alias, kind of prop, and size. Choose
   **Clear** to start over.
3. Look for rows marked **Not mapped** in amber. These are models that carry many effects, so
   leaving them out loses a lot. Choose **Map** on the row, then pick your props from the list.
   Type in **Find a prop or group** to narrow the list. A **suggested** tag marks PixelFlow's
   guess.
4. Use **Find one of their models (busiest first)** to search the vendor's list.
5. If you have already saved a mapping, **Load .xmap…** brings it back. **Save .xmap…** keeps
   yours for next time.
6. The window shows how much is done as "N of M mapped". When you're happy, choose **Import N
   effects**.

The song's music file is copied into your show's music folder. If the show isn't saved yet, the
window asks you to **Choose folder…** first.

Go to [B6. Make a sequence with music](#b6-make-a-sequence-with-music) for what to do next, or
straight to [B9. Play to the lights](#b9-play-to-the-lights).

## Route B: Starting fresh

### B1. Start a show

1. On the welcome screen, choose **New show**.
2. Save it early with ⌘S. A saved show also gives you History backups and lets
   PixelFlow keep music files next to it.

**You should see:** an empty **Layout** screen.

### B2. Add a controller

You can scan for it or add it by address.

**By scanning:**

1. Open **Controllers**.
2. Choose **Scan network**. It takes a few seconds.
3. Choose **Open** on your controller's row.
   - For a Falcon or WLED controller, a window shows its ports and strings. Choose **Add to
     show**. (A Falcon is read-only: PixelFlow reads its setup but can't send one, so **Send
     setup** is off for it.)
   - For an FPP, a page opens. Its **Outputs → your show** section lists the controllers the FPP
     sends to. Choose **Set up my show from this FPP**, review what it will add, and choose the
     button that adds the controllers.

**By address:**

1. On **Controllers**, type the controller's address in the box that says "Controller IP
   address, e.g. 192.168.1.50" and choose **Check address**.
2. Or, on **Wiring**, choose **Add controller** and fill in the form: **Name**, **Controller
   type**, **IP address**, **Protocol** (**DDP** or **sACN (E1.31)**), and **Ports**. Choose
   **Add**. Picking a known **Controller type**, such as **Falcon F16V5**, fills in the port
   count and a pixel limit for each port.

**You should see:** the controller in a list with its name, model, and address. Controllers in
your show are marked **In show**.

**If it doesn't:** see [The controller is not found or "not answering"](troubleshooting.md#the-controller-is-not-found-or-not-answering).

### B3. Add props on the Layout

1. Open **Layout**.
2. Choose **Add prop** (top right) and pick a shape from the menu, such as **Arch**, **Line /
   string**, **Matrix**, or **Mega tree**. It lands in the middle of the canvas. Or pick a tool on
   the toolbar and drag on the canvas to draw the prop where it belongs.
3. Select a prop to see its **Properties** panel. Set its pixel count, name, and position there.
4. **Photo behind the props:** choose **Photo** on the toolbar, then **Choose photo…**. Pick a
   picture of your house. Use **Photo strength** to fade it, **Move or resize photo** to line it
   up, and **Replace…** or **Remove photo** to change it.
5. Press V to switch between the 2D and 3D views. Press ⌘Z to undo any mistake.

**You should see:** each prop as a row of dots on the canvas, over your photo.

For more tips, choose **Tips** on the toolbar.

**If it doesn't:** see [Props light in the wrong place or
reversed](troubleshooting.md#props-light-in-the-wrong-place-or-reversed).

### B4. Wire props to ports

1. Open **Wiring**. Your props are listed on the left under **Props**. Each controller is a card
   with its ports.
2. Drag a prop onto a port. Or choose **+ Add…** on the port and pick the prop. Or choose **Wire**
   on the port, then click the props on the house picture in the order the wire runs, and
   choose **Done**.
3. Order matters. Props on a port are listed in wiring order, from the controller outward.
4. If a string plugs into the far end of a prop, open the prop's settings (its **Settings** button on the port's table)
   and tick **Starts at the other end**.
5. If a string has unlit stretches before the prop, set **Empty pixels before it** in the same
   settings. These are *null pixels*.

**You should see:** each port shows how many pixels it uses out of its limit. A port that is
over its limit shows a red message with the number of pixels over.

**If it doesn't:** see [Only some pixels light, or they stop partway](troubleshooting.md#only-some-pixels-light-or-they-stop-partway).

### B5. Test your lights

1. Open **Test**.
2. Under **Target**, choose **Whole show**, or a single prop or port. For a first test, pick one
   port.
3. Under **Pattern**, start with **Chase**. Choose **Start**.
4. Look at the real lights. A block of lit pixels should run along the string from the first pixel
   to the last.
5. Try **Red / green / blue / white**. Each color shows for one second. If red shows as another
   color, the color order is wrong. The test screen and **Wiring** both point to where to fix it:
   **Layout**, select the prop, then **More: color order and scale**.
6. Try **Pixel walk** to light one pixel at a time, and **Identify (blink)** to find which prop
   or port a target is.
7. Choose **Stop**. Output stops with a blackout.

**You should see:** under **Sends to**, each controller with "Web page answering" beside it. That
checks only the controller's web page, not that light data is arriving. Below that, a
row for each controller with its **State**, **Packets**, and **Errors**.

**If it doesn't:** see [Nothing lights](troubleshooting.md#nothing-lights). If colors are wrong,
see [Wrong colors](troubleshooting.md#wrong-colors-color-order).

### B6. Make a sequence with music

1. Open **Sequence** and choose **New sequence**.
2. In the window, choose **Choose music…** and pick your song. Give it a **Name**.
3. Under **Rows**, keep "A row for every prop and group", or choose **Start empty** to add rows
   yourself. Choose **Create**.

**You should see:** a timeline with your song's waveform and a row for each prop or group.

Choose **Save** often. The **Sequence** screen offers back any unsaved sequence from last time.

Beside the sequence's name, **Sequence settings** (the small sliders button) holds how the rows
combine where they light the same pixels (**Cover**, **Blend with the rows above**, or
**Replace**), plus the sequence's music and frame time.

### B7. Detect beats

1. Choose **Detect beats** in the toolbar. If a banner says "Find the beats, bars, and sections
   in this song?", you can use its **Detect beats** button.
2. Wait for the progress bar to finish.

**You should see:** new timing tracks for beats, bars, sections, accents, moments, and drums.
Effects can snap to these marks. Turn snapping on and off with **Snap**.

You can also choose **Find lyrics** to get the words as timing tracks. It uses the assistant's
settings, so see [Lyrics are wrong or in the wrong
language](troubleshooting.md#lyrics-are-wrong-or-in-the-wrong-language) and
[privacy and data flow](privacy-and-data-flow.md#lyrics) before you use it.

### B8. Add effects, or ask the assistant

**By hand:**

1. Find the **Effects** list. Drag an effect onto a row. Or move to an effect with the Tab key
   and press Enter to add it at the playhead.
2. Select the effect on the timeline to change its settings.
3. Press Space to play. Press L to loop.
4. If a row is missing, choose **Add row**.

**With the assistant:**

1. Choose **Assistant** in the top bar (or press ⌘L).
2. The first time, choose **Set up in Settings** and add your own Anthropic or OpenAI key. The
   assistant is optional. See [privacy and data flow](privacy-and-data-flow.md#the-assistant-ai)
   for what it sends.
3. Ask for what you want, or try "Create a compelling sequence". It asks you to **Choose a
   song…**.
4. Review what it proposes. Choose **Play preview** (or **Preview**) to see it, then **Apply**
   or **Discard**. Nothing changes until you choose **Apply**. **Apply** is one undo step.

**Preview what you made:**

- The preview sits beside or above the timeline. Switch between **2D** and **3D**.
- **Glow** softens the lit pixels so they look like bulbs behind a diffuser. Slide it to none for
  bare bulbs. Glow changes only how the preview and exported videos look.
- **Pop out the preview** opens the preview in its own window, which you can drag to another
  display. **Bigger preview** gives the preview most of the screen.

**If it doesn't:** if the preview looks late against the music, see [Lights lag the music, or
the preview looks late](troubleshooting.md#lights-lag-the-music-or-the-preview-looks-late).

### B9. Play to the lights

You can play a sequence on your real lights from this computer.

1. On **Sequence** or **Play**, turn on **Show on my lights** (the same toggle on both screens).
   With it off, the sequence plays only on screen.
2. Press Space to play, and watch the real lights.
3. On **Play**, choose **Playlist** for the sequences you can play. **Full screen** (or F) gives
   you just the lights. Esc brings the app back.
4. If you see a bar saying "*name* is playing …. While it plays, it overrides PixelFlow on the
   same controllers", choose **Stop FPP**.

**You should see:** the real lights follow the preview. The status bar at the foot of the window
shows "Live output" with a **Stop** button while PixelFlow is sending.

**If it doesn't:** see [Nothing lights](troubleshooting.md#nothing-lights) and [FPP is playing and
overriding PixelFlow](troubleshooting.md#fpp-is-playing-and-overriding-pixelflow).

### B10. Send to FPP for unattended playback

Playing from your computer works while the computer is on and attached. For a show that runs on
its own, put the sequence on the FPP.

1. On **Sequence**, choose **Send to FPP…**.
2. Pick your FPP from the list, or choose **Another address…**, type its address, and choose
   **Check**.
3. Check the **Music** line. Choose **Change…** or **Choose music…** if you need to, or **No
   music** to send lights only.
4. If the FPP already has a file with the same name, choose **Replace it**, **Keep both**, or
   **Use the one already on the FPP**.
5. Under **FPP playlist**, choose whether to add the sequence to a playlist. A playlist is what
   the FPP's scheduler plays at set times.
6. Choose **Send**. Wait for the progress bar.
7. When the window says the files are on the FPP, choose **Play it now on the FPP** to try it.

**You should see:** "…are on" followed by your FPP's name, then "Playing on" the FPP.

To check what is on an FPP later, open it from **Controllers**. The **On this FPP** section lists
its **Sequences**, **Music**, and **Playlists**.

**If it doesn't:** see [Send to FPP fails](troubleshooting.md#send-to-fpp-fails).

You can also choose **Export .fseq…** from the **More ways to export** menu next to it, if you
want the file itself.

### B11. Export a video to share

1. On **Sequence**, open the **More ways to export** menu and choose **Export video…**.
2. Choose the size (1080p or 720p), **Frame rate**, and **What to export**. **Glow** and **Pixel
   size** change how the lights look in the video.
3. Choose **Export…** and pick where to save the MP4.

**You should see:** a progress bar, then "Saved" followed by the file name. Choose **Done**.

## Where to go next

- [Troubleshooting](troubleshooting.md) for problems with lights, colors, and networks.
- [Real-hardware acceptance checklist](hardware-acceptance.md) to check your own setup end to end.
- [Measuring xLights import fidelity](fidelity.md) if you brought a show from xLights.
- [Scale benchmark](benchmarks.md) for how large a show PixelFlow is built to handle.
- [Your data: what stays on your computer](privacy-and-data-flow.md).

To see every shortcut, press ? or choose **Keyboard shortcuts** in the command palette (⌘K).
