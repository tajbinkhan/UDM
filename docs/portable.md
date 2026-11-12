# Portable mode

A portable copy of UDM keeps everything in its own folder, so it can run
from a USB stick and leave the computer as it found it. The Windows portable
zip, `UDM-<version>-windows-portable.zip`, is one.

## What makes a copy portable

A file called `portable.txt` beside the programs, or in the folder above
`target\release` as the zip has it. What the file says does not matter. Delete
it and the copy uses the usual folders in your user profile again (see
[Install](install.md)); add it to any copy to make that copy portable.

## Where things go

In the folder holding `portable.txt`:

| | |
|---|---|
| `Data` | the settings, the theme files (`Data\themes`), the download list, the grabber's projects, the logs, the window's layout, the browsers' host manifests, and the parts of downloads under way (`Data\parts`) when Options names no temporary folder |
| `Downloads` | new downloads, unless Options or the save-as dialog names another folder |

A named copy (`UDM_INSTANCE`, see [Building](building.md)) uses
`Data-<name>` instead of `Data`.

## What it leaves on the computer

Nothing in the registry but the browser registrations: the
`NativeMessagingHosts\com.udm.host` key of each browser UDM is
registered with. A portable copy:

- registers with the browsers it finds the first time it runs, and points
  their registrations at itself again when it has moved, for example to a USB
  stick with another drive letter;
- does not start when you sign in: **Start with Windows** in Options is
  greyed out;
- does not register a name and icon for its notifications, so Windows may show
  them without UDM's name, or not at all. The download-complete window
  opens as usual;
- does not move anything Pullbay or UIDM left behind, which belongs to an installed copy.

Two things still live outside the folder:

- Passwords for site logins (Options, **Connection**, **Site logins**) are kept in Windows
  Credential Manager, the macOS keychain or the Secret Service on Linux.
  Remove the logins in Options before you leave.
- Proxy passwords sit in the `Data` folder, sealed for your Windows account on
  that computer. On another computer, type them again.

The browser extension is loaded from the copy's `extension` folder, so the
browser keeps a note of that folder until you remove the extension.

## Starting

1. Unpack the zip, for example onto a USB stick.
2. Start `target\release\udm-gui.exe`. UDM registers with Chrome,
   Edge, Firefox and the other browsers it finds.
3. Load the extension once from the copy's `extension` folder (Firefox: the
   `firefox` folder), as [Install](install.md#windows) describes.

`install-host.ps1` also works in a portable copy: it writes the manifests into
`Data` and adds no Start menu shortcut.

## Leaving

1. In Options, on the **Browser capture** page, untick every browser under
   **Browsers**. That removes UDM's registrations from the registry.
2. Remove the extension from the browser.
3. Quit UDM from the tray, and take the folder with you.

`install-host.ps1 -Uninstall` removes every browser registration at once, as
for an installed copy.
