//! The Scheduler window: its view of the queues, and turning a queue's
//! draft back into a queue. Plain functions, tested without Tauri.

use udm_domain::download::Status;
use udm_domain::id::{DownloadId, QueueId};
use udm_domain::queue::{
    InvalidQueue, Queue, QueueKind, Schedule, WhenDone, QUEUE_CONCURRENCY_RANGE, QUEUE_RETRIES_MAX,
};
use udm_domain::settings::{InvalidSettings, Settings};

use crate::contract::{Id, QueueDraft, QueueFiles, QueueRow, QuotaDraft, SchedulerView, Then};
use crate::projection::{format, Ctx};
use crate::state::AppState;

/// Every day of the week, Monday first.
const ALL_DAYS: [u8; 7] = [0, 1, 2, 3, 4, 5, 6];
/// What a new schedule starts from, as IDM's do.
const DEFAULT_START: &str = "23:00";
const DEFAULT_STOP: &str = "07:30";
const DEFAULT_RETRIES: u32 = 10;

fn then_of(w: WhenDone) -> Then {
    match w {
        WhenDone::Nothing => Then::Nothing,
        WhenDone::Exit => Then::Exit,
        WhenDone::Sleep => Then::Sleep,
        WhenDone::Hibernate => Then::Hibernate,
        WhenDone::ShutDown => Then::ShutDown,
    }
}

/// A queue as the window edits it. `today` (`YYYY-MM-DD`) fills a date not
/// chosen yet.
pub fn draft(q: &Queue, today: &str) -> QueueDraft {
    let s = q.schedule.as_ref();
    let repeat = s.and_then(|s| s.repeat_minutes);
    QueueDraft {
        id: Id(q.id.get()),
        name: q.name.clone(),
        main: q.id == QueueId::MAIN,
        running: q.running,
        sync: q.kind.is_sync(),
        start_on_launch: q.start_on_launch,
        start_on: s.is_some(),
        start: s.map_or(DEFAULT_START.to_string(), |s| s.start.clone()),
        once: s.is_some_and(|s| s.date.is_some()),
        date: s.and_then(|s| s.date.clone()).unwrap_or_else(|| today.to_string()),
        days: s.map(|s| s.days.clone()).filter(|d| !d.is_empty()).unwrap_or_else(|| ALL_DAYS.to_vec()),
        repeat_on: repeat.is_some(),
        repeat_hours: repeat.map_or(2, |m| m / 60),
        repeat_minutes: repeat.map_or(0, |m| m % 60),
        stop_on: s.is_some_and(|s| s.stop.is_some()),
        stop: s.and_then(|s| s.stop.clone()).unwrap_or_else(|| DEFAULT_STOP.to_string()),
        retries_on: q.retries > 0,
        retries: if q.retries > 0 { q.retries } else { DEFAULT_RETRIES },
        open_on: q.open_when_done.is_some(),
        open_file: q.open_when_done.clone().unwrap_or_default(),
        then: then_of(q.when_done),
        force_close: q.force_close,
        max_concurrent: q.max_concurrent as u32,
    }
}

/// `current` with the draft's values, checked.
pub fn queue(d: &QueueDraft, current: &Queue) -> Result<Queue, InvalidQueue> {
    let mut q = current.clone();
    let main = q.id == QueueId::MAIN;
    q.name = d.name.trim().to_string();
    q.kind = if d.sync && !main { QueueKind::Synchronize } else { QueueKind::OneTime };
    q.start_on_launch = d.start_on_launch;
    q.max_concurrent = d.max_concurrent as usize;
    q.retries = if d.retries_on { d.retries } else { 0 };
    q.open_when_done = Some(d.open_file.trim().to_string()).filter(|f| d.open_on && !f.is_empty());
    q.when_done = crate::projection::dialogs::when_done(d.then);
    q.force_close = d.force_close;
    let sync = q.kind.is_sync();
    q.schedule = d.start_on.then(|| {
        let once = d.once && !sync;
        let mut days = d.days.clone();
        days.sort_unstable();
        days.dedup();
        Schedule {
            start: d.start.trim().to_string(),
            stop: d.stop_on.then(|| d.stop.trim().to_string()),
            days: if once { current.schedule.as_ref().map(|s| s.days.clone()).unwrap_or_default() } else { days },
            date: once.then(|| d.date.trim().to_string()),
            repeat_minutes: (sync && d.repeat_on).then_some(d.repeat_hours * 60 + d.repeat_minutes),
            wake: current.schedule.as_ref().is_none_or(|s| s.wake),
        }
    });
    q.validate()?;
    Ok(q)
}

pub fn quota(s: &Settings) -> QuotaDraft {
    let cap = |v: u64| v.min(u64::from(u32::MAX)) as u32;
    QuotaDraft { enabled: s.quota_enabled, mb: cap(s.quota_mb), hours: cap(s.quota_hours), warn: s.quota_warn }
}

/// `current` with the download limits, checked.
pub fn quota_settings(d: QuotaDraft, current: &Settings) -> Result<Settings, InvalidSettings> {
    let mut s = current.clone();
    s.quota_enabled = d.enabled;
    s.quota_mb = u64::from(d.mb);
    s.quota_hours = u64::from(d.hours);
    s.quota_warn = d.warn;
    s.validate()?;
    Ok(s)
}

/// The name New queue proposes: the first free `Queue # n`.
fn new_name(queues: &[Queue]) -> String {
    (queues.len() + 1..)
        .map(|n| format!("Queue # {n}"))
        .find(|name| !queues.iter().any(|q| q.name.eq_ignore_ascii_case(name)))
        .unwrap_or_default()
}

