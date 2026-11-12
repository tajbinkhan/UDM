# Draw the window with Tauri

Status: accepted, and done: the Tauri window replaced the egui one, which is gone. Supersedes [ADR 0001](0001-cloak-the-main-window-in-the-tray.md).

The egui window works, but it has hit limits egui cannot get past:

- **Text.** egui lays text out one character after another, so right-to-left languages and Indic scripts cannot be shaped (`docs/known-gaps.md`). A webview shapes them.
- **Fonts.** Chinese, Japanese, Korean and Vietnamese need fonts found and checked by hand (`SystemServices`). A webview falls back to the system's fonts.
- **Windows.** Every dialog is drawn in the main window's frame, so the main window has to stay "visible" while it is in the tray (ADR 0001), and Linux closes to the tray by starting a new copy.
- **Accessibility.** egui's AccessKit tree is thin. HTML elements carry their roles and names to screen readers.

We move the window to Tauri 2: a Rust shell owns the state, the windows and the tray, and a Svelte and TypeScript frontend draws it in the system's webview. The daemon, the host, `udm-cli`, `udm-mcp`, `domain`, `application` and `infrastructure` do not change. AGENTS.md section 7 holds the rules.

## Decisions

- **Rust owns the state.** One thread owns `AppState` and changes it with `apply`, as the egui window does now, so the existing tests and `udm_domain::table` keep the rules. The frontend draws projections and sends commands. A frontend store that owned the state would move rules into TypeScript, where Rust tests cannot reach them.
- **Projections over one channel per window.** A window subscribes once and gets a snapshot, then numbered patches, on a `tauri::ipc::Channel`. Tauri's docs recommend channels for ordered, fast data; its events are not ordered and carry JSON only, which does not suit the download list's progress.
- **Generated bindings.** `tauri-specta` writes the TypeScript for commands, contract types and events, so the two halves cannot drift. A test fails when the committed file differs. It is a release candidate (2.0.0-rc), so it is pinned to one version; `ts-rs` with hand-written `invoke` wrappers is the fallback.
- **A contract module.** Types that cross to the webview live in `crates/ui/src/contract/` and are mapped from the application's types, as `wire` maps the daemon's protocol. `domain` and `application` stay free of `specta`.
- **One webview window per dialog.** The shell opens and closes them by label. The main window hides normally, so ADR 0001's cloaking goes once the egui window does.
- **Least privilege.** A capability file per window kind, app commands restricted in `build.rs`, a strict Content Security Policy, no navigation away from the app, and no plugins that reach the disk, network or processes. That work stays behind UDM's ports.
- **Brownfield, not isolation.** Tauri recommends its isolation pattern where it can be used. UDM's windows load only bundled code, every command is already limited per window and parses its input into domain types, and the isolation iframe cannot load ES modules on Windows. We revisit this if a window ever shows remote content.
- **Svelte 5, TypeScript, Vite, pnpm.** A small runtime suits a download manager that sits in the tray all day. The same stack runs in GitButler, a large Tauri app.
- **UDM's installers, not Tauri's bundler.** `installer/` already builds the Windows, macOS and Linux packages; the Windows installer gains the WebView2 bootstrapper.
- **Side by side, then replace.** The Tauri shell was built as `crates/ui-tauri` (`udm-gui-next`) beside the egui window until it did what that window did; it then took `crates/ui` and `udm-gui` (AGENTS.md section 16 lists what did not come over).

## Considered options

- **Keep egui.** No rewrite, but the text, font and window limits above stay.
- **egui with a viewport per dialog.** Fixes the windows (ADR 0001's rejected option) but not text shaping or accessibility.
- **Slint or Iced.** Native Rust UIs without a webview and with less memory, but with the same shaping and accessibility gaps as egui, or less mature support for them on all three systems.
- **Electron.** A bundled Chromium per install, far heavier than the system webview, and a Node process beside UDM's Rust programs.

## Consequences

- About 22,000 lines of egui code are rewritten, window by window.
- The window uses more memory than egui, most of all with WebKitGTK on Linux. It is measured with `tools/bench/` before the egui window goes.
- Each system draws with its own webview (WebView2, WKWebView, WebKitGTK), so every window is checked against IDM on each system, not only on Windows.
- Building needs Node and pnpm, and on Linux WebKitGTK 4.1 and its development files. CI installs them, and the frontend is built before Cargo runs.
- The quality gates gain `svelte-check`, ESLint and Vitest.
- To confirm in the first spike: whether the shell builds for `x86_64-pc-windows-gnu` as the egui window does, whether the Content Security Policy holds with Svelte's styles, whether Linux can hide the main window in the tray without starting a new copy, and the window's memory use against egui's.

## Sources

- Tauri: [calling the frontend](https://v2.tauri.app/develop/calling-frontend/) (events and channels), [calling Rust](https://v2.tauri.app/develop/calling-rust/) (commands), [state management](https://v2.tauri.app/develop/state-management/), [capabilities](https://v2.tauri.app/security/capabilities/), [security](https://v2.tauri.app/security/), [isolation pattern](https://v2.tauri.app/concept/inter-process-communication/isolation/), [mocking](https://v2.tauri.app/develop/tests/mocking/), [WebDriver](https://v2.tauri.app/develop/tests/webdriver/), [`tauri::test`](https://docs.rs/tauri/latest/tauri/test/index.html).
- [tauri-specta](https://github.com/specta-rs/tauri-specta).
- Larger Tauri apps: [Tabby's architecture](https://markbrutx.github.io/Tabby/architecture/) (ports and adapters behind thin commands, generated bindings, projections), [GitButler's development guide](https://github.com/gitbutlerapp/gitbutler/blob/master/DEVELOPMENT.md) (Rust for everything that touches the disk, Svelte for what the user sees).
