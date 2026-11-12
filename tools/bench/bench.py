#!/usr/bin/env python3
"""Download the same file several times with UDM and other download
managers, against a local server that can inject faults, and record how long
each took and how much CPU it used.

    python3 tools/bench/bench.py list
    python3 tools/bench/bench.py run --tools udm,curl --runs 5
    python3 tools/bench/bench.py report docs/bench/results/<run folder>

Every tool runs at its own default settings. `run` writes the raw data
(`runs.csv`), the machine and versions (`machine.json`), a summary
(`summary.md`) and a chart (`chart.svg`) to a new folder under
docs/bench/results/; `report` makes the summary and chart again from a
folder's `runs.csv`. See docs/bench/README.md. Python's standard library only.
"""

from __future__ import annotations

import argparse
import csv
import ctypes
import datetime as dt
import hashlib
import json
import os
import platform
import shlex
import shutil
import statistics
import subprocess
import sys
import tempfile
import threading
import time
import uuid
from dataclasses import asdict, dataclass
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.dont_write_bytecode = True
import faultserver  # noqa: E402
from faultserver import Faults, parse_size  # noqa: E402

REPO = Path(__file__).resolve().parents[2]
WINDOWS = os.name == "nt"
MACOS = sys.platform == "darwin"
MIB = 1024 * 1024

# Each scenario is a server the tools download from. Speeds are per connection
# unless the name says otherwise, so a scenario rewards opening more than one
# connection only where real servers would.
SCENARIOS: dict[str, tuple[str, Faults]] = {
    "clean": ("A well-behaved server, as fast as loopback goes", Faults()),
    "per-connection-limit": ("Each connection held to 4 MiB/s", Faults(rate=4 * MIB)),
    "slow-link": ("All connections together held to 16 MiB/s", Faults(total_rate=16 * MIB)),
    "drops": ("Each connection dropped after 4 MiB; 8 MiB/s each", Faults(drop_after=4 * MIB, rate=8 * MIB)),
    "two-per-client": ("Two connections at once (503 for more); 4 MiB/s each", Faults(max_per_client=2, rate=4 * MIB)),
    "slow-start": ("1 s before every answer; 4 MiB/s each", Faults(slow_start=1.0, rate=4 * MIB)),
    "wrong-ranges": (
        "Two ranges answered 1 KiB off, one with the whole file; 4 MiB/s each",
        Faults(shifted_ranges=2, ignored_ranges=1, rate=4 * MIB),
    ),
    "no-ranges": ("No range support (one connection, no resume); 16 MiB/s", Faults(no_ranges=True, rate=16 * MIB)),
}

CSV_FIELDS = [
    "scenario",
    "tool",
    "run",
    "seconds",
    "cpu_seconds",
    "bytes",
    "ok",
    "exit_code",
    "server_connections",
    "server_requests",
    "server_peak_connections",
    "server_refused",
    "server_bytes_sent",
    "error",
]


# ---------------------------------------------------------------- measuring


@dataclass
class Ran:
    """One program run: its exit code, wall time and CPU time in seconds."""

    code: int | None
    seconds: float
    cpu: float
    out: str
    err: str


class FILETIME(ctypes.Structure):
    _fields_ = [("low", ctypes.c_uint32), ("high", ctypes.c_uint32)]


def _filetime_seconds(ft: FILETIME) -> float:
    return ((ft.high << 32) | ft.low) / 1e7


def _kernel32():
    """kernel32 with the argument types the calls below need: handles are
    pointer-sized, which ctypes' default int would cut on 64-bit Windows."""
    kernel32 = ctypes.windll.kernel32  # type: ignore[attr-defined]
    times = ctypes.POINTER(FILETIME)
    kernel32.GetProcessTimes.argtypes = [ctypes.c_void_p, times, times, times, times]
    kernel32.GetProcessTimes.restype = ctypes.c_int
    kernel32.OpenProcess.argtypes = [ctypes.c_uint32, ctypes.c_int, ctypes.c_uint32]
    kernel32.OpenProcess.restype = ctypes.c_void_p
    kernel32.CloseHandle.argtypes = [ctypes.c_void_p]
    return kernel32


