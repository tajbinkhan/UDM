---
name: observe-idm
description: Watch Internet Download Manager run to confirm its behavior before building Pullbay's version of it, or to see what one of its dialogs shows. Use when an IDM-parity change depends on how IDM's windows, dialogs or tray act, or on what a dialog contains.
---

Learn IDM only the way a user can: run it and watch its windows. Never decompile, disassemble or unpack IDM's programs, libraries or help files, or extract their icons, dialogs or strings; IDM's license forbids it, and Pullbay has to stay an independent rewrite. Don't build from memory either: on 2026-11-06 Pullbay's minimize button was built to hide the download window in the tray, and a run then showed that IDM's minimize goes to the taskbar and only its own caption button hides to the tray. Confirm by running IDM before building.

## Steps

1. **Name the behavior to confirm** as observations: which window opens, whether it has a taskbar button, is on top, takes the focus, is hidden or minimized, and when. Done when each one is a yes/no that a window log can answer.
2. **Start the logger** in the background, with IDM running:
   `pwsh -File tools/idm/watch-idm.ps1 -Log <scratchpad>/idm-log.txt -Seconds 900`
3. **Make IDM act.** Catch a download in the user's Chrome (ask first, and say the file and its size), or drive a window by handle with `tools/idm/idm-window.ps1` (Click, Minimize, Restore, Capture, Caption). Use window messages only: keystrokes and real clicks land in whatever program is in front.
4. **Read the log** and answer each observation from step 1 with the lines that show it. Done when every observation has a quoted log line.
5. **Clean up**: stop the logger, delete any full-screen captures (they show the user's screen), close tabs you opened.

## Reference

- **Dialog contents**: open the dialog in IDM and save it with `idm-window.ps1 -Action Capture`, then read the PNG. Describe what it offers in Pullbay's own words; don't copy IDM's text or images into Pullbay.
- **Window flags**: in the log's `exstyle`, `0x8` is always-on-top, `0x40000` a taskbar button (`WS_EX_APPWINDOW`), `0x80` a tool window (no taskbar button). `fg=True` is the window with the focus.
- **Tray icons in the hidden area** are not in the log. Windows lists every icon an app registered under `HKCU\Control Panel\NotifyIconSettings`, by executable and ID.
