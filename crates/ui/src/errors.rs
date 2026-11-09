//! The domain's errors, and the daemon's, in the window's language. Each
//! English text matches the error's own message, which the daemon, the
//! logs and the clients that do not translate use.

use uidm_application::daemon::service::ServiceError;
use uidm_application::daemon::Failure;
use uidm_application::downloads::{ConfirmError, StartOrWaitError};
use uidm_domain::batch::{BatchError, Bound, MAX_BATCH, MAX_WILDCARD_SIZE};
use uidm_domain::command::{CommandError, PATH_TOKEN};
use uidm_domain::download::DownloadError;
use uidm_domain::grab::{InvalidGrab, AT_ONCE_RANGE, DELAY_MS_RANGE, DEPTH_RANGE, MAX_MB_RANGE, MAX_PAGES_RANGE};
use uidm_domain::ports::SettingsError;
use uidm_domain::queue::{InvalidQueue, QueueError, QUEUE_CONCURRENCY_RANGE, QUEUE_RETRIES_MAX};
use uidm_domain::settings::{
    InvalidSettings, ProxyEntry, SettingsField, CONNECTIONS_RANGE, GENERAL, MAX_CONCURRENT_RANGE, QUOTA_HOURS_RANGE,
    SERVER_CONNECTIONS_RANGE, SPEED_LIMIT_RANGE_KBPS, TEXT_SIZE_RANGE,
};
use uidm_domain::update::UpdateError;

use crate::i18n::{tr, trf};

/// A failure to show the user, in their language. Only what a server, the
/// disk or a daemon too old to send an error's kind said stays in its own
/// words.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct UiError(String);

impl UiError {
    /// A message already in the user's language.
    pub fn new(message: impl Into<String>) -> UiError {
        UiError(message.into())
    }

    /// The message, to draw.
    pub fn text(&self) -> &str {
        &self.0
    }
}

impl From<String> for UiError {
    fn from(message: String) -> Self {
        UiError(message)
    }
}

impl From<&str> for UiError {
    fn from(message: &str) -> Self {
        UiError(message.to_string())
    }
}

impl From<BatchError> for UiError {
    fn from(e: BatchError) -> Self {
        UiError(batch(&e))
    }
}

impl From<InvalidSettings> for UiError {
    fn from(e: InvalidSettings) -> Self {
        UiError(settings(&e))
    }
}

impl From<SettingsError> for UiError {
    fn from(e: SettingsError) -> Self {
        UiError(settings_error(&e))
    }
}

impl From<InvalidQueue> for UiError {
    fn from(e: InvalidQueue) -> Self {
        UiError(queue(&e))
    }
}

impl From<InvalidGrab> for UiError {
    fn from(e: InvalidGrab) -> Self {
        UiError(grab(&e))
    }
}

impl From<Failure> for UiError {
    fn from(e: Failure) -> Self {
        UiError(daemon(&e))
    }
}

impl From<&UiError> for eframe::egui::RichText {
    fn from(e: &UiError) -> Self {
        e.0.clone().into()
    }
}

impl From<&UiError> for eframe::egui::WidgetText {
    fn from(e: &UiError) -> Self {
        e.0.clone().into()
    }
}

fn bound(b: Bound) -> &'static str {
    match b {
        Bound::From => tr("From"),
        Bound::To => tr("To"),
    }
}

