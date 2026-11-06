# UIDM

A download manager that follows Internet Download Manager's behavior: it catches downloads from the browser, asks where to save them, and shows each one's progress in its own window.

## Windows

**Main window**:
The window with the list of downloads, categories and queues. It lives in the tray when it is not open.
_Avoid_: App window, UIDM window

**Save-as dialog**:
The dialog that asks where to save a new download and whether to start it now or later. IDM calls it "Download File Info".
_Avoid_: Download File Info, start download dialog, confirm dialog

**Download window**:
One download's own window, with its status, speed, time left, Pause and Cancel. IDM calls it the download progress dialog.
_Avoid_: Progress window, progress dialog

**Completion dialog**:
The dialog shown when a download finishes or fails, offering to open the file or its folder. Options can show a notification instead.
_Avoid_: Download complete dialog, done dialog

## Putting windows away

**Minimize**:
Sending a download window to the taskbar with the standard Windows button. The window keeps its taskbar button.

**Hide**:
Sending a download window to the tray, where the downloads tray icon lists it. It has no taskbar button while hidden.
_Avoid_: Hide to tray, minimize to tray (for download windows)

**Downloads tray icon**:
A second tray icon, two green arrows, that lists the hidden download windows and brings one back. It is there only while a download window is hidden.
_Avoid_: Progress tray, tray controller

**Queue start**:
A download started by a queue rather than by the user. Its download window opens hidden.