def _windows_times(handle) -> float:
    """User plus kernel CPU seconds of a process handle."""
    kernel32 = _kernel32()
    c, e, k, u = FILETIME(), FILETIME(), FILETIME(), FILETIME()
    if not kernel32.GetProcessTimes(handle, ctypes.byref(c), ctypes.byref(e), ctypes.byref(k), ctypes.byref(u)):
        return float("nan")
    return _filetime_seconds(k) + _filetime_seconds(u)


def process_cpu(pid: int) -> float:
    """CPU seconds a running process has used so far."""
    if WINDOWS:
        kernel32 = _kernel32()
        handle = kernel32.OpenProcess(0x1000, False, pid)  # PROCESS_QUERY_LIMITED_INFORMATION
        if not handle:
            return float("nan")
        try:
            return _windows_times(handle)
        finally:
            kernel32.CloseHandle(handle)
    stat = Path(f"/proc/{pid}/stat")
    if stat.exists():
        fields = stat.read_text().rsplit(")", 1)[1].split()
        return (int(fields[11]) + int(fields[12])) / os.sysconf("SC_CLK_TCK")
    # macOS and the BSDs: [[dd-]hh:]mm:ss.ss
    text = subprocess.run(["ps", "-o", "time=", "-p", str(pid)], capture_output=True, text=True).stdout.strip()
    if not text:
        return float("nan")
    days, _, clock = text.rpartition("-")
    seconds = 0.0
    for part in clock.split(":"):
        seconds = seconds * 60 + float(part)
    return seconds + (int(days) * 86400 if days else 0)


def run_program(cmd: list[str], timeout: float, env: dict | None = None, cwd: Path | None = None) -> Ran:
    """Run `cmd` to its end; its wall time and its own CPU time."""
    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        start = time.perf_counter()
        proc = subprocess.Popen(cmd, stdin=subprocess.DEVNULL, stdout=out, stderr=err, env=env, cwd=cwd)
        if WINDOWS:
            try:
                code = proc.wait(timeout)
            except subprocess.TimeoutExpired:
                proc.kill()
                code = None
                proc.wait()
            seconds = time.perf_counter() - start
            cpu = _windows_times(int(proc._handle))  # type: ignore[attr-defined]
        else:
            killer = threading.Timer(timeout, proc.kill)
            killer.start()
            try:
                _, status, usage = os.wait4(proc.pid, 0)
            finally:
                killer.cancel()
            seconds = time.perf_counter() - start
            cpu = usage.ru_utime + usage.ru_stime
            code = os.waitstatus_to_exitcode(status) if hasattr(os, "waitstatus_to_exitcode") else status >> 8
            proc.returncode = code
            if code < 0:
                code = None if seconds >= timeout else code
        out.seek(0)
        err.seek(0)
        return Ran(code, seconds, cpu, out.read().decode(errors="replace"), err.read().decode(errors="replace"))


def first_line(cmd: list[str], env: dict | None = None) -> str:
    try:
        done = subprocess.run(cmd, capture_output=True, text=True, timeout=20, env=env)
    except (OSError, subprocess.TimeoutExpired):
        return "unknown"
    text = (done.stdout or done.stderr).strip()
    return text.splitlines()[0] if text else "unknown"


def sha256_of(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(MIB), b""):
            h.update(block)
    return h.hexdigest()


def exe(name: str) -> str:
    return name + ".exe" if WINDOWS else name


# ---------------------------------------------------------------- the tools


@dataclass
class Got:
    """What one download gave: the run and where the file should be."""

    ran: Ran
    path: Path
    error: str = ""


class Tool:
    name = ""

    def missing(self) -> str | None:
        """Why the tool cannot run here, or None."""
        return None

    def version(self) -> str:
        return "unknown"

    def setup(self) -> None:
        pass

    def download(self, url: str, folder: Path, name: str, size: int, timeout: float) -> Got:
        raise NotImplementedError

    def teardown(self) -> None:
        pass


