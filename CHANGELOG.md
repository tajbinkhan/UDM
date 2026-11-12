# Changelog

Each release's full notes are in [`release-notes/`](release-notes/), and the same text is on its
[GitHub Release](https://github.com/tajbinkhan/UDM/releases). This page lists them, newest
first. UDM was called Pullbay in version 0.4 and UIDM up to version 0.3.3.

## Unreleased

A preview of UDM's next window, built with Tauri (ADR 0002), comes as `udm-gui-next` beside the
current one. It has the main window, Add URL, Download File Info, the download progress window,
Download complete, Confirm deletion, About, Options, the Scheduler, the site grabber, batch downloads, import and export, Find, file properties, the drop target, the list's font, a customizable toolbar and the tip of the day, the login prompt, the countdown before the computer sleeps or shuts down, New link, package passwords and the Packages list, the browser's links, videos and galleries to pick from, the update check, and Options' own pages for the speed limit, downloads' behavior, the browsers, archives, tools, remote control and custom themes, problem reports and a watchdog, the clipboard monitor, the zip preview, Convert, Verify checksum, Open with, the tray's extras and the command line, laid out like Internet Download Manager's, in a
light and a dark look that follow the Theme setting (or the system's mode). The other windows
still open in `udm-gui`.

Pullbay is now called UDM (Unified Download Manager), with a new icon. Installing it over Pullbay or UIDM keeps
your settings, list, saved site passwords and half-finished downloads, and Pullbay's Update now
can install it.

On Windows, the first download after the background service has stopped starts about two seconds
sooner: the browser, the window and `udm-cli` no longer wait for Windows to report that the
service isn't running before they start it. The service also no longer quits while short
commands keep using it. A command or download that came and went within five seconds did not
count, so the service could quit less than a minute after it.

## [0.4.0](release-notes/v0.4.0.md) - 2026-11-09

Minor. UIDM is now called Pullbay, with a new icon, and keeps your settings, list and
half-finished downloads. The background service's errors show in your language, and videos from
sites that check the player's headers download.

## [0.3.3](release-notes/v0.3.3.md) - 2026-11-08

Patch. Fixes two problems with the windows on Linux under Wayland.

## [0.3.2](release-notes/v0.3.2.md) - 2026-11-08

Patch. On Linux, closing the window leaves UIDM in the tray, as in IDM, and its icon shows in the
dock.

## [0.3.1](release-notes/v0.3.1.md) - 2026-11-08

Patch. Fixes problems found on Linux, a download that failed after fetching every byte, and test
copies that interfered with the installed copy.

## [0.3.0](release-notes/v0.3.0.md) - 2026-11-06

Minor. The tray and dialogs move closer to IDM's. Adds HTTP/2, NTLM logins, subtitles from
streams, and keeping the computer awake for downloads and schedules.

## [0.2.1](release-notes/v0.2.1.md) - 2026-11-04

Patch. Fixes a browser extension error from 0.2.0.

## [0.2.0](release-notes/v0.2.0.md) - 2026-11-04

Minor. A menu bar, toolbar, category tree and IDM's columns, gaps found by comparing with IDM
6.43 closed, and security holes found in a code review fixed.

## [0.1.0](release-notes/v0.1.0.md) - 2026-10-31

The first release. It catches downloads in Chrome, Edge and Firefox, fetches each file over
parallel connections, resumes after interruptions, and keeps downloading after the browser
closes, on Windows, macOS and Linux.
