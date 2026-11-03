# IDM parity audit, 2026-11-03

Target: IDM 6.43 `IDMan.exe` (SHA-256 `75ac33a7…2145285`, the same build as the October audit) and the Chrome extension `IDMGCExt.crx`. UIDM checked at origin/main `8143399`.

Method:
- REA `inspect_pe_resources` (Evidence `ev_2a983598…11647`) listed 436 resources with complete coverage. A local decoder read 2 menus, 105 dialogs (34 EX and 71 classic templates) and 37 string tables from REA's payload ranges.
- REA `analyze_javascript_application` read the extracted CRX (Evidence `ev_38401a30…a7d7`, graph partial because the code is minified). The rest came from reading `background.js`, `content.js` and `document.js` directly.
- Six read-only passes checked every control, menu entry and behaviour-bearing string against the code. An item counts as done only when code performs it, not when the string merely exists.

Filed on 2026-11-03 as #133 to #155 (milestones 20 to 23) under roadmap #156. The roadmap's "Not filed" section lists what was left out and why.

Out of scope, unchanged: dial-up/VPN, IE monitoring and cache, toolbar skins and customization, registration, reports to Tonec, component download, Chrome/Edge help screens, admin warning, Tell a friend.

## Bug found

| Where | Problem | Evidence |
|---|---|---|
| Queue dialog (306) "Force processes to terminate" | The GUI sends `force_close`, but the daemon's `save_queue` never copies it. The checkbox is silently lost, so a queue's shutdown never forces programs closed. | `src/bin/uidm-daemon.rs:702-708` (no `q.force_close = edited.force_close`) |

## Missing

Main window and menus
- View > Arrange by Referer. UIDM has one referrer field, used for "By Parent Web Page" (`menubar.rs:238`).
- View > IDM tray icon > 3D style / Classic style (`menubar.rs:248`).
- View > Font > Choose Font / Reset to default. Only a text-size slider exists (`uidm-gui.rs:2747`).
- Help > Tutorials (`menubar.rs:255-267`).
- About: copyright line and support link (`uidm-gui.rs:4207`).

Options
- General (139/231, 297): "Add browser..." for a browser by program file. The list is fixed (`browsers.rs:45-75`).
- General (139): "Restart" button for the browser, and a status line saying whether advanced integration is connected.
- Speed Limiter (337): "Always turn on Speed Limiter on IDM startup" (`settings.rs:414-418`).
- Progress dialog settings (367): "Show 'Hide tab' buttons" (`progress.rs:252,265`).
- Special keys (229): Del as a force key. `CaptureKey` has only Alt, Ctrl, Shift and Insert (`settings.rs:40-45`).
- Checking Internet Connection (230): no connectivity check. It may belong to dial-up.

Adding downloads and queues
- Download later picker (345): "Start queue processing" box and "+" for a new queue. The picker is hidden when only the main queue exists (`uidm-gui.rs:2289-2306`).
- "Remove item from download queue" (string 1231): a download can be moved to another queue but not taken out of every queue (`table.rs:306-354`).
- Queue dialog (306): "Periodic synchronization" that re-checks finished files for changes (`queue.rs:85-132`).
- Queues dialog (308): Help button.

Site Grabber
- "Add Filter" for the "Don't download" field (240) (`tools.rs:1097`).
- "Add checked files to IDM main list and queue on closing the grabber" (259) (`tools.rs:1399-1425`).
- Save-project prompt with "Don't show this dialog if the name is unique" (269).
- Project schedule (284): "Force processes to terminate". `Project` has no such field (`grab.rs:719-758`).
- Capture tip (236): "Read more" help link.

Browser extension
- Context menu "Download with" for the page or frame address itself (`background.js:786`).
- "Transfer download to IDM" for a download the browser already started.
- Link title and `download` attribute sent with a context-menu link (`background.js:903`).
- Hiding the browser's download bubble or shelf while integrated (`background.js:640`).
- "Cannot find the links" and "cannot contact IDM" alerts. `pickLinks` fails silently (`background.js:695-713`).
- Firefox: hide "selected links" when the selection has no links (`background.js:796`).
- Video panel on `<audio>`, `<object>` and `<embed>`. Only `HTMLVideoElement` is handled (`content.js:360-377`).
- Panel tied to one video element. UIDM lists every stream the tab loaded (`background.js:155-168`).
- HDS (F4M/F4F) streams (`media.js:97-110`).
- Page-context re-fetch and XHR/fetch hooks for player responses. Only YouTube goes through yt-dlp.
- Content script injection into tabs already open at install or connect (`manifest.json:11-13`).
- Loopback WebSocket fallback when native messaging fails.
- Drift: `firefox/` lacks #132's `adoptFile` (0 hits there, 3 in `extension/`).

## Partial

