# Cloak the main window in the tray instead of hiding it

Every UDM dialog and download window is drawn during the main window's frame, and Windows sends no paint messages to a window hidden with `SW_HIDE`, so with UDM in the tray a caught download's save-as dialog and its download window could not open. We keep the main window "visible" to Windows but cloaked through DWM (`DWMWA_CLOAK`), minimized and without a taskbar button, so its frames keep coming and dialogs open on their own while it stays out of sight, as IDM's do.

## Considered options

- **Give each dialog its own deferred egui viewport**, drawn independently of the main window. Rejected for now: it moves every dialog's state behind shared, thread-safe ownership, a rewrite of most of the window code.
- **Bring the main window up whenever a dialog opens.** The old behavior; it is what IDM does not do, and it put the whole window in front of the browser for every caught download.

## Consequences

Code that asks Windows whether the main window is visible gets "yes" while it is in the tray. Only Windows 8 and later can cloak a window; on macOS "hidden" stays "minimized" (see `win_other.rs`). On Linux closing to the tray starts a new copy with `--in-tray`, which waits in the tray with its daemon connection and no window, and ends the old one, which takes its window off the screen (Wayland keeps a dropped window until the event loop runs again, so the same process cannot wait). The new copy opens its window when something asks for it (see `reopen.rs`); the dialogs still need that window, so it comes up with them.
