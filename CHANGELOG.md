# Changelog

Each release's full notes are in [`release-notes/`](release-notes/), and the same text is on its
[GitHub Release](https://github.com/tajbinkhan/UDM/releases). This page lists them, newest
first. UDM was called Pullbay in version 0.4 and UIDM up to version 0.3.3.

## Unreleased

UDM's window is now drawn with Tauri (ADR 0002), in each system's webview, and the egui window
is gone. It has every window IDM's has but Registration, in a light and a dark look that follow
the Theme setting, plus UDM's own: the browser's link, video and gallery pickers, packages, the
update check, Options' pages beyond IDM's, problem reports, the clipboard monitor, the zip
preview, Convert, Verify checksum and Open with. On Windows a finished file drags out of the list
or Download complete, and Setup installs the WebView2 runtime when Windows lacks it; on Linux the
packages now need WebKitGTK 4.1. The domain's and the background service's errors show in your
language again. Some of the new window's text is still English in other languages. What the
egui window had beyond that is back too: Download File Info warns about a folder UDM may not
save in and offers to save there with administrator rights (its Browse button now works), the
Scheduler and the grabber's schedule warn when the power plan lets nothing wake the computer,
Options leaves off the creation time where the system has none and starting at sign-in in a
portable copy, the link picker hides web pages, images and repeated links, keeps only the types
you name and renames the files by a pattern, and File Properties names a file's type as Windows
does.

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