Main window and dialogs
- Downloads > Stop All: Pause All does the same job, and the menu shows Resume All in that slot (`menubar.rs:184`).
- Grabber > Load: only lists projects in UIDM's folder, with no file picker (`tools.rs:924-939`).
- Tip of the Day: no "Do you know..." header (`help.rs:51`).
- New-version notice (144, 290): the banner has no close button (`banner.rs:56-59,121-131`).
- Download All Links (228): the Type column comes from the URL extension only. Nothing asks the server for type or size (`links.rs:241-245`).
- File Properties (135): Save To and Browse work only on finished downloads (`properties.rs:190-194`).
- Columns (209): widths are in egui points, not pixels, and there is Close only, with no Cancel to undo (`columns.rs:348,396`).
- Language (208): no first-run prompt. The Windows language is used.
- Find (487/506): name and description are one search text, and links can't be searched alone. With "Whole name only" on, a bare site name never matches (`find.rs:14-19,51-84`).

Options
- Connection (141): no TLS 1.3 toggle. SChannel decides (`Cargo.toml:31-34`).
- Save to (143): General has no explicit type list, and "file creation date from server" sets only the modified time (`engine/mod.rs:673`).
- Downloads (291): no setting to hide the queue panel on batch dialogs.
- Browser menus (334/335): one hidden-items list covers every browser, so Chromium and Firefox menus cannot differ (`settings.rs:122-129`).
- Web players (340): no option to capture web-player media automatically.
- Speed Limiter (365): always remembers per-file limits across stop and resume, with no checkbox. The per-file cap (337) is not a default for every file (`progress.rs:244`).
- Advanced proxy (369): SOCKS never covers ftp://, and "HTTP via proxy, HTTPS via SOCKS" can't be set up (`net.rs:211-225`, `engine/ftp.rs:36`).
- Progress dialog start view (367): on/off only, no choice of view.

Batch and grabber
- Batch download (280): bracket ranges instead of `*` with "Replace asterisk to". Width comes from leading zeros only (`batch.rs:1-74`).
- Grabber Add Filter (240) replaces the Keep types instead of adding to them (`tools.rs:1525`).
- Masks (245) shrink to extensions, so `photo_*.jpg` loses its name part (`grab.rs:929-940`).
- Login page URL (257): only browser cookies for the start site.
- Relative subfolders and overwrite (259) work in mirror mode only (`grab.rs:142-182`).
- Advanced page processing (262) is merged into one `render_scripts` setting.
- Project type (284): no one-time vs periodic choice. The stop time ends the crawl but not the queue it started (`uidm-daemon.rs:1770-1776`).
- Open-when-done file (284, 306): a text field with no browse button.
- Statistics (271): no downloaded count while a crawl runs.
- Grabber review: no Stop for the checked files' downloads (string 1433).

Browser extension
- Per-tab integration toggle on the toolbar icon with a grey icon and "X" badge. UIDM has only a global switch (`popup.js:172`).
- Capture point: UIDM cancels in `downloads.onCreated`, not at response headers, and leaves the blank download tab open (`background.js:598-660`).
- Request context: IDM sends all request and response headers, the redirect chain, the server IP, size and MIME. UIDM sends URL, cookies, UA, referrer, file name, POST body and proxy (`background.js:575-596`).
- Referer walks past `about:blank` frames in IDM (`background.js:653`).
- Multipart and XHR POST bodies are not rebuilt (`post.js:44-62`).
- Proxy forwarding: one fixed proxy only, no PAC, auto, system or bypass (`background.js:543-573`).
- Modifier keys act only on `<a>` clicks, not on forms or script downloads started while the key is held (`content.js:46-90`).
- Private windows: UIDM always skips them and IDM captures there. No opt-in (`background.js:47-108`).
- All/selected links skip iframe src, URLs in scripts, textarea selections and images inside a selection (`background.js:670-691`).
- The panel hides only on size or off-screen, with no occlusion, minimize or fullscreen check (`content.js:251-263`).
- Site-specific handling: IDM has modes for YouTube, Facebook, Instagram and Vimeo. UIDM records only YouTube pages (`media.js:115-131`).
- Video file names: the top-frame title and og:title are not used, and the " - site" suffix is stripped for YouTube only (`media.js:136-139`).
- SPA navigation resets media only on YouTube URL changes (`background.js:97-114`).
- App-pushed notifications: only "could not take download" (`background.js:467-485`).

## Undecided

- 195 Sounds: IDM fills the event list at runtime, so the resources don't show it. UIDM has three events.
- 271 Statistics "Simple / Advanced" columns: the resources don't say what they split.
- 292 "Rebooting in 15 seconds": UIDM has shutdown, sleep and hibernate, but no reboot action.
- 364 "Save To:" on the completion tab: it has no edit control and may be a leftover.
- String 34025 "Update all": its purpose isn't clear from the resources.
- IDMMsgHost.exe internals, the per-site rule letters `GPTSDVACRHX`, and IDM's video format list. The native app supplies these, and they were not disassembled.
