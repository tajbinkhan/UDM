# Known gaps

- On macOS and Linux: the drop target and proxy configuration scripts
  (PAC and WPAD) are Windows-only; the system proxy comes from the
  `http_proxy`, `https_proxy`, `ftp_proxy`, `all_proxy` and `no_proxy`
  variables. On macOS closing to the tray minimizes the window rather than
  hiding it. On Linux the window closes and Pullbay stays in the tray; a
  caught download's save-as dialog or download window opens the main
  window with it.
  macOS notifications have no buttons. Passwords and saved requests are
  sealed with a key in Pullbay's data folder that only your user can read,
  rather than the Keychain or the Secret Service. **Update now** opens the
  release page instead of installing. Microsoft Defender is Windows' own:
  choose a scanner program such as `clamscan`. These builds were tested on
  Linux; the macOS build is checked by the compiler and CI only.

- FTPS data connections do not resume the control connection's TLS
  session (the operating system's TLS, through `native-tls`, offers no way
  to), so a server that requires it, such as vsftpd with
  `require_ssl_reuse` on, may refuse the transfers. An `ftp://` server is
  switched to TLS only when it refuses a plain login and asks for TLS, not
  whenever it offers `AUTH TLS`.
- The history is written about a second after each change, so a crash can
  lose the last second of progress; the sidecar still has it, so resuming is
  unaffected.
- Under the rename setting, the save-as dialog shows the proposed name, not
  the `name (1)` the engine may pick if that name is taken when the download
  starts.
- Starting a download from the browser popup while it waits for the save-as
  dialog answers no question: under the ask setting a taken name gets a new
  name, and an address already in the list is added with a new name.
- A copy added with a new name while the old download has a matching
  partial file resumes that partial file instead of starting a new one, so
  both entries point at it.
- Adding a category does not add its types to the captured types; that list
  is edited on its own.
- A schedule only works while the daemon runs. It stays up for a schedule
  once started, but after a reboot something has to start it: turn on Start
  with Windows, or open the window or the browser.
- A queue run started by hand with nothing in it ends at once, and a run that
  is only waiting on failed downloads ends too: failed downloads count as
  finished unless the queue retries them.
- A form download that sends a file stays in the browser (the browser does
  not share the file), and a POST download cannot resume: a failure means
  sending the form again with Retry.
- Chrome does not let an extension cancel a download at its headers, so
  there it is still taken from the browser as it starts; Firefox cancels it
  at the headers.
- Streams: live streams, DASH manifests split into several periods (often
  ads), and HLS `SAMPLE-AES` without a key system are refused. The size
  shown for a quality is an estimate from its bit rate. Without ffmpeg,
  separate video and audio stay two files.
- Video pages: Pullbay does not give yt-dlp the browser's cookies, so videos
  that need signing in (private or members-only ones, most of Facebook and
  Instagram) are not offered. yt-dlp's automatic captions are not offered,
  only a page's own subtitles. A page on another site is tried with yt-dlp
  only when the browser saw no stream in its tab and only when the video
  button's menu opens; a page without any player gets no button, so its
  video can only be added by its address. A playlist's videos are added as
  one batch with one quality for all; the same video added twice is not
  asked about as a duplicate (video pages, like streams, are never
  duplicates), only a taken file name is. A pasted playlist address in Add
  URL is downloaded as a page, not listed.
- Installing yt-dlp and ffmpeg: Pullbay checks each download against the
  SHA-256 published in the same GitHub release, not against yt-dlp's GPG
  signature. There is no ffmpeg to install on macOS (no build there
  publishes a checksum), so Options names Homebrew. Unpacking ffmpeg needs
  the system's `tar` (Windows 10 version 1803 and later have it). A yt-dlp
  that is running cannot be replaced on Windows, so an update waits for the
  next day's look. Only a yt-dlp Pullbay installed is kept up to date.
- The video button sits outside a video in full screen, so it is hidden
  then, and it only knows the media its tab loaded after the extension
  started; reloading the page fills the list.
- The site grabber reads pages as the server sends them unless **Find links
  that page scripts add** is on. That option needs the browser extension: it
  opens each page in a background tab, which is slower and shows the tabs
  briefly. Without the browser connected, pages are read as sent.
- Clipboard watching runs in the window, so it stops when the window is
  closed with **Exit**.
- Every client sees every download, not only the ones it started.
- A process that squats on the port cannot learn anything, but it does stop
  downloads from starting until it goes away. A per-user named pipe would close
  that gap.
- Other processes running as the same user can read the token. That is outside
  this design's threat model: such a process can already read the browser's
  cookies directly.

---

[← Back to the help pages](README.md)