class Command(Tool):
    """A program that downloads the file and exits; `{url}`, `{folder}`,
    `{name}` and `{path}` in its arguments are filled in."""

    def __init__(self, name: str, args: list[str], version_args: list[str] | None = None):
        self.name = name
        self.args = args
        self.version_args = version_args

    def missing(self) -> str | None:
        return None if shutil.which(self.args[0]) else f"{self.args[0]} is not on the PATH"

    def version(self) -> str:
        return first_line(self.version_args) if self.version_args else "unknown"

    def download(self, url, folder, name, size, timeout):
        path = folder / name
        fill = {"url": url, "folder": str(folder), "name": name, "path": str(path)}
        cmd = [a.format(**fill) for a in self.args]
        return Got(run_program(cmd, timeout, cwd=folder), path)


class UDM(Tool):
    """`udm-cli get`, through a daemon of its own: a separate copy
    (UDM_INSTANCE=bench) with its settings and data in a scratch folder,
    so the installed copy's list and settings are left alone. Its CPU time
    is the tool's plus the daemon's while the download ran."""

    name = "udm"
    INSTANCE = "bench"

    def __init__(self, bin_dir: Path):
        self.cli = bin_dir / exe("udm-cli")
        self.daemon_exe = bin_dir / exe("udm-daemon")
        self.bin_dir = bin_dir
        self.daemon: subprocess.Popen | None = None
        self.scratch: Path | None = None
        self.env: dict = {}

    def missing(self):
        for p in (self.cli, self.daemon_exe):
            if not p.is_file():
                return f"{p} not found (cargo build --release, or pass --udm-bin)"
        return None

    def version(self):
        v = first_line([str(self.cli), "version"], self.env or None)
        commit = first_line(["git", "-C", str(REPO), "rev-parse", "--short", "HEAD"])
        return f"{v} (repository at {commit})" if self.bin_dir.is_relative_to(REPO) else v

    def portable_root(self) -> Path | None:
        """The folder of a portable copy, found as paths::portable_root does."""
        if (self.bin_dir / "portable.txt").is_file():
            return self.bin_dir
        if self.bin_dir.name == "release" and self.bin_dir.parent.name == "target":
            root = self.bin_dir.parent.parent
            if (root / "portable.txt").is_file():
                return root
        return None

    def folders(self) -> tuple[Path, Path]:
        """This copy's settings and data folders (paths::config_dir, data_dir)."""
        own = ("UDM" if WINDOWS or MACOS else "udm") + "-" + self.INSTANCE
        portable = self.portable_root()
        if portable:
            data = portable / f"Data-{self.INSTANCE}"
            return data, data
        if WINDOWS:
            return Path(self.env["APPDATA"]) / own, Path(self.env["LOCALAPPDATA"]) / own
        if MACOS:
            support = Path.home() / "Library" / "Application Support" / own
            return support, support
        return Path(self.env["XDG_CONFIG_HOME"]) / own, Path(self.env["XDG_DATA_HOME"]) / own

    def setup(self):
        self.scratch = Path(tempfile.mkdtemp(prefix="udm-bench-"))
        env = dict(os.environ)
        env["UDM_INSTANCE"] = self.INSTANCE
        if WINDOWS:
            env["APPDATA"] = str(self.scratch / "Roaming")
            env["LOCALAPPDATA"] = str(self.scratch / "Local")
        elif not MACOS:
            env["XDG_CONFIG_HOME"] = str(self.scratch / "config")
            env["XDG_DATA_HOME"] = str(self.scratch / "data")
        self.env = env
        config, data = self.folders()
        config.mkdir(parents=True, exist_ok=True)
        data.mkdir(parents=True, exist_ok=True)
        # The defaults, but no notification for each finished download: a
        # benchmark finishes hundreds.
        settings_file = config / "settings.json"
        try:
            settings = json.loads(settings_file.read_text())
        except (OSError, ValueError):
            settings = {}
        settings["notify"] = "off"
        settings_file.write_text(json.dumps(settings, indent=2))
        log = data / "udm-daemon.log"
        seen = log.stat().st_size if log.exists() else 0
        self.daemon = subprocess.Popen(
            [str(self.daemon_exe)],
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            if self.daemon.poll() is not None:
                raise SystemExit(
                    "udm-daemon exited at once: another copy named "
                    f"'{self.INSTANCE}' may be running (see {log})"
                )
            if log.exists():
                with open(log, "rb") as f:
                    f.seek(seen)
                    if b"daemon started" in f.read():
                        return
            time.sleep(0.1)
        raise SystemExit(f"udm-daemon did not start within 20 s (see {log})")

    def download(self, url, folder, name, size, timeout):
        assert self.daemon is not None
        before = process_cpu(self.daemon.pid)
        ran = run_program(
            [str(self.cli), "get", "-q", "-d", str(folder), "-n", name, url], timeout, env=self.env, cwd=folder
        )
        ran.cpu += process_cpu(self.daemon.pid) - before
        lines = [line for line in ran.out.splitlines() if line.strip()]
        path = Path(lines[-1]) if ran.code == 0 and lines else folder / name
        return Got(ran, path, "" if ran.code == 0 else ran.err.strip()[-300:])

    def teardown(self):
        if self.daemon and self.daemon.poll() is None:
            self.daemon.terminate()
            try:
                self.daemon.wait(10)
            except subprocess.TimeoutExpired:
                self.daemon.kill()
                self.daemon.wait()
        if self.scratch:
            shutil.rmtree(self.scratch, ignore_errors=True)


class Idm(Tool):
    """Internet Download Manager through its own command line
    (`IDMan.exe /d URL /p folder /f name /n`). That call hands the address to
    the IDM already running and returns, so the download counts as done when
    the whole file is in the folder and IDM has let go of it. Its CPU time is
    IDMan.exe's while the download ran, plus the call's."""

    name = "idm"
    POLL = 0.02

    def __init__(self, path: str | None):
        self.exe = Path(path) if path else self.find()

    @staticmethod
    def find() -> Path | None:
        if not WINDOWS:
            return None
        try:
            import winreg

            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Software\DownloadManager") as key:
                found = Path(winreg.QueryValueEx(key, "ExePath")[0])
                if found.is_file():
                    return found
        except OSError:
            pass
        for base in (os.environ.get("ProgramFiles(x86)"), os.environ.get("ProgramFiles")):
            if base and (Path(base) / "Internet Download Manager" / "IDMan.exe").is_file():
                return Path(base) / "Internet Download Manager" / "IDMan.exe"
        return None

    @staticmethod
    def pid() -> int | None:
        out = subprocess.run(
            ["tasklist", "/FI", "IMAGENAME eq IDMan.exe", "/FO", "CSV", "/NH"], capture_output=True, text=True
        ).stdout
        for row in csv.reader(out.splitlines()):
            if len(row) > 1 and row[0].lower() == "idman.exe":
                return int(row[1])
        return None

    def missing(self):
        if not WINDOWS:
            return "IDM runs on Windows only"
        if not self.exe or not self.exe.is_file():
            return "IDMan.exe not found (pass --idm PATH)"
        if self.pid() is None:
            return "IDM is not running: start it first, so its start-up is not timed"
        return None

    def version(self):
        assert self.exe is not None
        return first_line(
            ["powershell", "-NoProfile", "-Command", f"(Get-Item '{self.exe}').VersionInfo.ProductVersion"]
        )

    def download(self, url, folder, name, size, timeout):
        assert self.exe is not None
        pid = self.pid()
        if pid is None:
            return Got(Ran(None, 0.0, float("nan"), "", ""), folder / name, "IDM is not running")
        before = process_cpu(pid)
        start = time.perf_counter()
        call = run_program([str(self.exe), "/d", url, "/p", str(folder), "/f", name, "/n"], 60)
        path = folder / name
        done = False
        while time.perf_counter() - start < timeout:
            if path.is_file() and path.stat().st_size == size and self.released(path):
                done = True
                break
            time.sleep(self.POLL)
        seconds = time.perf_counter() - start
        cpu = process_cpu(pid) - before + call.cpu
        ran = Ran(0 if done else None, seconds, cpu, call.out, call.err)
        return Got(ran, path, "" if done else f"no whole file after {timeout:.0f} s")

    @staticmethod
    def released(path: Path) -> bool:
        """Whether no program still holds the file for writing."""
        try:
            with open(path, "r+b"):
                return True
        except OSError:
            return False


def make_tools(args) -> dict[str, Tool]:
    tools: dict[str, Tool] = {
        "udm": UDM(Path(args.udm_bin).resolve()),
        "idm": Idm(args.idm),
        "aria2c": Command(
            "aria2c", ["aria2c", "--quiet=true", "--dir={folder}", "--out={name}", "{url}"], ["aria2c", "--version"]
        ),
        "curl": Command(
            "curl", ["curl", "--silent", "--show-error", "--fail", "--output", "{path}", "{url}"], ["curl", "--version"]
        ),
        "wget": Command("wget", ["wget", "--quiet", "--output-document={path}", "{url}"], ["wget", "--version"]),
    }
    for spec in args.tool or []:
        name, _, command = spec.partition("=")
        if not name or not command:
            raise SystemExit(f"--tool takes NAME=COMMAND, not {spec!r}")
        parts = shlex.split(command, posix=not WINDOWS)
        tools[name] = Command(name, parts)
    return tools


# ---------------------------------------------------------------- the machine


def machine(tools: list[Tool], args, size: int, scenarios: list[str]) -> dict:
    """What the runs ran on. No host or user names."""
    cpu, ram = platform.processor() or "unknown", None
    try:
        if WINDOWS:
            import winreg

            with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, r"HARDWARE\DESCRIPTION\System\CentralProcessor\0") as key:
                cpu = winreg.QueryValueEx(key, "ProcessorNameString")[0].strip()

            class MEMORYSTATUSEX(ctypes.Structure):
                _fields_ = [("dwLength", ctypes.c_ulong), ("dwMemoryLoad", ctypes.c_ulong)] + [
                    (n, ctypes.c_ulonglong)
                    for n in (
                        "ullTotalPhys",
                        "ullAvailPhys",
                        "ullTotalPageFile",
                        "ullAvailPageFile",
                        "ullTotalVirtual",
                        "ullAvailVirtual",
                        "ullAvailExtendedVirtual",
                    )
                ]

            status = MEMORYSTATUSEX()
            status.dwLength = ctypes.sizeof(status)
            ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(status))  # type: ignore[attr-defined]
            ram = status.ullTotalPhys
        elif MACOS:
            cpu = first_line(["sysctl", "-n", "machdep.cpu.brand_string"])
            ram = int(first_line(["sysctl", "-n", "hw.memsize"]))
        else:
            for line in Path("/proc/cpuinfo").read_text().splitlines():
                if line.startswith("model name"):
                    cpu = line.split(":", 1)[1].strip()
                    break
            for line in Path("/proc/meminfo").read_text().splitlines():
                if line.startswith("MemTotal:"):
                    ram = int(line.split()[1]) * 1024
    except (OSError, ValueError):
        pass
    return {
        "started_utc": dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "os": platform.platform(),
        "cpu": cpu,
        "logical_cpus": os.cpu_count(),
        "ram_bytes": ram,
        "python": platform.python_version(),
        "file_bytes": size,
        "runs": args.runs,
        "warmup": args.warmup,
        "server": "tools/bench/faultserver.py on 127.0.0.1",
        "scenarios": {s: {"about": SCENARIOS[s][0], "faults": asdict(SCENARIOS[s][1])} for s in scenarios},
        "tools": {t.name: t.version() for t in tools},
        "note": args.note or "",
    }