/// A batch pattern, IDM-style address or list that makes no downloads.
pub fn batch(e: &BatchError) -> String {
    use BatchError as E;
    match e {
        E::NumberTooBig(r) => trf!("[{range}]: the number is too big", range = r),
        E::StepTooBig(r) => trf!("[{range}]: the step is too big", range = r),
        E::StepZero(r) => trf!("[{range}]: the step must be at least 1", range = r),
        E::MixedCaseRange(r) => trf!("[{range}]: both letters must be the same case", range = r),
        E::PatternNotWeb => tr("The pattern must start with http://, https:// or ftp://.").to_string(),
        E::PatternTooBig(n) => {
            trf!("The pattern makes {count} addresses; at most {max} can be added at once.", count = n, max = MAX_BATCH)
        }
        E::AddressNotWeb => tr("The address must start with http://, https:// or ftp://.").to_string(),
        E::NoAsterisk => tr("The address must have a * where the numbers or letters go.").to_string(),
        E::TwoAsterisks => tr("The address may have only one *.").to_string(),
        E::NotWholeNumber(b) => trf!("{field} must be a whole number.", field = bound(*b)),
        E::BoundTooBig(b) => trf!("{field} is too big.", field = bound(*b)),
        E::WildcardSize => trf!("The wildcard size must be from 1 to {max}.", max = MAX_WILDCARD_SIZE),
        E::NumbersReversed(a, b) => trf!("From ({from}) must not be greater than To ({to}).", from = a, to = b),
        E::NotOneLetter(b) => trf!("{field} must be one letter, a to z.", field = bound(*b)),
        E::MixedCaseLetters => tr("From and To must be letters of the same case.").to_string(),
        E::LettersReversed(a, b) => trf!("From ({from}) must not be after To ({to}).", from = a, to = b),
        E::RangeTooBig(a, b, n) => trf!(
            "From {from} to {to} makes {count} addresses; at most {max} can be added at once.",
            from = a,
            to = b,
            count = n,
            max = MAX_BATCH
        ),
        E::ListTooLong(n) => {
            trf!("The list has {count} addresses; at most {max} can be added at once.", count = n, max = MAX_BATCH)
        }
    }
}

/// A scanner command line that cannot be run.
pub fn command(e: &CommandError) -> String {
    match e {
        CommandError::UnclosedQuote => tr("The scanner command has an unclosed quote.").to_string(),
        CommandError::Empty => tr("The scanner command is empty.").to_string(),
        CommandError::NoPath => {
            trf!("The scanner command must contain {token} where the file goes.", token = PATH_TOKEN)
        }
    }
}

fn field(f: &SettingsField) -> String {
    match f {
        SettingsField::DownloadFolder => tr("The download folder").to_string(),
        SettingsField::CategoryFolder(name) => trf!("The folder of \"{name}\"", name = name),
        SettingsField::Category(name) => trf!("The category \"{name}\"", name = name),
        SettingsField::CapturedTypes => tr("The captured file types").to_string(),
        SettingsField::VideoTypes => tr("The video types").to_string(),
        SettingsField::PlayerTypes => tr("The types taken from web players").to_string(),
        SettingsField::AutoStartTypes => tr("The file types that start without asking").to_string(),
    }
}

fn proxy(p: ProxyEntry) -> &'static str {
    match p {
        ProxyEntry::Http => tr("The HTTP proxy"),
        ProxyEntry::Https => tr("The HTTPS proxy"),
        ProxyEntry::Ftp => tr("The FTP proxy"),
        ProxyEntry::Socks => tr("The SOCKS proxy"),
    }
}

