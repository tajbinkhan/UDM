# Remote control from a phone

Pullbay can serve a small page to phones (or tablets, or another computer's browser) on your home
network. A paired phone sees the download list with each download's progress, and can add a link,
pause, resume and remove downloads. It cannot change settings, open files, or reach anything else
on the computer.

It is **off** until you turn it on. While it is off, nothing listens on the network.

## Turning it on

1. Open **Options > Remote control**.
2. Tick **Let phones on this network control Pullbay**.
3. Leave the port (18443) unless something else uses it, and choose where to listen:
   - **Every network interface** (the default), or
   - **Only these addresses**: this computer's own IP addresses, separated by commas, such as
     `192.168.1.20`. Use this when the computer is on more than one network (a VPN, a guest
     network) and the page should be on one of them only.
4. Choose **Save**. The page shows its address (`https://192.168.1.20:18443/`) and its
   certificate's fingerprint. Turning the option off and saving stops the page at once.

While remote control is on, Pullbay's background program (the daemon) keeps running even when
no window is open and nothing is downloading, so a phone can reach it any time. It starts when
Pullbay starts; turn on **Start with Windows, in the tray** (**Start when I sign in, in the tray**
on macOS and Linux) in Options > General if the phone should reach it without opening Pullbay
first.

### On a server or in Docker

A daemon started with `--server` ([server mode](server.md)) serves the page too. It has no window,
so turn remote control on with `"remote_enabled": true` in `settings.json`, and pair with
`pullbay-cli pair` (in a container: `docker exec -it pullbay pullbay-cli pair --host <server
address>`), which prints the QR code, the code and the fingerprint; `pullbay-cli devices` and
`pullbay-cli unpair ID` list and remove phones. In Docker the port has to be published (commented
out in `installer/docker/compose.yaml`); see [From a phone](server.md#from-a-phone).

### The firewall

The page is served by the daemon (`pullbay-daemon`, `pullbay-daemon.exe` on Windows). The first
time it listens, Windows asks whether to allow it: allow **private networks** only, never public
ones. macOS may ask whether to accept incoming connections for it. On Linux, open the port if a
firewall runs, for the home network only, for example with `ufw allow from 192.168.1.0/24 to any
port 18443 proto tcp`.

Do not forward the port on your router. The page is meant for the home network; it is not made to
face the internet.

## Pairing a phone

1. With remote control on and saved, choose **Pair a phone...** in Options > Remote control.
   Pullbay shows a QR code, the code itself and how long it still works (5 minutes).
2. Scan the QR code with the phone's camera and open the link. (Or open the page's address on the
   phone and type the code.)
3. The phone warns that the page's certificate is not trusted. That is expected: the certificate
   is Pullbay's own, made on this computer, and no public authority signed it. **Before going
   on, check the fingerprint**: in the warning, look at the certificate's details and compare its
   SHA-256 fingerprint with the one Options shows. If they differ, something else on the network
   is answering for Pullbay; do not go on. If they match, go on (Chrome: *Advanced > Proceed*;
   Safari: *Show Details > visit this website*; Firefox: *Advanced > Accept the Risk*).
4. The page pairs by itself and shows the list. The phone appears under **Paired devices** in
   Options.

The code works once. A new code replaces the one shown, and closing Options withdraws it.

## Removing a device

In Options > Remote control, choose **Remove** next to the device. It stops working at once: its
next request is refused. The page on a phone can also unpair itself (**Unpair this phone** at the
bottom of the list).

A phone keeps its pairing in the browser's storage for the page's address. Clearing the browser's
data for the site, or using a private tab, forgets it; pair again then.

## On the phone

- **Add a link**: paste an `http`, `https` or `ftp` link. It starts at once, into the folder
  Pullbay's settings choose for it, with the server's file name. It never asks where to save.
- **Pause** stops a running download and keeps what it has.
- **Resume** goes on with a stopped or failed download. As in the window, a stopped download goes
  on as a new download.
- **Remove** takes a download out of the list. **The file stays on the computer** unless you tick
  *Also delete the file*; then it goes to the Recycle Bin or trash. A running download cannot be
  removed; pause it first.

The page refreshes every 2 seconds while it is open (every 10 while it is in the background). Its
text is in the phone's language when Pullbay has it (English, German, Spanish and French), else
English. The daemon's own refusals (for example why a download cannot be resumed) are in English.

## How it is protected (threat model)

**What it protects.** The download list and the ability to add, pause, resume and remove
downloads; and, behind them, the computer: the daemon's token, the settings, the files.

**Who it guards against.** Other people and devices on the same network (a guest on the Wi-Fi, a
compromised smart device), web pages open in a browser on the network that try to reach the page
(cross-site requests, DNS rebinding), and anyone who learns the page's address.

- **Nothing listens until you turn it on**, and turning it off stops the listener. It listens only
  on the addresses and port Options names.
- **HTTPS only.** The certificate is made on this computer the first time remote control is turned
  on (RSA 2048, SHA-256, valid for 800 days and made again before it runs out). Its private key is
  kept in the data folder, sealed for your user (DPAPI on Windows; elsewhere encrypted with the
  data folder's key) and readable by your user only. The fingerprint Options shows is how you tell
  Pullbay's certificate from an impostor's when the phone first connects.
- **Pairing.** The code is 128 random bits, works once, and runs out after 5 minutes. It travels in
  the link's `#` fragment, which browsers never send to a server, so it appears in no request line
  and no log. A phone that pairs gets a credential of its own: its device number and a 256-bit
  random secret. Pullbay keeps only the SHA-256 digest of each secret, in `remote/devices.bin` in
  the data folder, sealed like the saved requests and readable by your user only.
- **Every request but pairing needs a paired credential**, sent in the `Authorization` header,
  and checked in constant time. The page itself (its HTML, script and style) holds no data and is
  served to anyone who can reach the port, so a phone can load it to pair.
- **Guessing is cut short.** Five wrong codes from one address within a minute lock that address
  out for the rest of the minute, even with the right code; twenty wrong codes from anywhere
  within ten minutes withdraw the code shown. Ten wrong credentials from one address within a
  minute lock it out too.
- **Other sites' pages are refused.** No CORS header is ever sent, so another site's page cannot
  read an answer. Requests whose `Origin` is not the page's own address, or that a browser marks as
  cross-site, are refused, and every request that changes something must be JSON, which a plain
  form cannot send. The credential is never a cookie, so a browser never sends it on its own.
- **Safe headers.** Every answer has `Cache-Control: no-store`, `X-Content-Type-Options: nosniff`,
  `X-Frame-Options: DENY`, `Referrer-Policy: no-referrer` and a strict Content Security Policy
  (`default-src 'none'`, scripts and styles from the page only, `frame-ancestors 'none'`).
- **The daemon's token never leaves the computer.** The page's requests run inside the daemon,
  through the same commands a window uses, without the token. Pairing codes, device secrets and
  the token are never logged.
- **The list shows little.** For each download the page gets its file name, the server's name,
  its state, progress and why it failed; never its full address, folder, referrer, login or
  cookies.
- **Small and bounded.** At most 32 connections at once; requests over 8 KB of headers or 16 KB of
  body are refused; slow requests time out.
- **Nothing is deleted unless asked.** Removing keeps the file unless *Also delete the file* is
  ticked, and even then it goes to the Recycle Bin or trash.

**What it does not protect against.**

- **Trust on first use.** The first time a phone connects, only the fingerprint check tells
  Pullbay's certificate from one presented by someone intercepting the network. Skip the check and
  a person in the middle could pair in the phone's place. After the first visit the phone's
  browser remembers the certificate and warns again if another one appears.
- **A lost or shared phone** is a paired phone: remove it in Options. Anyone who can use a paired
  phone's browser can control downloads until then.
- **Anyone on the network can see that the page exists** and load it, and can try codes (which
  the limits above make hopeless).
- **The computer itself.** Programs running as your user can read what Pullbay keeps, as they can
  for every program of yours.
- **The internet.** The page is not built to face it. Keep the port off your router.

## Files

In the data folder (`%LOCALAPPDATA%\Pullbay` on Windows, `~/Library/Application Support/Pullbay`
on macOS, `~/.local/share/pullbay` on Linux, or the portable copy's `Data` folder):

- `remote/certificate.json`: the page's certificate (public) and when it runs out.
- `remote/certificate.key`: its private key, sealed.
- `remote/devices.bin`: the paired devices (number, name, when paired and last seen, the digest of
  the secret), sealed.

Deleting `remote/` unpairs every phone and makes a new certificate (with a new fingerprint) the
next time remote control starts; each phone then has to accept the new certificate and pair again.

The settings are `remote_enabled`, `remote_port` and `remote_addresses` in `settings.json`; a
settings file from before remote control keeps it off.
