# Benchmark: Pullbay against IDM

> **No IDM results yet.** The harness, the fault server and a Pullbay-only sample run are here;
> the comparison with IDM needs a Windows machine with IDM installed, and none has been run.
> There is no chart here until there is a run to draw it from. The one sample below is Pullbay
> alone on Linux, and it compares Pullbay with nothing.

The harness downloads the same file several times with each download manager, from a local
server that can misbehave on purpose, and records how long each download took, how much CPU it
used and whether the file came out byte-identical. Every manager runs at its own default
settings.

- `tools/bench/bench.py`: the harness.
- `tools/bench/faultserver.py`: the server, also usable on its own.
- `docs/bench/results/`: one folder per run of the harness, with its raw data.

Both are Python 3.9 or later, standard library only, and run on Windows, macOS and Linux. They
are developer tools: they are not part of the quality gates and are not shipped in any package.

## What it measures

For each scenario, each tool downloads the file `--runs` times (5 by default), after one
unrecorded warm-up run (`--warmup`). The tools take turns, and a different one goes first in
each round. Each run gets a fresh, empty folder and an address of its own (`?run=<random>`), so
no tool takes it for a download it already has.

| Column in `runs.csv` | What it is |
|---|---|
| `seconds` | Wall time from starting the tool's command to having the whole file. For a tool that exits when it is done, until it exits. For IDM, until the whole file is in the folder and IDM has let go of it (checked every 20 ms) |
| `cpu_seconds` | User plus system CPU time during the run: the tool's own process; for Pullbay, `pullbay-cli` plus the daemon; for IDM, `IDMan.exe` plus the `/d` call |
| `bytes`, `ok` | The file's size, and whether it matched the server's SHA-256 |
| `exit_code`, `error` | What the tool returned, and why a run counts as failed |
| `server_*` | What the server saw: connections opened, requests, the most connections from the client at once, connections refused, and body bytes sent (more than the file means bytes sent twice, or sent and thrown away) |

`summary.md` gives the median, minimum and maximum of the runs that got the whole file, and
`chart.svg` draws the medians. `bench.py report FOLDER` makes both again from `runs.csv`.

## The server and the scenarios

`faultserver.py` serves one generated file (byte *i* is *i* mod 251, so a byte at the wrong
place changes the hash) on 127.0.0.1, with HTTP/1.1 keep-alive, ranges, an ETag and a
Last-Modified date. It can inject the same faults as the engine's test server
(`crates/infrastructure/src/engine/testserver.rs`), which the engine's own tests use:

| Fault | Option | |
|---|---|---|
| Dropped connections | `--drop-after SIZE` | Each response's connection is closed after that many bytes of its body |
| Connections per client | `--max-per-client N` | At most N connections from one address are answered at once; the others get 503 and are closed |
| Slow starts | `--slow-start SECONDS` | Every answer waits that long |
| Wrong ranges | `--shifted-ranges N`, `--ignored-ranges N` | The first N range requests that start past byte 0 get a range 1 KiB earlier; the next N get 200 and the whole file |
| Speed per connection | `--rate SIZE` | Each connection gets at most that many bytes a second |
| Speed in all | `--total-rate SIZE` | All connections together get at most that many bytes a second |
| No ranges | `--no-ranges` | Range requests get the whole file, as from a server that cannot resume |

The harness runs these scenarios (`bench.py list` shows them, `--scenarios` picks some):

| Scenario | The server |
|---|---|
| `clean` | Well-behaved and as fast as loopback goes. This mostly measures each tool's fixed costs and the Python server, not a network |
| `per-connection-limit` | Each connection held to 4 MiB/s, as many real servers do. Tools that open more connections finish sooner |
| `slow-link` | All connections together held to 16 MiB/s, like a slow line: more connections should not help |
| `drops` | Each connection dropped after 4 MiB; 8 MiB/s each. A tool must resume to finish |
| `two-per-client` | Two connections at once, 503 for the rest; 4 MiB/s each |
| `slow-start` | One second before every answer; 4 MiB/s each |
| `wrong-ranges` | Two range answers 1 KiB off and one with the whole file; 4 MiB/s each. A tool must not write those bytes |
| `no-ranges` | No range support; 16 MiB/s. One connection, no resume |

To run the server alone, for a tool the harness does not know or to watch one by hand:

```sh
python3 tools/bench/faultserver.py --size 256M --rate 4M --max-per-client 2
```

It prints the file's address, and its counters are at `/_bench/stats`. `--host` makes it listen
on another address, to test from another machine on a network you trust.

## The tools