# ---------------------------------------------------------------- running


def run(args) -> int:
    size = args.size
    tools_all = make_tools(args)
    names = [n.strip() for n in args.tools.split(",") if n.strip()]
    scenarios = [s.strip() for s in args.scenarios.split(",") if s.strip()] if args.scenarios else list(SCENARIOS)
    for s in scenarios:
        if s not in SCENARIOS:
            raise SystemExit(f"no scenario {s!r}; `bench.py list` shows them")
    tools: list[Tool] = []
    for n in names:
        tool = tools_all.get(n)
        if tool is None:
            raise SystemExit(f"no tool {n!r}; `bench.py list` shows them")
        why = tool.missing()
        if why:
            raise SystemExit(f"{n}: {why}")
        tools.append(tool)

    work = Path(args.work) if args.work else Path(tempfile.mkdtemp(prefix="bench-downloads-"))
    work.mkdir(parents=True, exist_ok=True)
    free = shutil.disk_usage(work).free
    if free < size * 3:
        raise SystemExit(f"{work} has {free // MIB} MiB free; the runs need about {3 * size // MIB} MiB")

    stamp = dt.datetime.now().strftime("%Y%m%d-%H%M%S")
    out = Path(args.out) if args.out else REPO / "docs" / "bench" / "results" / f"{stamp}-{platform.system().lower()}"
    out.mkdir(parents=True, exist_ok=False)

    payload = faultserver.pattern(size)
    expected = hashlib.sha256(payload).hexdigest()
    server = faultserver.start(payload, Faults())
    try:
        for t in tools:
            t.setup()
        # After the set-up, so UDM's version comes from the bench copy.
        info = machine(tools, args, size, scenarios)
        info["file_sha256"] = expected
        (out / "machine.json").write_text(json.dumps(info, indent=2) + "\n")
        log(f"results go to {out}")
        with open(out / "runs.csv", "w", newline="") as f:
            writer = csv.DictWriter(f, fieldnames=CSV_FIELDS, lineterminator="\n")
            writer.writeheader()
            for scenario in scenarios:
                faults = SCENARIOS[scenario][1]
                for n in range(args.warmup + args.runs):
                    warm = n < args.warmup
                    # A different tool goes first in each round.
                    order = tools[n % len(tools) :] + tools[: n % len(tools)]
                    for tool in order:
                        settle(server)
                        server.set_faults(faults)
                        folder = work / f"{scenario}-{tool.name}-{n}"
                        folder.mkdir(parents=True, exist_ok=True)
                        # A new address each time, so no tool takes it for one
                        # it already has.
                        url = f"{server.url}?run={uuid.uuid4().hex[:12]}"
                        got = tool.download(url, folder, "file.bin", size, args.timeout)
                        seen = server.stats.snapshot()
                        ok, error = check(got, size, expected)
                        row = {
                            "scenario": scenario,
                            "tool": tool.name,
                            "run": "warmup" if warm else n - args.warmup + 1,
                            "seconds": f"{got.ran.seconds:.3f}",
                            "cpu_seconds": f"{got.ran.cpu:.3f}",
                            "bytes": got.path.stat().st_size if got.path.is_file() else 0,
                            "ok": int(ok),
                            "exit_code": "" if got.ran.code is None else got.ran.code,
                            "server_connections": seen["connections"],
                            "server_requests": seen["requests"],
                            "server_peak_connections": seen["peak_connections"],
                            "server_refused": seen["refused"],
                            "server_bytes_sent": seen["bytes_sent"],
                            "error": error,
                        }
                        shutil.rmtree(folder, ignore_errors=True)
                        state = "ok" if ok else f"FAILED ({error[:80]})"
                        log(f"{scenario:<22} {tool.name:<10} {row['run']!s:<7} {got.ran.seconds:8.2f} s  {state}")
                        if not warm:
                            writer.writerow(row)
                            f.flush()
    finally:
        for t in tools:
            t.teardown()
        server.shutdown()
        if not args.work:
            shutil.rmtree(work, ignore_errors=True)
    report_dir(out)
    log(f"done: {out}")
    return 0


