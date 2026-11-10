#!/usr/bin/env python3
"""A local HTTP server that serves one test file and can misbehave on purpose.

It offers download managers the faults the engine's own test server
(crates/infrastructure/src/engine/testserver.rs) gives the engine's tests:
dropped connections, a limit on connections per client, slow starts, wrong
ranges and speed limits. bench.py starts it for each scenario; it also runs on
its own:

    python3 tools/bench/faultserver.py --size 64M --rate 4M --max-per-client 2

prints the file's address and serves until Ctrl+C. It listens on 127.0.0.1
unless --host says otherwise, and serves nothing but the generated file (any
path) and its own counters at /_bench/stats.

The file's byte i is i % 251, so a byte written at the wrong offset changes
its SHA-256. Python's standard library only.
"""

from __future__ import annotations

import argparse
import hashlib
import http.server
import json
import re
import socket
import sys
import threading
import time
from dataclasses import asdict, dataclass

CHUNK = 64 * 1024
STATS_PATH = "/_bench/stats"
LAST_MODIFIED = "Mon, 01 Jan 2024 00:00:00 GMT"
ETAG = '"bench-v1"'


@dataclass
class Faults:
    """How the server misbehaves. Every field off is a well-behaved server."""

    # Close a response's connection after this many bytes of its body.
    drop_after: int | None = None
    # Answer at most this many connections from one client address at once;
    # the others get 503 Service Unavailable and are closed.
    max_per_client: int | None = None
    # Seconds to wait before answering each request.
    slow_start: float = 0.0
    # The first this many range requests that start past byte 0 get a 206 for
    # a range 1 KiB earlier than the one asked for.
    shifted_ranges: int = 0
    # The next this many such range requests get 200 and the whole file.
    ignored_ranges: int = 0
    # Send each connection at most this many bytes a second.
    rate: int | None = None
    # Send all connections together at most this many bytes a second, as a
    # slow link does.
    total_rate: int | None = None
    # Ignore Range headers altogether: no resuming, no splitting.
    no_ranges: bool = False


