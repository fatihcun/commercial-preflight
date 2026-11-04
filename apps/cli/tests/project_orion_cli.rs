use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use commercial_preflight_core::json::{self, JsonValue};

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run_scan(file: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_preflight"))
        .current_dir(repository_root())
        .arg("scan")
        .arg(file)
        .arg("--policy")
        .arg("config/company_policy_orion.json")
        .arg("--as-of")
        .arg("2026-10-30")
        .arg("--json")
        .output()
        .expect("preflight CLI starts")
}

#[test]
fn project_orion_cli_reports_critical_with_six_findings_without_modifying_source() {
    let fixture = repository_root().join("tests/fixtures/Project_Orion_Quote_v0.1.xlsx");
    let before = fs::read(&fixture).expect("fixture can be read before scan");

    let output = run_scan(&fixture);

    assert_eq!(output.status.code(), Some(0));
    let result = json::parse(&output.stdout).expect("CLI emits valid JSON");
    assert_eq!(
        result.get("verdict").and_then(JsonValue::as_str),
        Some("CRITICAL")
    );
    assert_eq!(
        result.get("checks_run").and_then(JsonValue::as_f64),
        Some(6.0)
    );
    assert_eq!(
        result
            .get("findings")
            .and_then(JsonValue::as_array)
            .map(<[JsonValue]>::len),
        Some(6)
    );

    let after = fs::read(&fixture).expect("fixture can be read after scan");
    assert_eq!(
        after, before,
        "the source workbook must remain byte-for-byte unchanged"
    );
}

#[test]
fn unsupported_input_is_incomplete_and_never_passes() {
    let output = run_scan(&repository_root().join("README.md"));

    assert_eq!(output.status.code(), Some(2));
    let result = json::parse(&output.stdout).expect("CLI emits valid JSON");
    assert_eq!(
        result.get("verdict").and_then(JsonValue::as_str),
        Some("INCOMPLETE")
    );
    assert_eq!(
        result
            .get("findings")
            .and_then(JsonValue::as_array)
            .map(<[JsonValue]>::len),
        Some(0)
    );
    assert!(result.get("incomplete_reason").is_some());
}

#[test]
fn clean_fixture_passes_through_the_cli() {
    let clean = repository_root().join("tests/fixtures/Project_Orion_Quote_clean.xlsx");
    let output = run_scan(&clean);

    assert_eq!(output.status.code(), Some(0));
    let result = json::parse(&output.stdout).expect("CLI emits valid JSON");
    assert_eq!(
        result.get("verdict").and_then(JsonValue::as_str),
        Some("PASS")
    );
    assert_eq!(
        result
            .get("findings")
            .and_then(JsonValue::as_array)
            .map(<[JsonValue]>::len),
        Some(0)
    );
}