def settle(server, wait: float = 3.0) -> None:
    """Give the last run's connections a moment to close."""
    deadline = time.monotonic() + wait
    while server.stats.open_now > 0 and time.monotonic() < deadline:
        time.sleep(0.05)


def check(got: Got, size: int, expected: str) -> tuple[bool, str]:
    if got.error:
        return False, got.error.replace("\n", " ")
    if got.ran.code != 0:
        return False, f"exit code {got.ran.code}: {got.ran.err.strip()[-200:]}".replace("\n", " ")
    if not got.path.is_file():
        return False, f"no file at {got.path}"
    if got.path.stat().st_size != size:
        return False, f"{got.path.stat().st_size} bytes, not {size}"
    if sha256_of(got.path) != expected:
        return False, "the bytes differ from the server's file"
    return True, ""


def log(text: str) -> None:
    print(text, flush=True)


# ---------------------------------------------------------------- reporting


def load_rows(folder: Path) -> list[dict]:
    with open(folder / "runs.csv", newline="") as f:
        return list(csv.DictReader(f))


def summarize(rows: list[dict]) -> list[dict]:
    """Per scenario and tool: the runs that got the whole file, and medians."""
    groups: dict[tuple[str, str], list[dict]] = {}
    for r in rows:
        groups.setdefault((r["scenario"], r["tool"]), []).append(r)
    out = []
    for (scenario, tool), rs in groups.items():
        good = [r for r in rs if r["ok"] == "1"]
        s = {"scenario": scenario, "tool": tool, "ok": len(good), "runs": len(rs)}
        if good:
            secs = [float(r["seconds"]) for r in good]
            cpus = [float(r["cpu_seconds"]) for r in good if r["cpu_seconds"] != "nan"]
            s.update(
                median=statistics.median(secs),
                low=min(secs),
                high=max(secs),
                cpu=statistics.median(cpus) if cpus else float("nan"),
                mib_s=int(good[0]["bytes"]) / MIB / statistics.median(secs),
                peak=statistics.median(int(r["server_peak_connections"]) for r in good),
                sent=statistics.median(int(r["server_bytes_sent"]) for r in good) / int(good[0]["bytes"]),
            )
        out.append(s)
    return out


