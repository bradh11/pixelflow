# Security

## Reporting a vulnerability

Please report it privately, not in a public issue.

Use GitHub's private vulnerability reporting for this repository: open the
[Security tab](https://github.com/bradh11/pixelflow/security) and choose **Report a
vulnerability**, or go straight to
<https://github.com/bradh11/pixelflow/security/advisories/new>. The report is visible only to
you and the maintainer.

If that button isn't there, open an issue that says only that you have a security report, with
no details, and the maintainer will arrange a private way to send it.

A useful report says:

- what you found, and which part of PixelFlow it is in;
- how to reproduce it: the steps, and a file or a device's answer that triggers it if there is
  one (the smallest that still does);
- what it lets someone do;
- the version or commit, and your operating system.

Please don't include real API keys, Wi-Fi passwords, or anyone's personal files.

## What to expect

PixelFlow is maintained by one person in spare time, and is in early development.

- An acknowledgement, usually within a week.
- A first assessment after that: whether it is accepted, and roughly how serious it looks.
- A fix on `develop` when there is one, and credit in the release notes if you want it.
- If a report is declined, a reason.

Please give a reasonable time for a fix before writing about it publicly.

## Supported versions

There is no stable release yet. Fixes go to `develop` and reach `main` with the next release;
earlier commits and tags are not patched.

| Version | Supported |
|---|---|
| The latest `develop` and `main` | Yes |
| Anything older | No |

## What counts

PixelFlow opens files other people made, talks to controllers on your network, and can send
some of your data to services you choose. Reports in these areas are welcome:

- a show, sequence, picture, song, or imported xLights file that crashes or hangs PixelFlow,
  exhausts memory, or makes it read, write, or send something it shouldn't;
- a device on the network that can do the same through what it answers;
- an API key reaching the window, a log, a file, or a model prompt;
- the assistant changing the show, a file, or a controller without the user applying it;
- PixelFlow sending data somewhere the user wasn't told about.

Not vulnerabilities in PixelFlow:

- FPP, Falcon, and WLED controllers accept commands from anything on their network. That is how
  they are built; PixelFlow can't change it.
- Anything that needs another program already running as you on your computer.
- Problems in xLights, FPP, or WLED themselves: please report those to their projects.

How PixelFlow is meant to protect you, and what is known to be unfinished, is in
[docs/security/threat-model.md](docs/security/threat-model.md). What it sends and where is in
[docs/privacy-and-data-flow.md](docs/privacy-and-data-flow.md).
