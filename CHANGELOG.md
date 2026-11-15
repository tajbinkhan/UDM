# Changelog

Each release's full notes are in [`release-notes/`](release-notes/), and the same text is on its
[GitHub Release](https://github.com/tajbinkhan/UDM/releases). This page lists them, newest
first. UDM was called Pullbay in version 0.4 and UIDM up to version 0.3.3.

## Unreleased

The window and the browser extension now come in Hebrew, Arabic, Persian and Hindi too, 21
languages in all (#251). Hebrew, Arabic and Persian lay the windows out right to left. The first
start asks which language to use, starting on the system's, and **View > Language** changes it
at any time. The new tables were made with a language model and wait for a native speaker's
review ([Help translate UDM](docs/translating.md)).

## [0.5.0](release-notes/v0.5.0.md) - 2026-11-14

Minor. Pullbay is now called UDM, with a new icon and a new window drawn with Tauri and styled
like Windows 11. Adds a command-line tool, an MCP server for AI assistants, control from a phone,
a server mode, video sites through yt-dlp, image galleries, packages, FTPS and 13 more languages.

Update now installs only files the release's signed checksum list names (#267). Each release
carries `SHA256SUMS` and its minisign signature, `SHA256SUMS.minisig`, and UDM refuses an update
when the signature is missing, is not by UDM's release key, or the file's SHA-256 is not on the
list. Update now also works on macOS and Linux: it replaces `UDM.app` from the disk image, the
AppImage, or a `.tar.gz` copy installed with `install.sh`, and starts the new copy. A `.deb` or
`.rpm` copy says to update through the package manager.

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