def summary_markdown(info: dict, summary: list[dict]) -> str:
    lines = [
        "# Benchmark summary",
        "",
        f"- Started: {info.get('started_utc', '?')} on {info.get('os', '?')}",
        f"- CPU: {info.get('cpu', '?')}, {info.get('logical_cpus', '?')} logical; RAM: "
        + (f"{int(info['ram_bytes']) / 1024**3:.1f} GiB" if info.get("ram_bytes") else "?"),
        f"- File: {int(info.get('file_bytes', 0)) / MIB:.0f} MiB, {info.get('runs', '?')} runs per tool and scenario",
        "- Tools: " + "; ".join(f"{k} = {v}" for k, v in info.get("tools", {}).items()),
    ]
    if info.get("note"):
        lines.append(f"- Note: {info['note']}")
    lines += [
        "",
        "Median of the runs that got the whole, correct file. CPU is user plus system time. "
        "Connections is the most the server saw at once; sent is the bytes it sent over the file's size.",
        "",
        "| Scenario | Tool | OK | Median s | Min s | Max s | MiB/s | CPU s | Connections | Sent |",
        "|---|---|---|---|---|---|---|---|---|---|",
    ]
    for s in summary:
        if s["ok"]:
            lines.append(
                f"| {s['scenario']} | {s['tool']} | {s['ok']}/{s['runs']} | {s['median']:.2f} | {s['low']:.2f} | "
                f"{s['high']:.2f} | {s['mib_s']:.1f} | {s['cpu']:.2f} | {s['peak']:g} | {s['sent']:.2f}x |"
            )
        else:
            lines.append(f"| {s['scenario']} | {s['tool']} | 0/{s['runs']} | failed | | | | | | |")
    return "\n".join(lines) + "\n"


