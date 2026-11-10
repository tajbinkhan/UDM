# Install

Every release on the
[Releases page](https://github.com/tajbinkhan/pullbay/releases) has a file for
each system:

| System | File | |
|---|---|---|
| Windows 10 and 11 | `Pullbay-Setup-<version>.exe` | the installer, below |
| | `Pullbay-<version>-windows-portable.zip` | a [portable copy](portable.md), everything in its own folder |
| macOS 11 or later | `Pullbay-<version>-macos.dmg` | Apple Silicon and Intel |
| Ubuntu, Debian, Mint | `pullbay_<version>_amd64.deb` | `sudo apt install ./pullbay_*.deb` |
| Fedora, openSUSE, RHEL | `pullbay-<version>-1.x86_64.rpm` | `sudo dnf install ./pullbay-*.rpm` |
| Any Linux | `Pullbay-<version>-x86_64.AppImage` | make it executable and run it |
| | `pullbay-<version>-linux-x86_64.tar.gz` | unpack and run `./install.sh` |

The Linux files need glibc 2.35 and OpenSSL 3 (Ubuntu 22.04, Debian 12,
Fedora 36 or newer) and GTK 3. The tray icon needs
`libayatana-appindicator3` (and on GNOME the AppIndicator extension);
notifications use `notify-send`. [Build](building.md#build) says how the files are made.

## Coming from UIDM

Pullbay was called UIDM up to version 0.3.3. Install Pullbay over it: the
first time a Pullbay program starts, it moves UIDM's settings, download
list and logs into Pullbay's folders. It also replaces UIDM's browser
registrations and its start-at-sign-in entry with Pullbay's. Half-finished
downloads resume.

- **Windows**: Setup closes UIDM, installs into a `Pullbay` folder beside
  UIDM's, and removes UIDM's folder and shortcuts.
- **Debian, Ubuntu, Fedora**: the `pullbay` package replaces the `uidm`
  one. `install.sh` removes UIDM's copy in `~/.local`.
- **macOS and AppImage**: delete `UIDM.app` or the UIDM AppImage yourself.

The browser extension has to be loaded again, because UIDM's went with its
folder. Remove the UIDM extension, then load Pullbay's as described below.
It keeps the same ID. In Firefox it has a new ID, `pullbay@pullbay.local`.

## Windows

Run `Pullbay-Setup-<version>.exe`. It installs for your user only, without administrator rights, into
`%LOCALAPPDATA%\Programs\Pullbay`. It registers Pullbay with Chrome, Edge and
Firefox, adds
**Pullbay** to the Start menu (and, if you tick it, the desktop), and lists Pullbay
under **Installed apps**. Setup is not code-signed, so Windows SmartScreen
may warn before it runs.

To install from a source checkout instead, with the build toolchain described
under [Build](building.md#build), run this from this folder:

```powershell
.\install-host.ps1
```

That builds the three programs if needed, registers Pullbay with Chrome, Edge and
Firefox, makes the Firefox copy of the extension, and adds **Pullbay** to the
Start menu.

Either way, load the extension once:

1. Open `chrome://extensions` (or `edge://extensions`) and turn on
   **Developer mode**.
2. Click **Load unpacked** and choose the `extension` folder: the one inside
   the install folder (Setup's last page shows it and can open it), or the
   one in this checkout.
3. Check that its ID reads `epmjakacaejhdhldkpbjnakhfmjjbhfm`. The manifest
   carries a fixed public key, so the ID is the same on every machine.

After updating, reload the extension on the extensions page. It asks for
`webRequest` (to send downloads started by a form as the same POST, to send
Pullbay the browser's whole request, and to notice the videos a page loads),
`webNavigation` (to find the frame a link in an `about:blank` frame belongs
to), `downloads.ui` (to keep the download bubble hidden for a download Pullbay
takes) and `proxy` (to read the browser's proxy setting, which it never
changes), in addition to native messaging, downloads, cookies, context
menus, storage, scripting and notifications. A small
content script runs on every page for the video button; it does nothing
until a video plays.

For streamed videos to end up as one `.mp4`, install
[ffmpeg](https://ffmpeg.org/) and put `ffmpeg.exe` on `PATH`, or choose it in
**Options > General**. Without it Pullbay still downloads streams, as `.ts`
files or as separate video and audio files.

Videos on YouTube, Facebook, Instagram and Vimeo pages need
[yt-dlp](https://github.com/yt-dlp/yt-dlp) as well:
`winget install yt-dlp.yt-dlp` installs it with ffmpeg and Deno, which
yt-dlp uses to read YouTube's player. Setup offers to run that command for
you when winget is there and yt-dlp is not. Pullbay finds `yt-dlp.exe` on
`PATH`, next to Pullbay's programs or next to ffmpeg. Restart Pullbay after
installing it yourself, so it sees the new `PATH`.

To remove Pullbay, uninstall it from **Installed apps**. That closes Pullbay if it
is running, removes the program, its browser registration, shortcuts, the
**Start with Windows** entry and the notification registration, and asks
whether to remove your settings and download list too. Installing a newer
setup over an older one closes Pullbay first and keeps your settings. With the
script, `.\install-host.ps1 -Uninstall` removes what it added. Downloaded
files are left alone either way.

**Updates.** Pullbay looks for a newer release on GitHub once a week (Options >
General can turn that off), and **About** (which also shows the version,
the copyright, and a support link to the issue tracker) checks on demand. When a newer
release has a Setup attached, **Update now** (in About, and next to the
notice in the status bar) has the daemon download `Pullbay-Setup-<version>.exe`
through your proxy settings, only from this repository's release downloads.
It checks the file's size, that it is a Windows program, and its SHA-256
when GitHub lists one, saves it to the temp folder and runs it with
`/frompullbay=yes`. Pullbay then stops its downloads, saves its list and quits;
Setup closes whatever is left without asking again and replaces the files.
Stopped downloads resume from the list as usual. The notice's **×** hides it
for that version (remembered in `window.json`); a newer release shows it
again.

## macOS

Open the disk image and drag **Pullbay** to **Applications**. Pullbay is not
signed with an Apple Developer ID, so the first time, right-click it in
Applications, choose **Open**, then **Open** again (or allow it under
**System Settings > Privacy & Security**). The extension folders are inside
the app: in Finder, right-click Pullbay, **Show Package Contents**, then
`Contents/Resources/extension` (Chrome, Edge, Brave) or
`Contents/Resources/firefox`. In Chrome's **Load unpacked** dialog,
Cmd+Shift+G and `/Applications/Pullbay.app/Contents/Resources/extension` gets
there directly.

## Linux

Install the `.deb` or `.rpm` with your package manager, or run the
AppImage, or unpack the `.tar.gz` and run `./install.sh` (it installs into
`~/.local` for your user; `./install.sh --remove` takes it away). Pullbay is
then among your applications, and `pullbay` starts it from a terminal. The
extension folders are `/usr/lib/pullbay/extension` and `/usr/lib/pullbay/firefox`
for the packages, `~/.local/lib/pullbay/...` for the archive, and inside the
AppImage (`./Pullbay-*.AppImage --appimage-extract` unpacks them).

## Browsers on macOS and Linux

There is no Setup to register Pullbay with the browsers, so the first time Pullbay
runs it registers itself with every browser it finds: Chrome, Edge,
Firefox, Brave, Chromium and Vivaldi (Opera reads Chrome's). Each browser
reads `com.pullbay.host.json` from its own `NativeMessagingHosts` folder, such
as `~/.config/google-chrome/NativeMessagingHosts` or
`~/Library/Application Support/Google/Chrome/NativeMessagingHosts`.
**Options > Browser capture** lists them to add or remove one later. When an
AppImage moves, Pullbay points the browsers at its new place the next time it
runs.

Then load the extension as described for Windows above, from the folder for
your system.

## Firefox

Firefox 128 or later loads the `firefox` folder, a copy of the extension with
its own manifest and the ID `pullbay@pullbay.local`. Firefox runs background scripts
where Chrome runs a service worker, and Chrome warns about Firefox's keys, so
the two cannot share one `manifest.json`. Setup and the macOS and Linux
packages install the `firefox` folder next to `extension`. In a checkout it
is made from `extension`, not kept in git, so a `firefox` folder made before
you updated or changed the extension lacks those changes: `install-host.ps1`
makes it again each time it runs, and so does
`.\installer\make-firefox.ps1 -Out firefox` (or
`node installer/make-firefox.js firefox`). Run one of them after every
update, then reload the add-on. To tell whether a copy is out of date,
`node installer/make-firefox.js --check firefox` (or
`.\installer\make-firefox.ps1 -Check firefox`) lists each file that differs,
is missing or is extra, and fails if there is any; CI checks that both
makers produce the same copy. Setup and `install-host.ps1` register Pullbay with
Firefox (under
`HKCU\Software\Mozilla\NativeMessagingHosts`, with a host manifest that names
that ID). Until the extension is signed by Mozilla, load it for each Firefox
session:

1. Open `about:debugging`, choose **This Firefox**, then **Load Temporary
   Add-on...**, and pick `manifest.json` in the `firefox` folder.
2. In `about:addons`, open Pullbay's **Permissions** and allow **Access your
   data for all websites**. Firefox asks for this separately; without it the
   video button, the selection button, form downloads and **Download all
   links** do not work, though plain downloads are still caught.

Known gaps in Firefox:

- A temporary add-on is removed when Firefox closes.
- Firefox has no way to hide its download panel, as Chrome's `downloads.ui`
  does. It needs it less: it lets the extension cancel a download at its
  headers (`webRequestBlocking`, which only the Firefox copy asks for), so
  a download Pullbay takes there never starts in Firefox. One it only takes
  later (a link with a `download` attribute, say) may show the panel.

These were checked against a stand-in for the browser API in the extension's
tests, not in a running Firefox.

---

[← Back to the help pages](README.md)