/// Where download `id` moves when it goes `offset` places along its queue:
/// the queue and the new place, or `None` when it is in no queue.
pub fn moved(queues: &[Queue], id: DownloadId, offset: i32) -> Option<(QueueId, usize)> {
    let q = queues.iter().find(|q| q.order.contains(&id))?;
    let at = q.order.iter().position(|d| *d == id)? as i64;
    let to = (at + i64::from(offset)).clamp(0, q.order.len() as i64 - 1);
    Some((q.id, to as usize))
}

pub fn view(ctx: &Ctx, state: &AppState) -> SchedulerView {
    let lang = ctx.lang;
    let files = state
        .queues
        .iter()
        .map(|q| QueueFiles {
            queue: Id(q.id.get()),
            rows: q
                .order
                .iter()
                .filter_map(|id| state.table.get(*id))
                .map(|e| {
                    let speed = state.speed(e.id).filter(|_| e.status == Status::Downloading);
                    let name = e.display_name();
                    QueueRow {
                        id: Id(e.id.get()),
                        icon: format::file_icon(&name),
                        name,
                        size: e.total.map(|t| format::bytes(lang, t, 2)).unwrap_or_default(),
                        status: match (e.status, format::fraction(e)) {
                            (Status::Downloading, Some(f)) => format!("{}%", lang.number(f64::from(f) * 100.0, 2)),
                            (s, _) => format::status_word(lang, s).to_string(),
                        },
                        time_left: format::time_left(e, speed).map(|s| format::duration(lang, s)).unwrap_or_default(),
                    }
                })
                .collect(),
        })
        .collect();
    SchedulerView {
        queues: state.queues.iter().map(|q| draft(q, &ctx.today)).collect(),
        files,
        quota: quota(&state.settings.clone().unwrap_or_default()),
        new_name: new_name(&state.queues),
        max_at_once: QUEUE_CONCURRENCY_RANGE.1 as u32,
        max_retries: QUEUE_RETRIES_MAX,
        connected: state.connected(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn main_queue() -> Queue {
        Queue::new(QueueId::MAIN, "Main queue")
    }

    #[test]
    fn an_unchanged_draft_keeps_the_queue() {
        let q = main_queue();
        assert_eq!(queue(&draft(&q, "2026-11-12"), &q).unwrap(), q);
    }

    #[test]
    fn a_daily_schedule_with_a_stop_time() {
        let q = main_queue();
        let mut d = draft(&q, "2026-11-12");
        d.start_on = true;
        d.start = "23:00".into();
        d.stop_on = true;
        d.days = vec![6, 0, 0];
        d.retries_on = true;
        d.then = Then::ShutDown;
        let out = queue(&d, &q).unwrap();
        let s = out.schedule.unwrap();
        assert_eq!((s.start.as_str(), s.stop.as_deref(), s.days.as_slice()), ("23:00", Some("07:30"), &[0, 6][..]));
        assert_eq!(out.retries, 10);
        assert_eq!(out.when_done, WhenDone::ShutDown);
    }

    #[test]
    fn once_on_a_date_and_bad_times() {
        let q = main_queue();
        let mut d = draft(&q, "2026-11-12");
        d.start_on = true;
        d.once = true;
        assert_eq!(queue(&d, &q).unwrap().schedule.unwrap().date.as_deref(), Some("2026-11-12"));
        d.start = "25:00".into();
        assert_eq!(queue(&d, &q), Err(InvalidQueue::StartTime("25:00".into())));
    }

    #[test]
    fn only_other_queues_synchronize_and_repeat() {
        let mut d = draft(&main_queue(), "2026-11-12");
        d.sync = true;
        assert_eq!(queue(&d, &main_queue()).unwrap().kind, QueueKind::OneTime);
        let sync = Queue::new(QueueId::new(2), "Sync");
        let mut d = draft(&sync, "2026-11-12");
        d.sync = true;
        d.start_on = true;
        d.repeat_on = true;
        d.repeat_hours = 1;
        d.repeat_minutes = 30;
        let out = queue(&d, &sync).unwrap();
        assert_eq!(out.kind, QueueKind::Synchronize);
        assert_eq!(out.schedule.unwrap().repeat_minutes, Some(90));
    }

    #[test]
    fn new_queues_get_a_free_name() {
        let queues = vec![main_queue(), Queue::new(QueueId::new(2), "Queue # 3")];
        assert_eq!(new_name(&queues), "Queue # 4");
    }

    #[test]
    fn downloads_move_within_their_queue() {
        let mut q = main_queue();
        q.order = vec![DownloadId::new(5), DownloadId::new(6), DownloadId::new(7)];
        assert_eq!(moved(&[q.clone()], DownloadId::new(6), -1), Some((QueueId::MAIN, 0)));
        assert_eq!(moved(&[q.clone()], DownloadId::new(7), 1), Some((QueueId::MAIN, 2)));
        assert_eq!(moved(&[q], DownloadId::new(9), 1), None);
    }

    #[test]
    fn the_limits_are_checked() {
        let s = Settings::default();
        let d = QuotaDraft { enabled: true, mb: 0, hours: 5, warn: true };
        assert_eq!(quota_settings(d, &s), Err(InvalidSettings::QuotaZero));
        let d = QuotaDraft { mb: 200, ..d };
        assert!(quota_settings(d, &s).unwrap().quota_enabled);
    }
}