PALETTE = ["#2f6db5", "#d9822b", "#3a9a5b", "#8e5bb5", "#c2464f", "#5a6b7d"]


def chart_svg(summary: list[dict]) -> str:
    """Median time per scenario, one bar per tool; each scenario has its own
    scale, since their times differ by far."""
    tools = sorted({s["tool"] for s in summary}, key=lambda t: (t != "udm", t != "idm", t))
    scenarios = list(dict.fromkeys(s["scenario"] for s in summary))
    by = {(s["scenario"], s["tool"]): s for s in summary}
    width, left, right, bar, gap, head = 760, 120, 130, 18, 4, 28
    plot = width - left - right
    height = 40 + len(scenarios) * (head + len(tools) * (bar + gap) + 12)
    esc = lambda t: str(t).replace("&", "&amp;").replace("<", "&lt;")  # noqa: E731
    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
        f'viewBox="0 0 {width} {height}" font-family="system-ui, sans-serif" font-size="12">',
        f'<rect width="{width}" height="{height}" fill="#ffffff"/>',
        '<text x="16" y="24" font-size="14" font-weight="600" fill="#1d2430">'
        "Median seconds to download the file (shorter is better; each scenario has its own scale)</text>",
    ]
    y = 40
    for scenario in scenarios:
        parts.append(f'<text x="16" y="{y + 18}" font-weight="600" fill="#1d2430">{esc(scenario)}</text>')
        y += head
        top = max((by[(scenario, t)].get("median", 0) for t in tools if (scenario, t) in by), default=0) or 1
        for i, tool in enumerate(tools):
            s = by.get((scenario, tool))
            parts.append(f'<text x="{left - 8}" y="{y + 13}" text-anchor="end" fill="#3b4656">{esc(tool)}</text>')
            if s is None:
                label, w = "not run", 0
            elif not s["ok"]:
                label, w = "failed", 0
            else:
                w = max(1, plot * s["median"] / top)
                label = f"{s['median']:.2f} s" + ("" if s["ok"] == s["runs"] else f" ({s['ok']}/{s['runs']} ok)")
            color = PALETTE[i % len(PALETTE)]
            parts.append(f'<rect x="{left}" y="{y}" width="{w:.1f}" height="{bar}" rx="2" fill="{color}"/>')
            parts.append(f'<text x="{left + w + 6:.1f}" y="{y + 13}" fill="#1d2430">{esc(label)}</text>')
            y += bar + gap
        y += 12
    parts.append("</svg>")
    return "\n".join(parts) + "\n"


