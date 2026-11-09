//! Virus scans of finished downloads.
//!
//! The scanner runs as a program with separate arguments, never through a
//! shell. A custom command line is split into arguments first and `{path}` is
//! replaced afterwards, inside each argument, so a file name can neither add
//! arguments nor run a command, whatever characters it holds.
//!
//! Exit codes: Defender's `MpCmdRun.exe` returns 0 for no threat and 2 for a
//! threat. A custom scanner follows the common convention (ClamAV's among
//! others): 0 clean, 1 threat found. Anything else is a failed scan.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use pullbay_domain::download::Scan;
use pullbay_domain::settings::Scanner;

use pullbay_domain::command::{parse_command, PATH_TOKEN};
/// A scan that runs longer than this is stopped and counts as failed.
const SCAN_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const POLL: Duration = Duration::from_millis(200);
/// Keep this much of the scanner's output as the result's detail.
const DETAIL_CHARS: usize = 300;

/// Why no scanner command can be made.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScanError {
    #[error("Microsoft Defender is only on Windows. Choose a scanner program such as clamscan.")]
    DefenderNotHere,
    #[error(transparent)]
    Command(#[from] pullbay_domain::command::CommandError),
}

/// The program and arguments that scan `file`.
pub fn command_for(scanner: Scanner, custom: &str, file: &Path) -> Result<(PathBuf, Vec<String>), ScanError> {
    let file = file.display().to_string();
    match scanner {
        Scanner::Defender if !cfg!(windows) => Err(ScanError::DefenderNotHere),
        Scanner::Defender => {
            let base = std::env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".into());
            let exe = PathBuf::from(base).join("Windows Defender").join("MpCmdRun.exe");
            let args = ["-Scan", "-ScanType", "3", "-File", &file, "-DisableRemediation"];
            Ok((exe, args.iter().map(|a| a.to_string()).collect()))
        }
        Scanner::Custom => {
            let (program, args) = parse_command(custom)?;
            let args = args.iter().map(|a| a.replace(PATH_TOKEN, &file)).collect();
            Ok((PathBuf::from(program), args))
        }
    }
}

/// What an exit code means for `scanner`.
fn outcome(scanner: Scanner, code: Option<i32>, output: String) -> Scan {
    let threat_code = match scanner {
        Scanner::Defender => 2,
        Scanner::Custom => 1,
    };
    match code {
        Some(0) => Scan::Clean,
        Some(c) if c == threat_code => Scan::Threat { detail: output },
        Some(c) => Scan::Failed { detail: join_detail(&format!("the scanner exited with code {c}"), &output) },
        None => Scan::Failed { detail: "the scanner was stopped".to_string() },
    }
}

fn join_detail(head: &str, output: &str) -> String {
    if output.is_empty() {
        head.to_string()
    } else {
        format!("{head}: {output}")
    }
}

/// Scan `file` and wait for the result.
pub fn scan(scanner: Scanner, custom: &str, file: &Path) -> Scan {
    let (program, args) = match command_for(scanner, custom, file) {
        Ok(c) => c,
        Err(e) => return Scan::Failed { detail: e.to_string() },
    };
    let mut child = match crate::platform::no_window(Command::new(&program).args(&args))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return Scan::Failed { detail: format!("could not run {}: {e}", program.display()) },
    };

    // Drain both pipes on their own threads so a chatty scanner never blocks.
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        thread::spawn(move || {
            let mut out = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut out);
            }
            out
        })
    };
    let stdout = drain(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let stderr = drain(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));

    let deadline = Instant::now() + SCAN_TIMEOUT;
    let code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) if Instant::now() < deadline => thread::sleep(POLL),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Scan::Failed {
                    detail: format!("the scan took longer than {} minutes", SCAN_TIMEOUT.as_secs() / 60),
                };
            }
            Err(e) => return Scan::Failed { detail: format!("could not wait for the scanner: {e}") },
        }
    };
    let mut output = stdout.join().unwrap_or_default();
    output.extend(stderr.join().unwrap_or_default());
    outcome(scanner, code, tail(&String::from_utf8_lossy(&output)))
}

