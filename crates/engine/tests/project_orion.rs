use std::fs;
use std::path::{Path, PathBuf};

use commercial_preflight_core::json::{self, JsonValue};
use commercial_preflight_core::{Date, Severity, Verdict};
use commercial_preflight_engine::scan_file_strict;
use commercial_preflight_policy::load_policy;

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture() -> PathBuf {
    repository_root().join("tests/fixtures/Project_Orion_Quote_v0.1.xlsx")
}

fn clean_fixture() -> PathBuf {
    repository_root().join("tests/fixtures/Project_Orion_Quote_clean.xlsx")
}

fn policy_path() -> PathBuf {
    repository_root().join("config/company_policy_orion.json")
}

fn as_of() -> Date {
    Date::new(2026, 10, 30).expect("fixed regression date is valid")
}

#[test]
fn project_orion_matches_frozen_six_rule_contract() {
    let policy = load_policy(policy_path()).expect("regression policy loads");
    let result = scan_file_strict(fixture(), &policy, as_of()).expect("fixture parses");
    let expected_bytes =
        fs::read(repository_root().join("tests/fixtures/project_orion_expected_findings.json"))
            .expect("expected findings can be read");
    let expected = json::parse(&expected_bytes).expect("expected findings are JSON");

    assert_eq!(result.verdict, Verdict::Critical);
    assert_eq!(result.findings.len(), 6);
    let actual_json = json::parse(result.to_json_pretty().as_bytes()).expect("result serializes");
    let actual_findings = actual_json
        .get("findings")
        .and_then(JsonValue::as_array)
        .expect("findings are an array");
    let expected_findings = expected
        .get("expected_findings")
        .and_then(JsonValue::as_array)
        .expect("expected findings are an array");
    assert_eq!(actual_findings.len(), expected_findings.len());
    for (actual, expected) in actual_findings.iter().zip(expected_findings) {
        for field in ["rule_id", "severity", "title", "evidence"] {
            assert_eq!(
                actual.get(field),
                expected.get(field),
                "mismatch in {field}"
            );
        }
    }

    let frozen_bytes = fs::read(repository_root().join("tests/project_orion_actual.json"))
        .expect("frozen result can be read");
    let frozen_full_result = json::parse(&frozen_bytes).expect("frozen result is JSON");
    assert_eq!(actual_json, frozen_full_result);
}

#[test]
fn cleaned_project_orion_workbook_passes() {
    let policy = load_policy(policy_path()).expect("regression policy loads");
    let result = scan_file_strict(clean_fixture(), &policy, as_of()).expect("clean fixture parses");
    assert_eq!(result.verdict, Verdict::Pass);
    assert!(result.findings.is_empty());
    assert_eq!(result.checks_run, 6);
}

#[test]
fn malformed_workbook_becomes_incomplete_at_the_safe_boundary() {
    let policy = load_policy(policy_path()).expect("regression policy loads");
    let result = commercial_preflight_engine::scan_file(
        repository_root().join("README.md"),
        &policy,
        as_of(),
    );
    assert_eq!(result.verdict, Verdict::Incomplete);
    assert_eq!(result.checks_run, 0);
    assert!(result.incomplete_reason.is_some());
    assert!(result.findings.is_empty());
}

#[test]
fn project_orion_has_one_critical_finding() {
    let policy = load_policy(policy_path()).expect("regression policy loads");
    let result = scan_file_strict(fixture(), &policy, as_of()).expect("fixture parses");
    assert_eq!(
        result
            .findings
            .iter()
            .filter(|finding| finding.severity == Severity::Critical)
            .count(),
        1
    );
}
