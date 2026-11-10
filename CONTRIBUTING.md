# Contributing to Pullbay

Thanks for helping. This page says how to build Pullbay, what a change has to pass, and where the
rules for the code are.

## Before you start

- Look through the [issues](https://github.com/tajbinkhan/pullbay/issues) first. For a bigger
  change, open one or comment on one, so the work is agreed before you write it.
- Read [AGENTS.md](AGENTS.md). It is the single source of truth for how code in Pullbay is
  written, by people and by AI agents alike. Its section 16 says where the code on `main` still
  differs from the rules.
- Found a security hole? Don't open an issue. Report it privately as [SECURITY.md](SECURITY.md)
  describes.

## Build

On macOS and Linux a stable Rust toolchain is enough, plus on Linux the GTK 3 and OpenSSL
development packages (`sudo apt install libgtk-3-dev libssl-dev` on Ubuntu). On Windows, Pullbay
builds with the `stable-x86_64-pc-windows-gnu` toolchain and MinGW-w64 (WinLibs) on `PATH`.

```bash
cargo build --release
```

This makes `pullbay-gui` (the window), `pullbay-daemon` (the engine) and `pullbay-host` (the
browser's relay) in `target/release/`. Keep the three together: the host and the window start the
daemon from their own folder.

To try your build beside an installed Pullbay, give it a name in `PULLBAY_INSTANCE`. That copy
keeps its settings and list in folders of its own and runs its daemon on a port of its own:

```bash
PULLBAY_INSTANCE=test ./target/debug/pullbay-gui
```

Packaging each system's files, the installer, the Windows toolchain and every module's place are in
[Building Pullbay](docs/building.md).

## Quality gates

A change is done when these pass. CI runs them on Windows, macOS and Linux for every pull request
(`.github/workflows/build.yml`).

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

If you changed the browser extension, run its tests too:

```bash
node --test extension/test/*.test.js
```

Clippy checks only the code for the system it runs on. Code under `cfg(windows)` or `cfg(unix)` is
checked on that runner, so say in the pull request which systems you could not build on.

## The layers

The Rust code is a Cargo workspace with one crate per layer of a hexagonal design:

| Crate | What goes there |
| --- | --- |
| `crates/domain` | Pure rules and the port traits. No I/O, threads, clock or logging. |
| `crates/application` | Use cases, one per file, that work through the ports. |
| `crates/infrastructure` | The adapters: the engine, files, settings, IPC, the operating system. |
| `crates/ui` | The egui window, its dialogs, the tray and the translations. |
| `crates/app` | The three programs, which wire the adapters into the use cases. |

Dependencies point inward only: `ui` never uses `infrastructure`, and `domain` uses no other
crate of ours. AGENTS.md has the full rules. The ones most often missed:

- No `unwrap()`, `expect()` or `panic!` outside tests, unless a comment says why it cannot fail.
- No `println!`, `eprintln!` or `dbg!`. Log with `tracing`.
- No `Result<_, String>`. Each layer has its own `thiserror` enum.
- Nothing slow inside the window's `update`: daemon calls, file reads and network work run on a
  worker thread.
- Every new use case comes with tests.
- Text the window shows goes through the translations (see below).

Pullbay copies how Internet Download Manager behaves. Learn that only by running IDM as a user
does. Don't decompile or unpack its files, and don't copy its text, icons or images.

## Adding a language

The window's strings are written in English in the code, and that English is each string's ID:
`tr("Save")`, or `trf!("{count} files", count = n)` for text with values.

- A language is one JSON file in `crates/ui/src/lang/`, mapping each English string to its
  translation, plus a line in `LANGUAGES` in `crates/ui/src/i18n.rs`. A missing string shows in
  English.
- The extension's text is in `extension/_locales/<code>/messages.json`, as Chrome expects.
- The tests check that every key is still used, keeps its `{placeholders}`, and that every
  extension locale has the same messages as `en`.

A new string in the window needs its German, Spanish and French translations, or a test fails.
egui draws text without shaping it, so scripts that need shaping, such as Bengali, Hindi or
Arabic, don't display correctly yet. More in
[Building Pullbay](docs/building.md#adding-a-language).

## Pull requests

- Branch from `main` and keep one change per pull request.
- Write commit subjects as a short imperative sentence, such as "Verify a download's checksum".
- Say what changed and why, link the issue (`Closes #123`), and list the checks you ran.
- If a change is something a user would notice, it goes in the next release's notes in
  `release-notes/`. The maintainer writes those when tagging, from
  [the template](release-notes/TEMPLATE.md).

By sending a pull request you agree that your work is released under the
[MIT License](LICENSE), like the rest of Pullbay.