/// Settings that break one of their rules.
pub fn settings(e: &InvalidSettings) -> String {
    use InvalidSettings as E;
    match e {
        E::ScriptNotWeb => tr("The configuration script must be an http or https address.").to_string(),
        E::Language(code) => trf!("\"{code}\" is not a language code, such as de or pt-BR.", code = code),
        E::TextSize => {
            trf!("The text size must be between {lo}% and {hi}%.", lo = TEXT_SIZE_RANGE.0, hi = TEXT_SIZE_RANGE.1)
        }
        E::QuotaHours => trf!(
            "The quota's window must be between {lo} and {hi} hours.",
            lo = QUOTA_HOURS_RANGE.0,
            hi = QUOTA_HOURS_RANGE.1
        ),
        E::QuotaZero => tr("The quota must be at least 1 MB.").to_string(),
        E::ServerName(site) => trf!("\"{site}\" is not a server name, such as files.example.com.", site = site),
        E::ServerConnections(site) => trf!(
            "The connections for {site} must be between {lo} and {hi}.",
            site = site,
            lo = SERVER_CONNECTIONS_RANGE.0,
            hi = SERVER_CONNECTIONS_RANGE.1
        ),
        E::SameCaptureKeys => tr("The keys that skip and force capture must differ.").to_string(),
        E::InsertDelSkip => tr("Insert and Del can only force capture, not skip it.").to_string(),
        E::TempNotFull(dir) => trf!("The temporary folder must be a full path: {path}", path = dir),
        E::UserAgentLines => tr("The User-Agent must be one line of text.").to_string(),
        E::Connections => trf!(
            "Connections per file must be between {lo} and {hi}.",
            lo = CONNECTIONS_RANGE.0,
            hi = CONNECTIONS_RANGE.1
        ),
        E::DownloadsAtOnce => trf!(
            "Downloads at once must be between {lo} and {hi}.",
            lo = MAX_CONCURRENT_RANGE.0,
            hi = MAX_CONCURRENT_RANGE.1
        ),
        E::SpeedLimit => trf!(
            "The speed limit must be between {lo} and {hi} KB/s.",
            lo = SPEED_LIMIT_RANGE_KBPS.0,
            hi = SPEED_LIMIT_RANGE_KBPS.1
        ),
        E::SpeedLimitPerFile => trf!(
            "The speed limit per file must be between {lo} and {hi} KB/s.",
            lo = SPEED_LIMIT_RANGE_KBPS.0,
            hi = SPEED_LIMIT_RANGE_KBPS.1
        ),
        E::FolderEmpty(f) => trf!("{field} is empty.", field = field(f)),
        E::FolderNotFull(f, dir) => trf!("{field} must be a full path: {path}", field = field(f), path = dir),
        E::CategoryUnnamed => tr("Every category needs a name.").to_string(),
        E::GeneralName => trf!("\"{name}\" is kept for files of every other type.", name = tr(GENERAL)),
        E::TwoCategories(name) => trf!("There are two categories named \"{name}\".", name = name),
        E::NoFileTypes(name) => trf!("The category \"{name}\" has no file types.", name = name),
        E::BadFileType(f, ext) => trf!(
            "{field} has an invalid file type \"{ext}\". Use letters and digits, such as zip or mp4.",
            field = field(f),
            ext = ext
        ),
        E::VideoMinSize => tr("The minimum video size is too large.").to_string(),
        E::NoPlayerTypes => tr("List the types to take from web players, such as mp4.").to_string(),
        E::MenuItem(item) => trf!("\"{item}\" is not a context menu item.", item = item),
        E::CaptureMinSize => tr("The minimum size to capture is too large.").to_string(),
        E::NotSite(site) => {
            trf!("\"{site}\" is not a site. Use a host name such as example.com or *.example.com.", site = site)
        }
        E::FfmpegNotFull(path) => trf!("ffmpeg must be the full path of ffmpeg.exe: {path}", path = path),
        E::ProxyNotHostPort(p) => trf!("{proxy} must be host:port, such as proxy.example.com:8080.", proxy = proxy(*p)),
        E::NoProxy => tr("Set at least one proxy, or choose another proxy option.").to_string(),
        E::NotLoginSite(site) => trf!(
            "\"{site}\" is not a site. Use a host name such as example.com, or one with a folder such as example.com/private/*.",
            site = site
        ),
        E::LoginNoUser(site) => trf!("The login for {site} needs a user name.", site = site),
        E::TwoLogins(site) => trf!("There are two logins for {site}.", site = site),
        E::SoundNotWav(path) => trf!("A sound must be the full path of a .wav file: {path}", path = path),
        E::Scanner(e) => command(e),
        E::DefenderNotHere => {
            tr("Microsoft Defender is only on Windows. Choose a scanner program such as clamscan.").to_string()
        }
    }
}

/// Why settings could not be saved: a broken rule, translated, or a disk
/// check's message.
pub fn settings_error(e: &SettingsError) -> String {
    match e {
        SettingsError::Invalid(e) => settings(e),
        SettingsError::Refused(why) => why.clone(),
    }
}

/// A queue's settings or schedule that break a rule.
pub fn queue(e: &InvalidQueue) -> String {
    use InvalidQueue as E;
    match e {
        E::NoName => tr("A queue needs a name.").to_string(),
        E::AtOnce => trf!(
            "A queue runs between {lo} and {hi} downloads at once.",
            lo = QUEUE_CONCURRENCY_RANGE.0,
            hi = QUEUE_CONCURRENCY_RANGE.1
        ),
        E::Retries => trf!("A queue retries a download at most {max} times.", max = QUEUE_RETRIES_MAX),
        E::OpenNotFull(path) => trf!("The file to open must be a full path: {path}", path = path),
        E::StartTime(t) => trf!("\"{time}\" is not a start time. Use HH:MM, such as 23:00.", time = t),
        E::StopTime(t) => trf!("\"{time}\" is not a stop time. Use HH:MM, such as 07:00.", time = t),
        E::SameTimes => tr("The stop time must differ from the start time.").to_string(),
        E::Repeat => tr("Start again every 1 minute to 99 hours 59 minutes.").to_string(),
        E::Date(d) => trf!("\"{date}\" is not a date. Use YYYY-MM-DD.", date = d),
        E::NoDays => tr("Pick at least one day, or a date, for the schedule.").to_string(),
        E::DayOfWeek => tr("Days of the week run from 0 (Monday) to 6 (Sunday).").to_string(),
    }
}

