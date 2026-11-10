# Notifications and virus scans

Windows only shows notifications from an app it knows, so at start the daemon
registers the app id `Pullbay.DownloadManager` under
`HKCU\Software\Classes\AppUserModelId` with Pullbay's name and an icon it writes
to `%LOCALAPPDATA%\Pullbay\pullbay.png`. A copy started with `PULLBAY_INSTANCE=test`
registers `Pullbay.DownloadManager.test` with the name `Pullbay (test)` and the icon
in its own data folder instead, so it leaves the installed copy's entry alone.
Each button carries an argument such as
`open:12`, which the daemon acts on while it runs. A failure is announced one
second late, and only if the download is still failed then, so a download the
extension hands back to the browser is not announced. A file with a threat
is never opened from a notification; its Open button brings up the window,
which asks first.

The scanner runs as a program with separate arguments, never through a shell.
A custom command line is split into arguments first (double quotes group;
backslashes are literal) and `{path}` is replaced afterwards inside each
argument, so a file name cannot add arguments or run anything, whatever it
contains. Defender runs as `MpCmdRun.exe -Scan -ScanType 3 -File <path>
-DisableRemediation` and reports a threat with exit code 2. A custom scanner
reports a threat with exit code 1 (ClamAV's convention). Any other exit code
is a failed scan, as is one that runs over 10 minutes.

---

[← Back to the help pages](README.md)
