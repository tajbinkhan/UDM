# Queues and the scheduler

Queues live in the daemon, above the engine, and are saved in
`history.json`. A scheduler thread runs a pass whenever a download ends or a
queue changes, and every 5 seconds for the clock. Each pass:

1. starts a queue whose scheduled run is open now and has not run today (it
   records the run's day, so a queue stopped by hand or finished early does
   not start again in the same window);
2. stops a queue its schedule started once that run's stop time passes; a
   run without a stop time that never got any downloads ends with its day;
3. in each started queue, starts waiting downloads in order until the queue's
   own limit is reached (the engine's **Downloads at once** still caps the
   total), and puts a download that failed in this run back to waiting while
   it has retries left;
4. ends the run of a started queue with nothing waiting or running, stops it,
   and if the run started anything, does its **when done** action. A
   scheduled run that has not started anything yet stays open until its stop
   time, so downloads added during the window still run.

A stopped queue's running downloads are canceled and their `canceled` event
becomes `waiting`, so they stay in the queue with their partial file and
sidecar. Starting one again resumes that file under the same id. After a
restart, a queue's downloads that were running wait in it again, a queue
that was started carries on, and a schedule whose window is still open
starts its queue. The daemon does not exit when idle while a queue is
started, a schedule has a run ahead, or a countdown runs. Without Start with
Windows nothing starts the daemon, though; see Known gaps.

Sleep and hibernate use `SetSuspendState`; shutting down runs
`shutdown.exe /s /t 0`. The daemon saves the history as the countdown starts.
With the environment variable `PULLBAY_POWER_DRY_RUN` set, the daemon only logs
what it would do, which is how these were tested.

`refresh_link` with a `download` gives download `id` (failed, canceled,
interrupted or waiting) that request as its new link. If the download had
started, the daemon probes the new link first and compares it with the
partial file's sidecar; for another file it replies `refresh_mismatch` and
changes nothing, unless `restart` is true. A waiting download keeps waiting
with its new link; any other starts again (as `retry` does, with a new id).
Without `download`, the daemon waits for the browser: it broadcasts
`refresh_waiting`, and the extension sends its next captured download as that
`refresh_link`, whatever the capture rules say. `cancel_refresh` stops
waiting; `refresh_waiting` with `id: null` says nothing waits.
`add` may carry `browser_proxy`, the browser's proxy setting, used when the
`use_browser_proxy` setting is on: `mode` (`direct`, `fixed`, `pac`,
`auto_detect` or `system`), the fixed proxies `http`, `https` and `socks`
(each `scheme://host:port`), the configuration script's `pac_url` or its
text `pac_script`, and the `bypass` list. Older extensions send only
`proxy`, the fixed proxy, which `browser_proxy` replaces when both come.

---

[← Back to the help pages](README.md)
