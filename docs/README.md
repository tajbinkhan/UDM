# Pullbay: help pages

Pullbay is a free download manager for Windows, macOS and Linux. It works the way Internet
Download Manager users expect: a browser extension that hands downloads over, a list of every
download, categories, queues and a scheduler. Its downloads run in a background program, so they
keep going after the browser closes.

These pages were the README's manual. Each covers one part of Pullbay.

## Start here

- **[Install](install.md)**: Setup, the portable zip, the `.dmg`, the Linux packages, loading the
  browser extension (Chrome, Edge, Brave, Vivaldi, Opera and Firefox), ffmpeg and yt-dlp, updates,
  and moving over from UIDM.
- **[Portable mode](portable.md)**: running from a USB stick with everything in Pullbay's own
  folder.
- **[Using Pullbay](using-pullbay.md)**: the window, the save-as dialog, categories, queues, the
  tray, the download window, the video button, adding many links and Options.
- **[Themes and interface size](themes.md)**: the built-in themes, high contrast, an interface
  up to 200%, and making a theme of your own.
- **[Command line and automation](command-line.md)**: IDM's switches, the `pullbay-cli` tool,
  sending links from other programs, and the automation API for programs of your own.
- **[AI assistants (MCP)](ai-assistants.md)**: letting Claude Desktop, Claude Code, Cursor or VS
  Code list, add, pause, resume and remove downloads through `pullbay-mcp`.

## Downloading

- **[What the browser extension takes](browser-capture.md)**: which downloads the extension hands
  to Pullbay, which it leaves with the browser, and the keys that change that.
- **[Streams, batches and the site grabber](streams-batches-and-grabber.md)**: HLS, DASH and HDS
  videos, YouTube and other video sites through yt-dlp, subtitles, numbered batches, and crawling
  a site for its files.
- **[Queues and the scheduler](queues-and-scheduler.md)**: running downloads a few at a time,
  starting and stopping them by the clock, and what happens when a queue is done.
- **[Proxies and logins](proxies-and-logins.md)**: the system proxy, manual and SOCKS proxies, PAC
  scripts, and site logins.
- **[Notifications and virus scans](notifications-and-virus-scans.md)**: what Pullbay tells you
  when a download ends, and scanning finished files.

## How it works

- **[How Pullbay fits together](architecture.md)**: the daemon, the host and the window, how the
  daemon checks its clients, and how the engine splits and resumes a file.
- **[Messages](protocol.md)**: every message the extension, the window and the daemon exchange.
  The stable part of them is the [automation API](command-line.md#the-automation-api), with its
  [JSON Schema](automation-api.schema.json).
- **[Building Pullbay](building.md)**: where the code lives, building and packaging it on each
  system, and adding a language.

## Status

- **[Known gaps](known-gaps.md)**: what doesn't work yet, or works differently on macOS and Linux.
- **[Verified](verified.md)**: what each change was checked against.
- **[Releases](https://github.com/tajbinkhan/pullbay/releases)**: download the latest version.
- **[Problem reports](problem-reports.md)**: what Pullbay writes down when a program crashes or
  freezes, where it keeps it, and Help > Report a problem.
- **[Report a problem](https://github.com/tajbinkhan/pullbay/issues)**: or ask a question.