/// The last `DETAIL_CHARS` characters of the output, on one line.
fn tail(output: &str) -> String {
    let one_line = output.split_whitespace().collect::<Vec<_>>().join(" ");
    let count = one_line.chars().count();
    one_line.chars().skip(count.saturating_sub(DETAIL_CHARS)).collect()
}

/// The `VirusScanner` adapter: runs the scanner the settings name.
pub struct Antivirus;

impl pullbay_domain::ports::VirusScanner for Antivirus {
    fn scan(&self, settings: &pullbay_domain::settings::Settings, path: &Path) -> Scan {
        scan(settings.scanner, &settings.scanner_command, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pullbay_domain::command::with_program;

    #[test]
    fn a_chosen_program_keeps_the_arguments() {
        let clam = r"C:\ClamAV\clamscan.exe";
        assert_eq!(with_program(clam, ""), format!("\"{clam}\" {{path}}"));
        assert_eq!(with_program(clam, r#""C:\Old\scan.exe" --quiet {path}"#), format!("\"{clam}\" --quiet {{path}}"));
        assert_eq!(with_program(clam, "scan.exe /f {path}"), format!("\"{clam}\" /f {{path}}"));
        assert!(parse_command(&with_program(clam, "")).is_ok());
    }

    #[test]
    fn a_file_name_stays_one_argument() {
        // Quotes, spaces and shell operators in the name never split it.
        let file = Path::new(r#"C:\dl\a" & calc.exe & ".pdf"#);
        let (_, args) = command_for(Scanner::Custom, "scan.exe --file={path} -q", file).unwrap();
        assert_eq!(args, [format!("--file={}", file.display()), "-q".to_string()]);

        if cfg!(windows) {
            let (exe, args) = command_for(Scanner::Defender, "", file).unwrap();
            assert!(exe.ends_with(r"Windows Defender\MpCmdRun.exe"));
            assert_eq!(args[..5], ["-Scan", "-ScanType", "3", "-File", &file.display().to_string()]);
        } else {
            assert!(command_for(Scanner::Defender, "", file).is_err());
        }
    }

    #[test]
    fn exit_codes() {
        assert_eq!(outcome(Scanner::Defender, Some(0), String::new()), Scan::Clean);
        assert!(matches!(outcome(Scanner::Defender, Some(2), "x".into()), Scan::Threat { .. }));
        assert!(matches!(outcome(Scanner::Defender, Some(1), String::new()), Scan::Failed { .. }));
        assert!(matches!(outcome(Scanner::Custom, Some(1), String::new()), Scan::Threat { .. }));
        assert!(matches!(outcome(Scanner::Custom, Some(2), String::new()), Scan::Failed { .. }));
        assert!(matches!(outcome(Scanner::Custom, None, String::new()), Scan::Failed { .. }));
    }

    /// Runs real programs: `cmd.exe` (or `sh`) stands in for a scanner, so
    /// the test can pick the exit code. (Pullbay itself never runs a scanner
    /// through a shell.)
    #[test]
    fn runs_a_scanner_and_reads_its_exit_code() {
        let file = std::env::temp_dir().join("pullbay scan test.txt");
        std::fs::write(&file, b"x").unwrap();
        #[cfg(windows)]
        let run = |code: u8| {
            let cmd = std::env::var("ComSpec").unwrap_or_else(|_| r"C:\Windows\System32\cmd.exe".into());
            scan(Scanner::Custom, &format!(r#""{cmd}" /c exit {code} {{path}}"#), &file)
        };
        #[cfg(not(windows))]
        let run = |code: u8| scan(Scanner::Custom, &format!(r#"/bin/sh -c "exit {code}" sh {{path}}"#), &file);
        assert_eq!(run(0), Scan::Clean);
        assert!(matches!(run(1), Scan::Threat { .. }));
        assert!(matches!(run(7), Scan::Failed { .. }));
        let missing = scan(Scanner::Custom, r"C:\no\such\scanner.exe {path}", &file);
        assert!(matches!(missing, Scan::Failed { detail } if detail.contains("could not run")));
        std::fs::remove_file(&file).ok();
    }
}