/// A grabber request that cannot run.
pub fn grab(e: &InvalidGrab) -> String {
    use InvalidGrab as E;
    match e {
        E::StartNotAddress => tr("The start page must be a full web address.").to_string(),
        E::StartNotWeb => tr("The start page must start with http:// or https://.").to_string(),
        E::Depth => trf!("The depth must be between {lo} and {hi}.", lo = DEPTH_RANGE.0, hi = DEPTH_RANGE.1),
        E::AtOnce => trf!("Files at once must be between {lo} and {hi}.", lo = AT_ONCE_RANGE.0, hi = AT_ONCE_RANGE.1),
        E::NoCategory => tr("Choose the category to save the files in.").to_string(),
        E::FolderNotFull(dir) => trf!("The folder to save the files in must be a full path: {path}", path = dir),
        E::Pages => {
            trf!("The page budget must be between {lo} and {hi}.", lo = MAX_PAGES_RANGE.0, hi = MAX_PAGES_RANGE.1)
        }
        E::Size => trf!("The size budget must be between {lo} and {hi} MB.", lo = MAX_MB_RANGE.0, hi = MAX_MB_RANGE.1),
        E::Delay => trf!(
            "The wait between pages must be between {lo} and {hi} ms.",
            lo = DELAY_MS_RANGE.0,
            hi = DELAY_MS_RANGE.1
        ),
        E::NoTypes => tr("Choose at least one file type to keep.").to_string(),
        E::LoginNotAddress => tr("The login page must be a full web address.").to_string(),
        E::LoginNotWeb => tr("The login page must start with http:// or https://.").to_string(),
        E::MirrorNotFull(dir) => trf!("The folder to keep the site in must be a full path: {path}", path = dir),
        E::SizesReversed => tr("The smallest file size is larger than the largest.").to_string(),
    }
}

/// A failure the daemon reported.
pub fn daemon(e: &Failure) -> String {
    match e {
        Failure::Service(e) => service(e),
        Failure::Settings(e) => settings_error(e),
        Failure::InvalidMessage(why) => trf!("invalid message: {why}", why = why),
        Failure::Probe(why) | Failure::Other(why) => why.clone(),
    }
}

/// A command the daemon refused.
fn service(e: &ServiceError) -> String {
    match e {
        ServiceError::Queue(e) => queue_error(e),
        ServiceError::Download(e) | ServiceError::Confirm(ConfirmError::Download(e)) => download(e),
        ServiceError::Confirm(ConfirmError::Start(StartOrWaitError::Queue(e))) => queue_error(e),
        // The engine's own words.
        ServiceError::Confirm(ConfirmError::Start(StartOrWaitError::Start(e))) => e.0.clone(),
        ServiceError::Update(e) => update(e),
        ServiceError::Grab(e) => grab(e),
        ServiceError::CrawlRunning => tr("A site crawl is already running.").to_string(),
        ServiceError::BadBody(url) => trf!("the form body of {url} is not valid base64", url = url),
        ServiceError::NotDownloadAddress(url) => trf!("{url} is not an http, https or ftp address", url = url),
    }
}

/// A change to the queues that was refused.
fn queue_error(e: &QueueError) -> String {
    match e {
        QueueError::NoQueue(id) => trf!("no queue with id {id}", id = id),
        QueueError::NoDownload(id) => trf!("no download with id {id}", id = id),
        QueueError::NameTaken(name) => trf!("There is already a queue named \"{name}\".", name = name),
        QueueError::MainQueue => tr("The main queue cannot be deleted.").to_string(),
        QueueError::Finished => tr("Only a download that has not finished can wait in a queue.").to_string(),
        QueueError::NotQueued => tr("That download is not in a queue.").to_string(),
        QueueError::Invalid(e) => queue(e),
    }
}

