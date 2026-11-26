# Changelog

Each release's full notes are in [`release-notes/`](release-notes/), and the same text is on its
[GitHub Release](https://github.com/tajbinkhan/UDM/releases). This page lists them, newest
first. UDM was called Pullbay in version 0.4 and UIDM up to version 0.3.3.

## [0.14.0](release-notes/v0.14.0.md) - 2026-11-26

Minor. UDM installs only from its own files, so Scoop and winget are gone, and windows on macOS
get their full height back.

## [0.13.0](release-notes/v0.13.0.md) - 2026-11-22

Minor. Install with winget or Scoop, a video's qualities show sooner, and the menus and toolbar
are translated in every language.

## [0.12.1](release-notes/v0.12.1.md) - 2026-11-19

Patch. The main window opens in the middle of the screen instead of the top-left corner.

## [0.12.0](release-notes/v0.12.0.md) - 2026-11-19

Minor. UDM can update from its own download server, and a crafted FTP address can no longer send
a saved login to the wrong server.

## [0.11.3](release-notes/v0.11.3.md) - 2026-11-18

Patch. Check for Update no longer fails with "server returned HTTP 403" on a busy or shared
network.

## [0.11.2](release-notes/v0.11.2.md) - 2026-11-18

Patch. Security fix: a malicious download server can no longer overwrite a file you already have.

## [0.11.1](release-notes/v0.11.1.md) - 2026-11-17

Patch. Stop, Pause and Resume work again for videos downloaded from sites such as YouTube, and a
minimized download window comes back when you ask for it.

## [0.11.0](release-notes/v0.11.0.md) - 2026-11-17

Minor. The browser extension's video button now sits outside the video, and its popup lists
only the downloads you can pause or resume.

## [0.10.1](release-notes/v0.10.1.md) - 2026-11-17

Patch. The browser extension no longer reports an error from tabs that were open when it was
updated or reloaded.

## [0.10.0](release-notes/v0.10.0.md) - 2026-11-16

Minor. Update now shows how far the new version's download has come, in About and in the main
window's update notice.

## [0.9.2](release-notes/v0.9.2.md) - 2026-11-16

Patch. A finished download opens the Download complete dialog by default, as IDM does.

## [0.9.1](release-notes/v0.9.1.md) - 2026-11-16

Patch. A download caught from the browser opens its dialog alone and in front, the dock shows
UDM's icon on Linux, and right-click menus no longer draw under the menu bar's names.

## [0.9.0](release-notes/v0.9.0.md) - 2026-11-16

Minor. Installing yt-dlp, ffmpeg or gallery-dl shows a bar and how much has arrived, and the
download window shows its values in one column with two decimals.

## [0.8.0](release-notes/v0.8.0.md) - 2026-11-15

Minor. Update now installs a new `.deb` or `.rpm` through the system's package manager, asking
for your password, instead of telling you to update it yourself.

## [0.7.1](release-notes/v0.7.1.md) - 2026-11-15

Patch. Adds Open extension folder and Open Firefox extension folder to Options' Browser
capture page, so the extension's folder opens in one click on every system.

## [0.7.0](release-notes/v0.7.0.md) - 2026-11-15

Minor. Offers to install the missing tools at the first start and from the main window,
downloads a new version ahead so it installs in one click, and stops offering the version you
already have after an upgrade.

## [0.6.0](release-notes/v0.6.0.md) - 2026-11-15

Minor. Adds torrents and magnet links, one network adapter for every download with DNS over
HTTPS and the server's country, debrid accounts, a Safari extension, signed updates and four
more languages with right-to-left windows. Downloads start while Download File Info is open.

## [0.5.0](release-notes/v0.5.0.md) - 2026-11-14

Minor. Pullbay is now called UDM, with a new icon and a new window drawn with Tauri and styled
like Windows 11. Adds a command-line tool, an MCP server for AI assistants, control from a phone,
a server mode, video sites through yt-dlp, image galleries, packages, FTPS and 13 more languages.

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
