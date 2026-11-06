---
name: observe-idm
description: Watch Internet Download Manager run to confirm its behavior before building UIDM's version of it, or to read its dialogs and icons. Use when an IDM-parity change depends on how IDM's windows, dialogs or tray act, or on what a dialog contains.
---

Decompiled code shows what IDM *can* do; only a run shows which path a user takes. On 2026-11-06 UIDM's minimize button was built to hide the download window in the tray, from IDM's tray code, and a run then showed that IDM's minimize goes to the taskbar and only its own caption button hides to the tray. Confirm by running IDM before building.

## Steps

1. **Name the behavior to confirm** as observations: which window opens, whether it has a taskbar button, is on top, takes the focus, is hidden or minimized, and when. Done when each one is a yes/no that a window log can answer.
2. **Start the logger** in the background, with IDM running:
   `pwsh -File tools/idm/watch-idm.ps1 -Log <scratchpad>/idm-log.txt -Seconds 900`
3. **Make IDM act.** Catch a download in the user's Chrome (ask first, and say the file and its size), or drive a window by handle with `tools/idm/idm-window.ps1` (Click, Minimize, Restore, Capture, Caption). Use window messages only: keystrokes and real clicks land in whatever program is in front.
4. **Read the log** and answer each observation from step 1 with the lines that show it. Done when every observation has a quoted log line.
5. **Clean up**: stop the logger, delete any full-screen captures (they show the user's screen), close tabs you opened.

## Reference

- **Dialog contents**: run REA `inspect_pe_resources` on `C:\Program Files (x86)\Internet Download Manager\IDMan.exe`, save the result to a file, then `python -I tools/idm/decode-dialogs.py <file> <IDMan.exe> "<text>|<text>"`.
- **Icons**: `python -I tools/idm/extract-icons.py <file> <IDMan.exe> <folder>`.
- **Code behind a behavior**: REA with Ghidra (`open_binary`, `procedure_pseudo_code`, `procedure_callers`). IDM's RTTI names survive, such as `CTrayIconController`.
- **Window flags**: in the log's `exstyle`, `0x8` is always-on-top, `0x40000` a taskbar button (`WS_EX_APPWINDOW`), `0x80` a tool window (no taskbar button). `fg=True` is the window with the focus.
- **Tray icons in the hidden area** are not in the log. Windows lists every icon an app registered under `HKCU\Control Panel\NotifyIconSettings`, by executable and ID.