/// A change to a download that was refused.
fn download(e: &DownloadError) -> String {
    use DownloadError as E;
    match e {
        E::NoDownload(id) => trf!("no download with id {id}", id = id),
        E::StillRunning(id) => trf!("download {id} is still running; cancel it first", id = id),
        E::NotRetryable(id) => trf!("download {id} is not failed, canceled or interrupted", id = id),
        E::NotFinishedOrStopped => tr("Only a finished or stopped download can be downloaded again.").to_string(),
        E::PlaylistGone => {
            tr("This stream's playlist is no longer known. Play it in the browser again to download it.").to_string()
        }
        E::StartedMeanwhile => tr("The download started again in the meantime.").to_string(),
        E::BadSpeedLimit { kbps, lo, hi } => {
            trf!("{kbps} KB/s is not a speed limit between {lo} and {hi} KB/s.", kbps = kbps, lo = lo, hi = hi)
        }
        E::NotStopped => tr("Stop the download before changing its properties.").to_string(),
        E::LinkNeedsStop => tr("Only a stopped download can get a new link.").to_string(),
        E::NotDownloadLink(url) => trf!("{url} is not an http, https or ftp link", url = url),
        E::NotWebAddress(url) => trf!("{url} is not an http or https address", url = url),
        E::LoginOnFinished => tr("A finished download keeps no login. Download it again to use one.").to_string(),
        E::RenameUnfinished => tr("Only a finished download's file can be renamed.").to_string(),
        E::RelativeFolder(dir) => trf!("The folder must be a full path: {path}", path = dir),
        E::NoFile => tr("The download has no file.").to_string(),
        E::NoFileName => tr("The download has no file name.").to_string(),
        E::NoFolder => tr("The download has no folder.").to_string(),
        E::Exists(path) => trf!("{path} already exists.", path = path),
        E::StartedWhileSaving => tr("The download started while its properties were being saved.").to_string(),
        E::NotConvertible => tr("Only a finished download whose file is still there can be converted.").to_string(),
        E::NoFfmpeg => tr("Converting needs ffmpeg. Install it, or choose it in Options > General.").to_string(),
        E::NotWaiting(id) => trf!("download {id} is not waiting to be started", id = id),
        E::AnswerNoFileName => tr("the download has no file name").to_string(),
        E::AnswerNoFolder => tr("the download has no folder").to_string(),
        E::AnswerRelativeFolder(dir) => trf!("the folder must be a full path: {path}", path = dir),
        E::AlreadyFinished => tr("This download has already finished.").to_string(),
        E::Unreachable(why) => trf!("could not reach the new link: {why}", why = why),
        // How moving the file failed, in the system's words.
        E::Files(why) => why.clone(),
        E::FilesStay { url, reason } => {
            trf!("Removed {url} from the list, but its files stay: {reason}", url = url, reason = reason)
        }
    }
}