| Name | What runs | Notes |
|---|---|---|
| `pullbay` | `pullbay-cli get -q -d FOLDER -n NAME URL` | A separate copy (`PULLBAY_INSTANCE=bench`) with its own daemon, which the harness starts and stops. On Windows and Linux its settings and list go in a temporary folder; on macOS in `~/Library/Application Support/Pullbay-bench`. Default settings, except that completion notifications are off |
| `idm` | `IDMan.exe /d URL /p FOLDER /f NAME /n` | Windows only. IDM must be running before the run starts, so its start-up is not timed. Found through IDM's registry key, or `--idm PATH` |
| `aria2c` | `aria2c --quiet=true --dir=FOLDER --out=NAME URL` | Its defaults, which open one connection per server |
| `curl` | `curl --silent --show-error --fail --output PATH URL` | One connection, and it does not retry by default, so it fails `drops` |
| `wget` | `wget --quiet --output-document=PATH URL` | |

`--tool 'NAME=COMMAND'` adds any other program that downloads a file and exits; `{url}`,
`{folder}`, `{name}` and `{path}` in its command are filled in. `--pullbay-bin` points at the
folder with `pullbay-cli` and `pullbay-daemon`: `target/release` by default, or an installed copy
(`%LOCALAPPDATA%\Programs\Pullbay` on Windows).

## Running the IDM comparison (Windows)

1. Install Python 3.9 or later, and IDM. Build Pullbay with
   `cargo build --release -p pullbay-app --bin pullbay-cli --bin pullbay-daemon`, or use an
   installed copy with `--pullbay-bin`.
2. Leave IDM's options as a fresh install has them. Write down IDM's version and the number of
   connections its Options show. If IDM shows a dialog each time a download completes, turn that
   dialog off (one per run gets in the way) and say so in the note.
3. Start IDM. Close browsers and other programs that use the network or the disk, plug in a
   laptop, and keep the download folder and the system on the same kind of disk between runs
   you compare. Windows Defender scans new files; note whether its real-time protection was on.
4. Run, from the repository:

   ```bat
   py tools\bench\bench.py list
   py tools\bench\bench.py run --tools pullbay,idm,aria2c --runs 5 ^
     --note "IDM 6.xx build yy, defaults, 8 connections, completion dialog off; Defender on; NVMe SSD"
   ```

   A full run of all eight scenarios with a 64 MiB file takes about three minutes for a tool that
   opens several connections, and about eight for one that opens one. `--size` changes the file
   (`--size 256M`), and `--scenarios` and `--runs` shorten it.
5. Commit the new folder under `docs/bench/results/` (`runs.csv`, `machine.json`, `summary.md`
   and `chart.svg`, a few kilobytes), put its chart and a short reading of it in this README, and
   replace the note at the top.

On Linux and macOS the same command runs without `idm`; `aria2c`, `curl` and `wget` are optional.

## What to record about the machine

`machine.json` records, without host or user names: the system and its version, the CPU and its
logical cores, the RAM, Python's version, every tool's version (Pullbay's with the commit it was
built from, when it is this repository's build), the file's size and hash, the number of runs and
every scenario's faults. Put in `--note` what the harness cannot see: the disk (SSD or HDD, and
whether the download folder is on it), the power plan, antivirus scanning, IDM's settings and
anything else running at the time.

## Reading the results

- Every server is on loopback: there is no real network's latency or loss. The scenarios model
  how servers behave, not how a line between you and them behaves. `clean` is limited by the
  Python server and by each tool's fixed costs.
- The CPU time of a long-lived process (Pullbay's daemon, IDM) is all it did during the run,
  which includes anything else it was doing then.
- IDM's time includes up to 20 ms of polling and whatever IDM does before the finished file is in
  the folder; Pullbay's and the others' end when their command exits.
- A run that did not produce the whole, correct file counts as failed and is left out of the
  medians, and the table shows how many runs succeeded.

## Results

| Folder | Machine | Tools | What it is |
|---|---|---|---|
| [2026-11-10-linux-pullbay-only](results/2026-11-10-linux-pullbay-only/summary.md) | Linux, Ryzen 7 9800X3D | Pullbay only | A sample run that checks the harness and the faults work end to end. Not a comparison |

The sample (64 MiB, five runs each, release build of commit 7006e77) shows Pullbay at its
defaults against each scenario. Each run got a byte-identical file. The times follow the server's
limits: for example, `two-per-client` took 8.1 s, which is 64 MiB over two connections at 4 MiB/s
each, and the server never saw more than two at once. `per-connection-limit` took 2.1 s with 8
connections, against the 16 s one 4 MiB/s connection would take. It says nothing about IDM.

---

[← Back to the help pages](../README.md)
