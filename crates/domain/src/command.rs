//! The scanner command line typed in Options.

/// Why a scanner command line cannot be run.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, serde::Serialize, serde::Deserialize)]
pub enum CommandError {
    #[error("The scanner command has an unclosed quote.")]
    UnclosedQuote,
    #[error("The scanner command is empty.")]
    Empty,
    #[error("The scanner command must contain {token} where the file goes.", token = PATH_TOKEN)]
    NoPath,
}

/// Where the file to scan goes in a custom scanner command.
pub const PATH_TOKEN: &str = "{path}";

/// Split a command line into the program and its arguments. Whitespace
/// separates arguments; double quotes group, so `"C:\Program Files\x.exe"` is
/// one. Backslashes are literal, as in Windows paths. The command must name a
/// program and contain `{path}`.
pub fn parse_command(line: &str) -> Result<(String, Vec<String>), CommandError> {
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut in_token = false;
    for c in line.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                in_token = true;
            }
            c if c.is_whitespace() && !quoted => {
                if in_token {
                    parts.push(std::mem::take(&mut current));
                    in_token = false;
                }
            }
            c => {
                current.push(c);
                in_token = true;
            }
        }
    }
    if quoted {
        return Err(CommandError::UnclosedQuote);
    }
    if in_token {
        parts.push(current);
    }
    let mut parts = parts.into_iter();
    let program = parts.next().filter(|p| !p.is_empty()).ok_or(CommandError::Empty)?;
    let args: Vec<String> = parts.collect();
    if !args.iter().any(|a| a.contains(PATH_TOKEN)) {
        return Err(CommandError::NoPath);
    }
    Ok((program, args))
}

/// The scanner command line with `program` in place of the one in
/// `command`, keeping its arguments, or with `{path}` alone when it had
/// none.
pub fn with_program(program: &str, command: &str) -> String {
    let rest = match command.trim().strip_prefix('"') {
        Some(quoted) => quoted.split_once('"').map_or("", |(_, rest)| rest),
        None => command.trim().split_once(' ').map_or("", |(_, rest)| rest),
    }
    .trim();
    let rest = if rest.is_empty() { "{path}" } else { rest };
    format!("\"{program}\" {rest}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_quoted_programs_and_arguments() {
        let (prog, args) = parse_command(r#""C:\Program Files\AV\scan.exe"  /scan  "{path}" --log=C:\x"#).unwrap();
        assert_eq!(prog, r"C:\Program Files\AV\scan.exe");
        assert_eq!(args, ["/scan", "{path}", r"--log=C:\x"]);
    }

    #[test]
    fn rejects_bad_commands() {
        assert!(parse_command("").is_err());
        assert!(parse_command("   ").is_err());
        assert!(parse_command("scan.exe /all").is_err(), "no {{path}}");
        assert!(parse_command(r#""scan.exe {path}"#).is_err(), "unclosed quote");
    }
}