/// Looking for or installing an update failed.
fn update(e: &UpdateError) -> String {
    match e {
        UpdateError::Unreachable(why) => trf!("Could not reach GitHub: {why}", why = why),
        // How GitHub's answer or the Setup program was wrong.
        UpdateError::Answer(why) | UpdateError::BadSetup(why) => why.clone(),
        UpdateError::NewestAlready => tr("This is the newest version.").to_string(),
        UpdateError::NoSetup => tr("This release has no Setup program; download it from its page.").to_string(),
        UpdateError::Download(why) => trf!("Could not download the update: {why}", why = why),
        UpdateError::Save { path, reason } => {
            trf!("Could not save the update to {path}: {reason}", path = path, reason = reason)
        }
        UpdateError::Start { path, reason } => trf!("Could not start {path}: {reason}", path = path, reason = reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uidm_domain::ports::ControlError;

    /// `?` turns the domain's errors into the window's, in the user's words.
    #[test]
    fn domain_errors_become_messages() {
        assert_eq!(UiError::from(InvalidQueue::NoName).text(), "A queue needs a name.");
        assert_eq!(UiError::from(BatchError::TwoAsterisks).text(), "The address may have only one *.");
        let refused = SettingsError::Refused("the settings file is read-only".into());
        assert_eq!(UiError::from(refused).to_string(), "the settings file is read-only");
    }

    /// In English each translation reads as the error's own message.
    #[test]
    fn english_matches_the_messages() {
        let batch_errors = [
            BatchError::NumberTooBig("1-99".into()),
            BatchError::PatternTooBig(10_100),
            BatchError::NotWholeNumber(Bound::From),
            BatchError::WildcardSize,
            BatchError::NumbersReversed(10, 1),
            BatchError::LettersReversed('c', 'a'),
            BatchError::RangeTooBig("0".into(), "10000".into(), 10_001),
            BatchError::ListTooLong(10_001),
        ];
        for e in &batch_errors {
            assert_eq!(batch(e), e.to_string());
        }
        let settings_errors = [
            InvalidSettings::TextSize,
            InvalidSettings::ServerConnections("a.com".into()),
            InvalidSettings::FolderNotFull(SettingsField::CategoryFolder("Music".into()), "rel".into()),
            InvalidSettings::BadFileType(SettingsField::Category(GENERAL.into()), "Zip".into()),
            InvalidSettings::GeneralName,
            InvalidSettings::ProxyNotHostPort(ProxyEntry::Socks),
            InvalidSettings::NotLoginSite("a b".into()),
            InvalidSettings::Scanner(CommandError::NoPath),
        ];
        for e in &settings_errors {
            assert_eq!(settings(e), e.to_string());
        }
        for e in [InvalidQueue::AtOnce, InvalidQueue::Retries, InvalidQueue::StartTime("25:00".into())] {
            assert_eq!(queue(&e), e.to_string());
        }
        for e in [InvalidGrab::Depth, InvalidGrab::Delay, InvalidGrab::Size, InvalidGrab::FolderNotFull("x".into())] {
            assert_eq!(grab(&e), e.to_string());
        }
    }

    /// The daemon's errors read in English as the daemon's own text, which
    /// the clients that do not translate show.
    #[test]
    fn daemon_errors_match_their_messages() {
        let failures: Vec<Failure> = vec![
            Failure::InvalidMessage("expected value".into()),
            Failure::Probe("HTTP 404".into()),
            Failure::Other("from an old daemon".into()),
            SettingsError::Invalid(InvalidSettings::TextSize).into(),
            ServiceError::CrawlRunning.into(),
            ServiceError::BadBody("http://a/".into()).into(),
            ServiceError::NotDownloadAddress("file:///x".into()).into(),
            ServiceError::Grab(InvalidGrab::Depth).into(),
            ServiceError::Confirm(ConfirmError::Download(DownloadError::AnswerNoFolder)).into(),
            ServiceError::Confirm(ConfirmError::Start(StartOrWaitError::Queue(QueueError::NoQueue(4)))).into(),
            ServiceError::Confirm(ConfirmError::Start(StartOrWaitError::Start(ControlError("busy".into())))).into(),
            QueueError::NameTaken("Night".into()).into(),
            QueueError::Invalid(InvalidQueue::NoName).into(),
            QueueError::NotQueued.into(),
            DownloadError::NoDownload(3).into(),
            DownloadError::StillRunning(3).into(),
            DownloadError::BadSpeedLimit { kbps: 1, lo: 2, hi: 3 }.into(),
            DownloadError::NotDownloadLink("javascript:".into()).into(),
            DownloadError::Exists("/d/f.zip".into()).into(),
            DownloadError::Unreachable("refused".into()).into(),
            DownloadError::Files("disk full".into()).into(),
            DownloadError::FilesStay { url: "http://a/f".into(), reason: "in use".into() }.into(),
            UpdateError::Unreachable("offline".into()).into(),
            UpdateError::Answer("no tag".into()).into(),
            UpdateError::NewestAlready.into(),
            UpdateError::Save { path: "/t/s.exe".into(), reason: "denied".into() }.into(),
            UpdateError::Start { path: "/t/s.exe".into(), reason: "denied".into() }.into(),
        ];
        for e in &failures {
            assert_eq!(daemon(e), e.to_string(), "{e:?}");
        }
        assert_eq!(UiError::from(Failure::from(QueueError::MainQueue)).text(), "The main queue cannot be deleted.");
    }
}
