# Real-hardware acceptance checklist

Run this checklist against real controllers and real lights to show that a PixelFlow build works
end to end. Print it or copy it, and fill it in as you go. A tester needs a computer running
PixelFlow, an FPP, a pixel controller, and some strings.

This is a record of what a person checked. PixelFlow's automated tests don't cover real
hardware. Mark a row only if you did that check. Leave it blank otherwise.

New to the app? [Your first show](first-show.md) walks through the same screens. If a row fails,
see [Troubleshooting](troubleshooting.md).

- [How to use this checklist](#how-to-use-this-checklist)
- [Safety notes](#safety-notes)
- [Test run details](#test-run-details)
- [1. Import](#1-import)
- [2. Wiring and mapping](#2-wiring-and-mapping)
- [3. Live output from PixelFlow](#3-live-output-from-pixelflow)
- [4. FSEQ export and Send to FPP](#4-fseq-export-and-send-to-fpp)
- [5. Long run](#5-long-run)
- [6. Recovery](#6-recovery)
- [Known-good reference run](#known-good-reference-run)

## How to use this checklist

- Tick **Pass** or **Fail** for each row, and write what you saw in **Notes**. Leave both blank for
  a row you didn't run.
- Run rows in order. Later groups depend on earlier ones.
- For a failed row, write the exact words of any message PixelFlow showed.
- Test one port at a time before you test the whole show.
- Use generic names in your notes if you share the filled-in copy. Leave out your addresses and
  the names of your props and songs.

## Safety notes

- **Never push a controller configuration during a live show.** **Send setup** can make an FPP's
  or a WLED's lights flicker or go dark while it saves. PixelFlow warns you: "This FPP is playing
  … Its lights may flicker or go dark while the new setup is saved." Stop the show first, send
  the setup, then start the show again.
- **Nothing changes on a controller until you click.** Every dialog shows what will change
  before it writes. This includes **Send setup to this controller…**, **Send to FPP…**, and
  **Play**, **Stop now**, and **Stop after this** on an FPP.
- **What Send setup changes.** It writes only pixel-output settings, and what the controller
  receives. It never writes Wi-Fi or network settings. At the time of writing it supports FPP and
  WLED. A Falcon controller is read only: its **Send setup** button is off, with the reason shown on hover, so set up a Falcon's pixel ports on its own page.
  The dialog subtitle says: "Only its outputs change; network settings are never touched."
- **What happens after Send setup.** The dialog reads the setup back and reports whether the
  controller now matches your show. For an FPP, the new outputs are used after the FPP's player
  restarts. Restart it from the FPP's own web page when no show is playing, then check its
  warnings. A WLED that is set to listen on a new port needs a reboot from its own page.
- **Put back restores the old setup.** Before the first change to a controller, PixelFlow keeps a
  copy of its setup. The copy kept is the oldest one not yet put back, so a later send never
  replaces the setup from before PixelFlow first changed it. It stays across restarts until you
  put it back or choose **Forget this copy**.
  1. Open the controller's **Send setup** dialog and choose **Put back the previous setup…**.
  2. PixelFlow reads the controller at that address. It refuses to put the copy on a different controller.
  3. The dialog lists what Put back will change. Choose **Put back on** the controller's name to
     write it, or **Don't put it back** to leave it.
- **Keep a manual backup too.** Before your first Send setup on a controller you care about, save a
  copy of its configuration from its own web page. Put back covers what PixelFlow wrote, but a
  copy from the controller is a second safety net.
- **Check the address before you send.** A show file holds controller addresses. If you open a
  show someone else made, the addresses are theirs. Look at the controller list before you press
  **Play** or **Send**.
- **Stop output when you are done.** The status bar shows "Live output" with a **Stop** button
  while PixelFlow is sending. A test ends with a blackout frame.

The **Send setup** dialog lists what will change, port by port, before anything is written:

![The Send setup to Main FPP dialog: it makes 3 changes, 2 of which turn pixels off or move them, and keeps a copy of the current setup to put back. Port by port: a string's pixel count going from 500 to 512, another from 850 to 800 (the last 50 go dark), and a string removed (its 100 pixels go dark). Cancel and Send to Main FPP at the foot.](images/send-setup.png)

## Test run details

Fill this in first.

| Item | Value |
|---|---|
| Date | |
| Tester | |
| PixelFlow version or commit | |
| Computer and operating system | |
| FPP version | |
| FPP hardware (for example, Raspberry Pi model) | |
| Controller model | |
| Controller firmware | |
| Protocol (DDP or sACN) | |
| Number of ports used | |
| Pixels on the longest port | |
| Frame rate of the show | |
| Pixel type and color order | |

## 1. Import

| # | Check | How | Pass | Fail | Notes |
|---|---|---|---|---|---|
| 1.1 | An xLights show folder imports | Welcome, **Import from xLights…**, pick the folder with `xlights_rgbeffects.xml`. The "Imported" window opens. | [ ] | [ ] | |
| 1.2 | Counts match xLights | Compare the props, pixels, controllers, and groups in the window with xLights. | [ ] | [ ] | |
| 1.3 | Import notes make sense | Read "These weren't imported exactly:". Each note names something you can find in the show. | [ ] | [ ] | |
| 1.4 | The show saves and reopens | **Save show…**, close it, then reopen it from the welcome screen. Nothing is missing. | [ ] | [ ] | |
| 1.5 | A controller is added by scan | **Controllers**, **Scan network**, **Open**, then add it to the show. | [ ] | [ ] | |
| 1.6 | A controller is added by address | **Controllers**, type its address, **Check address**. | [ ] | [ ] | |
| 1.7 | An FPP sets up the show | Open the FPP, **Set up my show from this FPP**, and add the controllers it lists. | [ ] | [ ] | |
| 1.8 | A sequence imports | **Sequence**, **Import from xLights**, pick an `.xsq`. The report says how much of the sequence comes across the same, looks close, and looks different. | [ ] | [ ] | |
| 1.9 | A vendor sequence maps | Import a vendor `.zip`. In the "Map … onto your props" window, **Auto-map**, fix **Not mapped** rows, **Import** the effects. | [ ] | [ ] | |

## 2. Wiring and mapping

Do these one port at a time. Use **Test** with the port as the **Target**.

| # | Check | How | Pass | Fail | Notes |
|---|---|---|---|---|---|
| 2.1 | First pixel of each port lights | **Pixel walk**: the first lit pixel is the first pixel of the first prop on that port. | [ ] | [ ] | |
| 2.2 | Last pixel of each port lights | **Chase** runs to the last pixel of the last prop and no further. Nothing past it lights. | [ ] | [ ] | |
| 2.3 | Pixel count of each port matches | The number of pixels that light equals the string's real count. | [ ] | [ ] | |
| 2.4 | Port numbers match the controller | **Identify (blink)** on port N blinks the string plugged into port N. | [ ] | [ ] | |
| 2.5 | Props appear in wiring order | **Chase** on a port with several props runs through them in the order they are wired. | [ ] | [ ] | |
| 2.6 | Color order is RGB-correct | **Red / green / blue / white** shows red, green, blue, then white. Note the order for each string. | [ ] | [ ] | |
| 2.7 | Color order of each string type | If there is more than one kind of pixel, repeat 2.6 for each. | [ ] | [ ] | |
| 2.8 | Reverse works | On a prop wired from its far end, tick **Starts at the other end**. **Chase** then runs the right way. | [ ] | [ ] | |
| 2.9 | Null pixels work | Set **Empty pixels before it** on a prop that follows a dark lead. The prop's first pixel is the first one lit. | [ ] | [ ] | |
| 2.10 | Brightness works | Set **Brightness (%)** on one prop. It is dimmer than the others by about that amount. | [ ] | [ ] | |
| 2.11 | Gamma works | Run **Brightness ramp**. It looks even from dark to bright for your pixels. | [ ] | [ ] | |
| 2.12 | The port limit warning is right | Add pixels to a port until the red message appears. It appears at about the controller's real limit. | [ ] | [ ] | |
| 2.13 | Every wired prop is on the right port | Use **Whole show** with **Chase**. Props light in the order you expect. | [ ] | [ ] | |

## 3. Live output from PixelFlow

| # | Check | How | Pass | Fail | Notes |
|---|---|---|---|---|---|
| 3.1 | The controller shows Web page answering | **Test**, under **Sends to**. This checks only the controller's web page, not that light data arrives. | [ ] | [ ] | |
| 3.2 | A test pattern lights the lights | **Start** a **Solid color** on one port. | [ ] | [ ] | |
| 3.3 | The pattern runs at a steady rate | The table shows `ok`, a rising **Packets** count, and **Errors** at 0. | [ ] | [ ] | |
| 3.4 | Changing the color is live | While running, change **Color**. The lights change without a restart. | [ ] | [ ] | |
| 3.5 | The whole show lights | **Whole show**, **Chase**. | [ ] | [ ] | |
| 3.6 | Stop blacks out | **Stop**. The lights go dark. | [ ] | [ ] | |
| 3.7 | A sequence plays on the lights | **Play**, turn on **Show on my lights**, and play a sequence. The lights follow the preview. | [ ] | [ ] | |
| 3.8 | Music plays with the lights | The sound matches the lights. Note any lag, in milliseconds if you can. If the lights are early or late, try **Light timing** (Play, Playlist, Music). | [ ] | [ ] | |
| 3.9 | Pause and resume | Pause, then play. The lights and the music stay together. | [ ] | [ ] | |
| 3.10 | Seek | Move the seek bar. The lights jump to the right place. | [ ] | [ ] | |
| 3.11 | Stopping a sequence blacks out | **Stop**. The lights go dark. | [ ] | [ ] | |
| 3.12 | A running FPP is flagged | Start a show on the FPP and open **Play**. The bar "… is playing" appears, and **Stop FPP** stops it. | [ ] | [ ] | |

## 4. FSEQ export and Send to FPP

| # | Check | How | Pass | Fail | Notes |
|---|---|---|---|---|---|
| 4.1 | An `.fseq` exports | **More ways to export**, **Export .fseq…**. The summary gives the frame count and length. | [ ] | [ ] | |
| 4.2 | Send to FPP completes | **Send to FPP…**, pick the FPP, **Send**. The window says the files are on the FPP. | [ ] | [ ] | |
| 4.3 | The sequence file is on the FPP | Open the FPP in **Controllers**. It is listed under **Sequences** in **On this FPP**. | [ ] | [ ] | |
| 4.4 | The music is attached | The music file is listed under **Music**. | [ ] | [ ] | |
| 4.5 | A playlist was made or updated | If you chose one, it is listed under **Playlists**. | [ ] | [ ] | |
| 4.6 | The channel check is quiet | There is no "The channels may not match …" warning, or you understand each one. | [ ] | [ ] | |
| 4.7 | It plays from the FPP | **Play it now on the FPP**. The lights and music play with PixelFlow closed. | [ ] | [ ] | |
| 4.8 | First frame is right | The first frame looks like the first frame in PixelFlow. | [ ] | [ ] | |
| 4.9 | Last frame is right | The last frame looks like the last frame in PixelFlow. | [ ] | [ ] | |
| 4.10 | Timing matches | The lights match the music as well as they do in PixelFlow. Note any lag. | [ ] | [ ] | |
| 4.11 | Stop works | **Stop now** on the FPP page stops it. | [ ] | [ ] | |
| 4.12 | Restart works | Play it again from the start. It matches the first run. | [ ] | [ ] | |
| 4.13 | Blackout at the end | When the sequence ends, the lights go dark (or whatever your FPP is set to do). Check the FPP's own page for its end-of-show setting. | [ ] | [ ] | |
| 4.14 | Replace works | Send the same sequence again and choose **Replace it**. The new version plays. | [ ] | [ ] | |
| 4.15 | Send setup shows the changes first | Open **Send setup to this controller…**. The dialog lists changes port by port. Close it without sending, unless this is a planned test. | [ ] | [ ] | |
| 4.16 | Send setup and Put back | On a test controller, send the setup. Check the lights. Then **Put back the previous setup…**. The controller returns to how it was. | [ ] | [ ] | |

## 5. Long run

| # | Check | How | Pass | Fail | Notes |
|---|---|---|---|---|---|
| 5.1 | A full song plays | Play one full song from the FPP. Note its length. | [ ] | [ ] | |
| 5.2 | The end of the song is clean | The last seconds look right, with no stuck pixels. | [ ] | [ ] | |
| 5.3 | Several songs back to back | Play at least three songs in a row from an FPP playlist. | [ ] | [ ] | |
| 5.4 | No gap or glitch between songs | Note the gap between songs. | [ ] | [ ] | |
| 5.5 | Loop | Loop a playlist for a set time (note it). It restarts cleanly each time. | [ ] | [ ] | |
| 5.6 | No drift | At the end of a long run, the lights are still in time with the music. | [ ] | [ ] | |
| 5.7 | No flicker or dropped frames | Watch for flicker over the whole run. Note when and where. | [ ] | [ ] | |
| 5.8 | PixelFlow stays steady | Play from PixelFlow for a full song. It doesn't slow down and the table shows no `degraded`. | [ ] | [ ] | |
| 5.9 | The controller stays cool and stable | It doesn't restart or drop out. | [ ] | [ ] | |

## 6. Recovery

| # | Check | How | Pass | Fail | Notes |
|---|---|---|---|---|---|
| 6.1 | Unplug the network cable during a test | The table shows `degraded` or errors. The lights hold or go dark. PixelFlow doesn't freeze. | [ ] | [ ] | |
| 6.2 | Plug it back in | Output resumes without restarting the test. | [ ] | [ ] | |
| 6.3 | Controller power off, then on | PixelFlow's **Sends to** goes to "Web page not answering", then **Web page answering** after **Check again**. | [ ] | [ ] | |
| 6.4 | Reboot the FPP while idle | After it is back, it is listed again and the files are still on it. | [ ] | [ ] | |
| 6.5 | Reboot the FPP during playback | After it is back, the playlist behaves as your FPP's schedule says. Note what it did. | [ ] | [ ] | |
| 6.6 | Send to FPP while the cable is pulled | The window gives a clear message, such as "The FPP at … stopped answering during the upload." Nothing is left half-sent, or the message names what is. | [ ] | [ ] | |
| 6.7 | Cancel a send | **Cancel** during a send. The message says nothing on the FPP was changed. | [ ] | [ ] | |
| 6.8 | Restore from History | Make a change, then restore an earlier backup on **History**. **Undo** takes it back. | [ ] | [ ] | |
| 6.9 | Unsaved sequence recovery | Quit PixelFlow with an unsaved sequence. On the next start, the **Sequence** screen offers it back. | [ ] | [ ] | |

## Known-good reference run

This section records only what is known. It is not a record of any check on this list.

- **Setup:** FPP on a Raspberry Pi 3B+, sending DDP to a Falcon F16V5.
- **Result reported by the owner:** PixelFlow-made sequences loaded and played successfully on this
  setup.

The detailed rows above have not been filled in for this setup. They are left for the owner to
complete.

| Item | Value |
|---|---|
| FPP hardware | Raspberry Pi 3B+ |
| Controller model | Falcon F16V5 |
| Protocol | DDP |
| PixelFlow version or commit | |
| FPP version | |
| Controller firmware | |
| Rows 1.1 to 6.9 | Not yet filled in |
