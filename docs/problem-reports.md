# Problem reports

When a UDM program closes by itself or stops responding, UDM writes a report to its data
folder. The window offers the report the next time it opens, and can open a GitHub issue for it in
your browser. Nothing is sent anywhere by UDM: you see the issue before you submit it, and you
attach the report yourself.

## When UDM writes one

- **A program crashes.** The window, the background service (the daemon), the browser's host,
  `udm-cli` and `udm-mcp` each write a report as they close. A program writes at most three in one run, so
  one that keeps failing doesn't fill the disk.
- **The window freezes.** If the window takes more than 10 seconds to draw itself, a watchdog
  writes a report while it waits, so one is kept even if you close UDM by force. When the
  window answers again, a strip under the toolbar says how long it stopped for: **Save a report**
  keeps the report, and the close box deletes it.
- **The daemon stops answering.** The window checks every few seconds that the daemon still
  answers. If it hasn't for 30 seconds, while its connection stays open, a strip offers to **Save a
  report** or to restart the daemon. The strip goes away if the daemon answers again.

The next time the window connects to the daemon, it offers each new report once: **Open an issue**
opens a new issue on GitHub with the crash form filled in and shows the report file so you can drag
it into the issue; **Show the report** only shows the file.

## Report a problem

**Help > Report a problem** opens a new bug report on GitHub with UDM's version, your system
and how UDM was installed filled in. Describe what happened and submit it there. To attach a
log or a report, see below for where they are.

## Where the reports are

In a `reports` folder in the data folder:

| System | Folder |
|---|---|
| Windows | `%LOCALAPPDATA%\UDM\reports` |
| macOS | `~/Library/Application Support/UDM/reports` |
| Linux | `~/.local/share/udm/reports` |

A [portable copy](portable.md) keeps them in `Data\reports`. Each report is a text file named after
the program, what happened and when, such as `udm-daemon-crash-1762778096123.txt`. UDM
keeps the newest 20 and deletes older ones. `offered.txt` beside them lists the ones the window has
already offered. You can delete any of these files.

## What a report holds

- UDM's version, the system (Windows, macOS or the Linux distribution, the desktop and X11 or
  Wayland, and the processor) and how UDM was installed.
- For a crash: the error, where in the code it happened, the thread, and a backtrace. On Windows
  the backtrace lists no function names.
- The last 200 lines of that program's log.

Before a report is written, UDM takes out the daemon's access token, the passwords in
addresses (`ftp://user:***@host`), and the values of cookies, authorization headers, passwords and
tokens. The logs don't record those to begin with. A report can still show the addresses of your
downloads and the names of files and folders, so look it over before you attach it to a public
issue, and remove anything you'd rather not share.