def pattern(size: int) -> memoryview:
    """The test file: byte i is i % 251."""
    block = bytes(range(251))
    return memoryview(block * (size // 251 + 1))[:size]


def parse_size(text: str) -> int:
    """`64M`, `1G`, `512K` or plain bytes (powers of 1024)."""
    m = re.fullmatch(r"\s*(\d+(?:\.\d+)?)\s*([KMG]?)i?B?\s*", text, re.IGNORECASE)
    if not m:
        raise argparse.ArgumentTypeError(f"not a size: {text!r}")
    scale = {"": 1, "K": 1024, "M": 1024**2, "G": 1024**3}[m.group(2).upper()]
    return int(float(m.group(1)) * scale)


class Bucket:
    """A shared speed limit: callers wait until their bytes fit."""

    def __init__(self, rate: int):
        self.rate = rate
        self.lock = threading.Lock()
        self.next_free = time.monotonic()

    def take(self, n: int) -> None:
        with self.lock:
            now = time.monotonic()
            start = max(now, self.next_free)
            self.next_free = start + n / self.rate
            wait = self.next_free - now
        if wait > 0:
            time.sleep(wait)


class Stats:
    """What the server saw, for the benchmark's raw data."""

    def __init__(self):
        self.lock = threading.Lock()
        # Connections of an earlier round (a client's idle keep-alive ones)
        # count neither toward this one's limit nor its peak.
        self.round = 0
        # Every connection open now, of any round.
        self.open_now = 0
        self.reset()

    def reset(self) -> None:
        with self.lock:
            self.round += 1
            self.connections = 0
            self.requests = 0
            self.range_requests = 0
            self.refused = 0
            self.bytes_sent = 0
            self.peak_open = 0
            self.past_zero_ranges = 0

    def snapshot(self) -> dict:
        with self.lock:
            return {
                "connections": self.connections,
                "requests": self.requests,
                "range_requests": self.range_requests,
                "refused": self.refused,
                "bytes_sent": self.bytes_sent,
                "peak_connections": self.peak_open,
            }


class FaultServer(http.server.ThreadingHTTPServer):
    daemon_threads = True
    allow_reuse_address = True
    # Many managers open 8 to 32 connections at once.
    request_queue_size = 128

    def __init__(self, host: str, port: int, payload: memoryview, faults: Faults):
        super().__init__((host, port), Handler)
        self.payload = payload
        self.faults = faults
        self.stats = Stats()
        # Connections being answered, by round and client address.
        self.open_by_client: dict[tuple[int, str], int] = {}
        self.bucket = Bucket(faults.total_rate) if faults.total_rate else None

    @property
    def url(self) -> str:
        host, port = self.server_address[:2]
        return f"http://{host}:{port}/file.bin"

    def set_faults(self, faults: Faults) -> None:
        self.faults = faults
        self.bucket = Bucket(faults.total_rate) if faults.total_rate else None
        self.stats.reset()


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    server: FaultServer

    # An idle keep-alive connection is closed after this many seconds.
    timeout = 30

    def log_message(self, format, *args):  # noqa: A002 - the base class's name
        pass

    # A connection counts against its client's limit from accept to close.
    def handle(self) -> None:
        srv = self.server
        with srv.stats.lock:
            client = (srv.stats.round, self.client_address[0])
            srv.stats.connections += 1
            n = srv.open_by_client.get(client, 0)
            limit = srv.faults.max_per_client
            refused = limit is not None and n >= limit
            if refused:
                srv.stats.refused += 1
            else:
                srv.open_by_client[client] = n + 1
                srv.stats.open_now += 1
                srv.stats.peak_open = max(srv.stats.peak_open, srv.open_by_client[client])
        if refused:
            self.refuse()
            return
        try:
            super().handle()
        finally:
            with srv.stats.lock:
                srv.open_by_client[client] -= 1
                srv.stats.open_now -= 1

    def refuse(self) -> None:
        """Read the request, then answer 503 and close, so the client sees
        the refusal rather than a reset connection."""
        try:
            self.raw_requestline = self.rfile.readline(65537)
            if self.raw_requestline and self.parse_request():
                self.send_response(503)
                self.send_header("Content-Length", "0")
                self.send_header("Retry-After", "1")
                self.send_header("Connection", "close")
                self.end_headers()
                self.wfile.flush()
        except OSError:
            pass
        self.close_connection = True

    def do_HEAD(self) -> None:
        self.answer(send_body=False)

    def do_GET(self) -> None:
        if self.path.split("?")[0] == STATS_PATH:
            body = json.dumps(self.server.stats.snapshot()).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        self.answer(send_body=True)

    def answer(self, send_body: bool) -> None:
        srv = self.server
        faults = srv.faults
        payload = srv.payload
        total = len(payload)
        with srv.stats.lock:
            srv.stats.requests += 1
        if faults.slow_start:
            time.sleep(faults.slow_start)

        asked = None if faults.no_ranges else parse_range(self.headers.get("Range"), total)
        if asked == "unsatisfiable":
            self.send_response(416)
            self.send_header("Content-Range", f"bytes */{total}")
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        start, end, status = 0, total - 1, 200
        if asked is not None:
            start, end = asked
            status = 206
            with srv.stats.lock:
                srv.stats.range_requests += 1
                wrong = None
                if start > 0:
                    n = srv.stats.past_zero_ranges
                    srv.stats.past_zero_ranges += 1
                    if n < faults.shifted_ranges:
                        wrong = "shifted"
                    elif n - faults.shifted_ranges < faults.ignored_ranges:
                        wrong = "whole"
            if wrong == "shifted":
                start = max(0, start - 1024)
            elif wrong == "whole":
                start, end, status = 0, total - 1, 200

        length = end - start + 1 if total else 0
        self.send_response(status)
        self.send_header("Content-Type", "application/octet-stream")
        self.send_header("Content-Length", str(length))
        self.send_header("Last-Modified", LAST_MODIFIED)
        self.send_header("ETag", ETAG)
        if not faults.no_ranges:
            self.send_header("Accept-Ranges", "bytes")
        if status == 206:
            self.send_header("Content-Range", f"bytes {start}-{end}/{total}")
        self.end_headers()
        if not send_body or length == 0:
            return

        cut = length if faults.drop_after is None else min(length, faults.drop_after)
        began = time.monotonic()
        sent = 0
        try:
            while sent < cut:
                n = min(CHUNK, cut - sent)
                if srv.bucket:
                    srv.bucket.take(n)
                self.wfile.write(payload[start + sent : start + sent + n])
                sent += n
                with srv.stats.lock:
                    srv.stats.bytes_sent += n
                if faults.rate:
                    ahead = began + sent / faults.rate - time.monotonic()
                    if ahead > 0:
                        time.sleep(ahead)
            self.wfile.flush()
        except OSError:
            # The client went away, as clients do when they have enough.
            self.close_connection = True
            return
        if cut < length:
            # A dropped connection: the body ends early.
            self.close_connection = True
            try:
                self.connection.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass


def parse_range(value: str | None, total: int):
    """(start, end) for one `bytes=` range, "unsatisfiable", or None for no
    usable Range header (which gets the whole file)."""
    if not value:
        return None
    m = re.fullmatch(r"\s*bytes\s*=\s*(\d*)\s*-\s*(\d*)\s*", value)
    if not m or (not m.group(1) and not m.group(2)):
        return None
    if m.group(1):
        start = int(m.group(1))
        end = int(m.group(2)) if m.group(2) else total - 1
    else:
        suffix = int(m.group(2))
        start, end = max(0, total - suffix), total - 1
    if start >= total:
        return "unsatisfiable"
    return start, min(end, total - 1)


def start(payload: memoryview, faults: Faults, host: str = "127.0.0.1", port: int = 0) -> FaultServer:
    """Serve `payload` on a background thread; stop with `shutdown()`."""
    server = FaultServer(host, port, payload, faults)
    threading.Thread(target=server.serve_forever, name="faultserver", daemon=True).start()
    return server


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--size", type=parse_size, default=parse_size("64M"), help="file size (default 64M)")
    ap.add_argument("--host", default="127.0.0.1", help="address to listen on (default 127.0.0.1)")
    ap.add_argument("--port", type=int, default=0, help="port (default: any free one)")
    ap.add_argument("--drop-after", type=parse_size, help="close each response after this many bytes")
    ap.add_argument("--max-per-client", type=int, help="connections answered at once per client")
    ap.add_argument("--slow-start", type=float, default=0.0, help="seconds before each answer")
    ap.add_argument("--shifted-ranges", type=int, default=0, help="range requests answered 1 KiB off")
    ap.add_argument("--ignored-ranges", type=int, default=0, help="range requests answered with the whole file")
    ap.add_argument("--rate", type=parse_size, help="bytes a second per connection")
    ap.add_argument("--total-rate", type=parse_size, help="bytes a second for all connections")
    ap.add_argument("--no-ranges", action="store_true", help="ignore Range headers")
    args = ap.parse_args()
    faults = Faults(
        drop_after=args.drop_after,
        max_per_client=args.max_per_client,
        slow_start=args.slow_start,
        shifted_ranges=args.shifted_ranges,
        ignored_ranges=args.ignored_ranges,
        rate=args.rate,
        total_rate=args.total_rate,
        no_ranges=args.no_ranges,
    )
    payload = pattern(args.size)
    server = start(payload, faults, args.host, args.port)
    # The program's output: where the file is, and what to expect of it.
    print(server.url, flush=True)
    print(f"size {args.size} sha256 {hashlib.sha256(payload).hexdigest()}", file=sys.stderr)
    print(f"faults {json.dumps(asdict(faults))}", file=sys.stderr)
    try:
        while True:
            time.sleep(3600)
    except KeyboardInterrupt:
        server.shutdown()
    return 0


if __name__ == "__main__":
    sys.exit(main())
