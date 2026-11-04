use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use commercial_preflight_core::{Date, ScanResult, Verdict};
use commercial_preflight_engine::scan_file;
use commercial_preflight_policy::load_policy;

const HELP: &str = "Commercial Preflight scanner

Usage:
  preflight scan <FILE.xlsx|FILE.pdf> --policy <POLICY.json> [--as-of YYYY-MM-DD] [--json]

Options:
  --policy <PATH>       Company policy JSON (required)
  --as-of <DATE>       Scan date; defaults to the current UTC date
  --json                Emit machine-readable JSON
  -h, --help            Show this help
";

struct ScanArguments {
    file: PathBuf,
    policy: PathBuf,
    as_of: Date,
    json: bool,
}

fn main() -> ExitCode {
    match run(env::args_os().skip(1).collect()) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("preflight: {message}");
            eprintln!("Run `preflight --help` for usage.");
            ExitCode::from(2)
        }
    }
}

fn run(arguments: Vec<OsString>) -> Result<ExitCode, String> {
    if arguments
        .first()
        .is_some_and(|argument| argument == "-h" || argument == "--help")
    {
        print!("{HELP}");
        return Ok(ExitCode::SUCCESS);
    }
    let scan = parse_scan_arguments(arguments)?;
    let policy = load_policy(&scan.policy).map_err(|error| error.to_string())?;
    let result = scan_file(&scan.file, &policy, scan.as_of);
    if scan.json {
        println!("{}", result.to_json_pretty());
    } else {
        print_human_result(&result);
    }
    Ok(match result.verdict {
        Verdict::Incomplete => ExitCode::from(2),
        Verdict::Pass | Verdict::Review | Verdict::Critical => ExitCode::SUCCESS,
    })
}

fn parse_scan_arguments(arguments: Vec<OsString>) -> Result<ScanArguments, String> {
    let mut arguments = arguments.into_iter();
    let Some(command) = arguments.next() else {
        return Err("missing command".to_owned());
    };
    if command != "scan" {
        return Err(format!("unknown command `{}`", command.to_string_lossy()));
    }

    let mut file = None;
    let mut policy = None;
    let mut as_of = None;
    let mut json = false;
    while let Some(argument) = arguments.next() {
        if argument == "--policy" {
            let value = arguments
                .next()
                .ok_or_else(|| "--policy requires a path".to_owned())?;
            if policy.replace(PathBuf::from(value)).is_some() {
                return Err("--policy may only be specified once".to_owned());
            }
        } else if argument == "--as-of" {
            let value = arguments
                .next()
                .ok_or_else(|| "--as-of requires a YYYY-MM-DD date".to_owned())?;
            let parsed = value
                .to_str()
                .ok_or_else(|| "--as-of must be valid Unicode".to_owned())?
                .parse::<Date>()
                .map_err(|error| error.to_string())?;
            if as_of.replace(parsed).is_some() {
                return Err("--as-of may only be specified once".to_owned());
            }
        } else if argument == "--json" {
            if json {
                return Err("--json may only be specified once".to_owned());
            }
            json = true;
        } else if argument == "-h" || argument == "--help" {
            return Err("place --help before the command to show usage".to_owned());
        } else if argument.to_string_lossy().starts_with('-') {
            return Err(format!("unknown option `{}`", argument.to_string_lossy()));
        } else if file.replace(PathBuf::from(argument)).is_some() {
            return Err("scan accepts exactly one workbook path".to_owned());
        }
    }

    Ok(ScanArguments {
        file: file.ok_or_else(|| "scan requires a workbook path".to_owned())?,
        policy: policy.ok_or_else(|| "scan requires --policy <POLICY.json>".to_owned())?,
        as_of: match as_of {
            Some(date) => date,
            None => Date::today_utc().map_err(|error| error.to_string())?,
        },
        json,
    })
}

fn print_human_result(result: &ScanResult) {
    println!("Commercial Preflight: {}", result.verdict);
    println!("File: {}", result.file_name);
    println!("Checks: {}", result.checks_run);
    if result.checks_not_applicable > 0 {
        println!("Not applicable: {}", result.checks_not_applicable);
    }
    if result.checks_incomplete > 0 {
        println!("Incomplete: {}", result.checks_incomplete);
    }
    if let Some(reason) = &result.incomplete_reason {
        println!("Reason: {reason}");
    }
    for finding in &result.findings {
        println!(
            "[{}] {} - {}",
            severity_name(finding.severity),
            finding.rule_id,
            finding.title
        );
        println!("  {}", finding.evidence);
    }
}

fn severity_name(severity: commercial_preflight_core::Severity) -> &'static str {
    use commercial_preflight_core::Severity;
    match severity {
        Severity::Critical => "CRITICAL",
        Severity::Review => "REVIEW",
        Severity::Info => "INFO",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(value: &str) -> OsString {
        OsString::from(value)
    }

    #[test]
    fn parses_scan_arguments_in_any_supported_order() {
        let parsed = parse_scan_arguments(vec![
            os("scan"),
            os("--json"),
            os("quote.xlsx"),
            os("--as-of"),
            os("2026-10-30"),
            os("--policy"),
            os("policy.json"),
        ])
        .expect("arguments are valid");
        assert_eq!(parsed.file, PathBuf::from("quote.xlsx"));
        assert_eq!(parsed.policy, PathBuf::from("policy.json"));
        assert_eq!(parsed.as_of.to_string(), "2026-10-30");
        assert!(parsed.json);
    }

    #[test]
    fn rejects_duplicate_or_missing_required_arguments() {
        assert!(parse_scan_arguments(vec![os("scan"), os("quote.xlsx")]).is_err());
        assert!(
            parse_scan_arguments(vec![
                os("scan"),
                os("a.xlsx"),
                os("b.xlsx"),
                os("--policy"),
                os("policy.json"),
            ])
            .is_err()
        );
    }
}
