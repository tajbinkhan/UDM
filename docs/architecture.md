# How Pullbay fits together

The three programs, how they talk to each other, how the daemon checks who is talking to it, and how the download engine fetches a file.

## The three programs

```
browser extension --stdio--> pullbay-host --+
                                          +--loopback TCP--> pullbay-daemon --> engine
desktop window (pullbay-gui) ---------------+   (authenticated)   (owns downloads)
```

- **`pullbay-daemon`** owns the download engine and keeps a list of every
  download it has seen. It listens on `127.0.0.1:51515`; winning that bind is
  its single-instance lock. Replies to a request go to the client that asked;
  download events go to every client, so the window, the popup and any open
  browser all stay in step. It exits by itself after 60 seconds with no
  clients and no queued or running downloads. It also shows the
  notifications, plays the sounds and runs the virus scans, since it is the
  one process running whenever a download ends. Its log is
  `pullbay-daemon.log` in the data folder, beside the host's `pullbay-host.log`
  and the window's `pullbay-gui.log`. A log over 5 MB moves to `.old` when
  the daemon starts and then once an hour. Passwords in addresses are hidden
  as `***`. It keeps its files in two folders:

  | | Windows | macOS | Linux |
  | --- | --- | --- | --- |
  | settings folder | `%APPDATA%\Pullbay` | `~/Library/Application Support/Pullbay` | `~/.config/pullbay` |
  | data folder | `%LOCALAPPDATA%\Pullbay` | `~/Library/Application Support/Pullbay` | `~/.local/share/pullbay` |

  On Linux, `XDG_CONFIG_HOME` and `XDG_DATA_HOME` move them.
  - `settings.json` (settings folder): the settings. A missing file or field
    means the default.
  - `history.json` (data folder): the download list and the next id,
    written about a second after each change and again on exit. It never
    holds cookies or request headers.
  - `requests.bin` (data folder): the original requests of downloads that can
    still be retried, cookies included, encrypted for the current user (with
    Windows DPAPI on Windows). It is deleted when none is left.
- **`pullbay-host`** is what the browser launches. It connects to the daemon,
  starting it if nothing answers, then copies frames between the browser's
  stdin/stdout and the daemon without parsing them. It waits up to 10 seconds
  for a daemon it just started, so it gives up within about 15 seconds if no
  genuine daemon answers. When the browser closes the port, the host closes
  its side, delivers any reply still in flight for up to 2 seconds, and exits.
  The daemon is started with the host's stdio marked non-inheritable, so it
  never holds the browser's pipes open. `pullbay-host send` connects the same
  way to hand the daemon links from other programs.
- **`pullbay-gui`** is the desktop window: one more daemon client. It starts the
  daemon too if needed, asks for the full list on connect, and applies every
  event to the same `DownloadTable` (`pullbay_domain::table`) the daemon keeps.
- **The extension** keeps one native-messaging port open, reconnecting with
  backoff (1 s doubling to 30 s) if Pullbay goes away.

Every link uses the same framing: a 4-byte little-endian length, then that
many bytes of UTF-8 JSON. Between browser and host that is the browser's
native-messaging standard; Pullbay reuses it everywhere so the host can relay
frames untouched.

## Host-daemon authentication

Loopback keeps other machines out, but any local process can connect to the
port. Both ends therefore prove they hold a shared secret, and the secret never
crosses the socket.

At start the daemon writes a random 256-bit token to
`%LOCALAPPDATA%\Pullbay\daemon.token`, which the profile's default ACL limits to
the current user, SYSTEM and Administrators. It writes the token only after it
wins the bind, so a second daemon cannot overwrite the running one's token.
Each connection then runs:

1. client to daemon: `auth_hello { client_nonce }`
2. daemon to client: `auth_challenge { server_nonce, proof }`, where `proof` is
   `HMAC-SHA256(token, "pullbay-daemon", client_nonce, server_nonce)`
3. the client checks that proof, then sends `auth { proof }` with the
   `"pullbay-host"` label
4. the daemon checks it and replies `auth_ok`

Every field is length-prefixed before it is hashed, and the two labels keep one
role's proof from being replayed as the other's. A process squatting on the
port cannot answer step 2, so a client gives up before it sends a single
request. The squatter only ever sees a random nonce, never the token, cookies
or URLs. A client that sends nothing, sends the wrong frame or sends a frame
over 1 KiB is dropped, and each step times out after 5 seconds.

## The download engine