def report_dir(folder: Path) -> None:
    rows = load_rows(folder)
    info = json.loads((folder / "machine.json").read_text()) if (folder / "machine.json").exists() else {}
    summary = summarize(rows)
    (folder / "summary.md").write_text(summary_markdown(info, summary))
    (folder / "chart.svg").write_text(chart_svg(summary))


def report(args) -> int:
    report_dir(Path(args.folder))
    log(f"wrote {Path(args.folder) / 'summary.md'} and chart.svg")
    return 0


def list_all(args) -> int:
    log("Scenarios:")
    for name, (about, _) in SCENARIOS.items():
        log(f"  {name:<22} {about}")
    log("Tools:")
    for name, tool in make_tools(args).items():
        why = tool.missing()
        log(f"  {name:<22} {'ready' if why is None else why}")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0], formatter_class=argparse.RawTextHelpFormatter)
    sub = ap.add_subparsers(dest="command", required=True)

    def tool_options(p):
        p.add_argument(
            "--udm-bin",
            default=str(REPO / "target" / "release"),
            help="folder with udm-cli and udm-daemon (default: target/release)",
        )
        p.add_argument("--idm", help="IDMan.exe (default: found from IDM's registry key)")
        p.add_argument(
            "--tool",
            action="append",
            metavar="NAME=COMMAND",
            help="another program that downloads and exits; {url}, {folder}, {name}, {path} are filled in",
        )

    p = sub.add_parser("list", help="show the scenarios and which tools can run here")
    tool_options(p)
    p.set_defaults(func=list_all)

    p = sub.add_parser("run", help="run the benchmark")
    tool_options(p)
    p.add_argument("--tools", default="udm", help="comma-separated tools (default: udm)")
    p.add_argument("--scenarios", help="comma-separated scenarios (default: all)")
    p.add_argument("--size", type=parse_size, default=parse_size("64M"), help="file size (default 64M)")
    p.add_argument("--runs", type=int, default=5, help="runs per tool and scenario (default 5)")
    p.add_argument("--warmup", type=int, default=1, help="unrecorded runs first (default 1)")
    p.add_argument("--timeout", type=float, default=600, help="seconds one download may take (default 600)")
    p.add_argument("--out", help="results folder (default: docs/bench/results/<date>-<system>)")
    p.add_argument("--work", help="folder to download into (default: a temporary one)")
    p.add_argument("--note", help="free text kept in machine.json, such as the IDM settings used")
    p.set_defaults(func=run)

    p = sub.add_parser("report", help="make summary.md and chart.svg again from a results folder")
    p.add_argument("folder")
    p.set_defaults(func=report)

    args = ap.parse_args()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