The engine talks HTTP/2 when the server offers it (through TLS ALPN, as IDM
does) and HTTP/1.1 otherwise. On HTTP/2 the segments of a download are
streams on one connection, and **Maximum connections per server** counts
them as it counts connections. The client is reqwest's async one on a small
shared runtime; the engine's threads block on each step, and a response that
sends nothing for 60 seconds fails its segment, which is retried.

Given an `add`, the engine:

1. Probes the URL with a one-byte range request. A `206` reply means the server
   supports ranges and reports the total size; anything else falls back to a
   single stream.
2. Splits the total into up to 8 contiguous byte ranges (one connection per
   ~256 KiB, capped at 16), and fetches them on parallel threads. Each thread
   opens its own file handle and writes at its own offset, so writes never race.
   A connection that finishes its range early takes over the second half of
   the range with the most left, as long as both halves keep at least 256 KiB.
   The connection that had it stops at the new end of its first half without a
   new request, so the download no longer ends at the pace of its slowest
   range. The split ranges are saved in the sidecar like the first ones.
3. Records progress in a `<file>.pullbaypart` JSON sidecar next to the output,
   flushed about every 1 MiB and again on cancel or failure. Re-running the
   download reads the sidecar and refetches only the missing bytes of each
   segment. The sidecar is checked against the server's ETag, Last-Modified and
   size, so stale bytes are never stitched onto a changed file, and it is
   deleted on success. Some servers report a new Last-Modified on every
   request; for them, **Options > General > Ignore file modification time
   changes when resuming** drops the date check, keeping the size and ETag
   checks.
   **Give finished files the date the server reports** sets a finished
   file's modified time to the server's Last-Modified date, and **Use the
   server's date for the file's creation time too** sets its creation time
   as well. Only Windows and macOS keep a creation time that can be set;
   on Linux that option is greyed out.
4. Retries a segment that fails with exponential backoff (400 ms doubling to a
   10 s ceiling, up to 5 times), resuming from that segment's own cursor.

The output filename is, in priority order: an explicit name on the request, the
server's `Content-Disposition` (including the RFC 5987
`filename*=UTF-8''...` form, percent-decoded), the URL path, then a generic
fallback. The file's folder is its category's, from that final name, unless
the save-as dialog chose one.

A new download never writes into an existing file unless that file is a
partial copy of the same download, which its `.pullbaypart` sidecar proves (same
size, and same ETag or Last-Modified when the server sends them). Otherwise it
is saved as `name (1).ext`, `name (2).ext`, and so on, or replaces the file
when Options says to overwrite. When Options says to ask, the engine stops
before writing and reports `name_taken`. The daemon holds the download as
`confirming` until the save-as dialog answers with rename or overwrite, or
gives it a new name when no window answers within 20 seconds. A form
download (POST) is renamed without asking, since asking would send the form
again. Two downloads never write the same file at once. A retry reuses the
file its earlier attempt wrote.

Before it writes anything, the engine checks that it may create files in the
folder; if not, the download fails with `Saving here needs administrator
rights: <folder>`. A request with `elevate` set is downloaded into a staging
folder instead, and the finished file is moved into its folder by `pullbay-daemon
--move-elevated <from> <to>`, started with the `runas` verb so Windows shows
its UAC prompt. The helper copies the file, so it takes the folder's
permissions, then deletes the staged one.

Downloads run up to the **Downloads at once** setting (3 by default) and the
rest queue in order. Changing the setting takes effect at once: raising it
starts queued downloads, lowering it lets running ones finish. Canceling a
queued download takes it off the queue at once.

The speed limit is one token bucket shared by every connection of every
download. Each read takes tokens for the bytes it read and sleeps when the
bucket runs dry, and reads shrink to about a tenth of a second's worth so
throttled downloads flow evenly. A new limit, or turning it off, applies to
running downloads within 100 ms. Each download also has a bucket of its own,
at its own limit or, while the speed limit is on, at the limit per file;
a read waits for both, so the lower limit wins.

The daemon keeps each download's original request, cookies included, while it
can still be retried, so a retry resends the same headers. It drops the request
once the download finishes or is removed. When a retried download is refused
with HTTP 401, 403, 404 or 410, the error says the link or its sign-in may
have expired.

Ids keep increasing across restarts, so a new download never reuses a saved
id. With **Resume unfinished downloads at startup** on, the daemon retries
every interrupted download as soon as it starts.

---

[← Back to the help pages](README.md)
