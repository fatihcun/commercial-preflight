use std::path::Path;

#[cfg(test)]
use commercial_preflight_core::Date;
use commercial_preflight_core::{Finding, ScanResult, Verdict};
#[cfg(test)]
use commercial_preflight_engine::scan_file;
#[cfg(test)]
use commercial_preflight_policy::parse_policy;
use serde::{Deserialize, Serialize};

pub const DEFAULT_POLICY_ID: &str = "orion-default";
pub const ENFORCEMENT_MODE: &str = "SHADOW";

#[cfg(test)]
const DEFAULT_POLICY_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../config/company_policy_orion.json"
));

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanRequest {
    pub path: String,
    #[cfg_attr(not(test), allow(dead_code))]
    pub policy_id: Option<String>,
    pub as_of: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    pub state: &'static str,
    pub enforcement_mode: &'static str,
    pub supported_extensions: [&'static str; 2],
    pub default_policy_id: &'static str,
    pub default_policy_name: String,
    pub local_only: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScanResponse {
    Completed {
        result: Box<ScanResultView>,
    },
    Unsupported {
        file_name: String,
        title: &'static str,
        message: &'static str,
    },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResultView {
    pub schema_version: String,
    pub file_name: String,
    pub verdict: String,
    pub findings: Vec<FindingView>,
    pub checks_run: usize,
    pub checks_not_applicable: usize,
    pub checks_incomplete: usize,
    pub as_of_date: String,
    pub incomplete_reason: Option<String>,
    pub enforcement_mode: &'static str,
    pub scan_id: Option<String>,
    pub policy_name: Option<String>,
    pub policy_version: Option<u32>,
    pub template_name: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingView {
    pub rule_id: String,
    pub severity: String,
    pub title: String,
    pub message: String,
    pub evidence: String,
    pub actual_value: Option<String>,
    pub expected_value: Option<String>,
    pub sheet: Option<String>,
    pub cell: Option<String>,
    pub page: Option<u32>,
    pub amount: Option<f64>,
    pub currency: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutostartStatus {
    pub enabled: bool,
    pub available: bool,
    pub is_error: bool,
    pub message: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowCloseAction {
    HideToTray,
    ExitApplication,
}

pub fn app_status() -> AppStatus {
    AppStatus {
        state: "READY",
        enforcement_mode: ENFORCEMENT_MODE,
        supported_extensions: ["xlsx", "pdf"],
        default_policy_id: DEFAULT_POLICY_ID,
        default_policy_name: "Active commercial policy".to_owned(),
        local_only: true,
    }
}

#[cfg(test)]
pub fn scan_proposal(request: ScanRequest) -> ScanResponse {
    scan_proposal_with_policy(request, DEFAULT_POLICY_BYTES)
}

#[cfg(test)]
fn scan_proposal_with_policy(request: ScanRequest, policy_bytes: &[u8]) -> ScanResponse {
    let path = Path::new(request.path.trim());
    let file_name = display_file_name(path);
    if !is_supported_proposal(path) {
        return unsupported(file_name);
    }

    if request
        .policy_id
        .as_deref()
        .is_some_and(|id| id != DEFAULT_POLICY_ID)
    {
        return incomplete_view(
            file_name,
            request.as_of.as_deref().unwrap_or(""),
            "The selected company policy is not available.",
        );
    }

    let as_of = match request.as_of {
        Some(value) => match Date::parse_supported(&value) {
            Ok(date) => date,
            Err(_) => {
                return incomplete_view(
                    file_name,
                    &value,
                    "The requested check date was not valid.",
                );
            }
        },
        None => match Date::today_utc() {
            Ok(date) => date,
            Err(_) => {
                return incomplete_view(
                    file_name,
                    "",
                    "The current date could not be determined safely.",
                );
            }
        },
    };

    let policy = match parse_policy(policy_bytes) {
        Ok(policy) => policy,
        Err(_) => {
            return incomplete_view(
                file_name,
                &as_of.to_string(),
                "The configured company policy could not be loaded.",
            );
        }
    };

    let result = scan_file(path, &policy, as_of);
    ScanResponse::Completed {
        result: Box::new(result.into()),
    }
}

pub fn unexpected_scan_failure(path: &str) -> ScanResponse {
    incomplete_view(
        display_file_name(Path::new(path)),
        "",
        "Commercial Preflight could not complete this check.",
    )
}

pub fn autostart_status(result: Result<bool, String>) -> AutostartStatus {
    match result {
        Ok(enabled) => AutostartStatus {
            enabled,
            available: true,
            is_error: false,
            message: None,
        },
        Err(_) => AutostartStatus {
            enabled: false,
            available: false,
            is_error: true,
            message: Some("The Windows startup preference is temporarily unavailable."),
        },
    }
}

pub fn autostart_set_status(
    operation: Result<(), String>,
    query: impl FnOnce() -> Result<bool, String>,
) -> AutostartStatus {
    match operation {
        Ok(()) => autostart_status(query()),
        Err(_) => match query() {
            Ok(enabled) => AutostartStatus {
                enabled,
                available: true,
                is_error: true,
                message: Some(
                    "The Windows startup preference could not be changed. The previous setting was retained.",
                ),
            },
            Err(_) => AutostartStatus {
                enabled: false,
                available: false,
                is_error: true,
                message: Some("The Windows startup preference is temporarily unavailable."),
            },
        },
    }
}

pub fn close_action(explicit_quit: bool) -> WindowCloseAction {
    if explicit_quit {
        WindowCloseAction::ExitApplication
    } else {
        WindowCloseAction::HideToTray
    }
}

#[cfg(test)]
fn is_supported_proposal(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("xlsx") || extension.eq_ignore_ascii_case("pdf")
        })
}

#[cfg(test)]
fn unsupported(file_name: String) -> ScanResponse {
    ScanResponse::Unsupported {
        file_name,
        title: "Unsupported file",
        message: "Commercial Preflight currently supports XLSX and PDF proposals.",
    }
}

fn incomplete_view(file_name: String, as_of_date: &str, reason: &'static str) -> ScanResponse {
    ScanResponse::Completed {
        result: Box::new(ScanResultView {
            schema_version: "0.1".to_owned(),
            file_name,
            verdict: Verdict::Incomplete.as_str().to_owned(),
            findings: Vec::new(),
            checks_run: 0,
            checks_not_applicable: 0,
            checks_incomplete: 1,
            as_of_date: as_of_date.to_owned(),
            incomplete_reason: Some(reason.to_owned()),
            enforcement_mode: ENFORCEMENT_MODE,
            scan_id: None,
            policy_name: None,
            policy_version: None,
            template_name: None,
        }),
    }
}

fn display_file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Proposal".to_owned())
}

fn safe_incomplete_reason(reason: &str) -> String {
    if reason.contains("does not contain a usable text layer") {
        "This PDF does not contain a usable text layer. OCR documents are not supported in this version.".to_owned()
    } else if reason.starts_with("could not read")
        || reason.starts_with("could not inspect workbook")
        || reason.starts_with("could not inspect PDF")
        || reason.starts_with("could not resolve workbook path")
    {
        "The proposal could not be opened. It may have moved or you may not have access.".to_owned()
    } else if reason.starts_with("unsupported workbook")
        || reason.starts_with("unsupported document")
    {
        "This proposal uses a feature that Commercial Preflight cannot safely inspect.".to_owned()
    } else if reason.starts_with("invalid or corrupt workbook") {
        "The proposal appears damaged or is not a valid XLSX workbook.".to_owned()
    } else if reason.starts_with("workbook safety limit exceeded") {
        "The proposal is too large or complex to inspect safely.".to_owned()
    } else if reason.starts_with("invalid or uninspectable PDF") {
        "The proposal appears damaged or is not a valid inspectable PDF.".to_owned()
    } else if reason.starts_with("PDF safety limit exceeded") {
        "The PDF is too large or complex to inspect safely.".to_owned()
    } else if reason.contains("no visible offer worksheet") {
        "Required proposal fields could not be identified in the workbook.".to_owned()
    } else {
        "Commercial Preflight could not safely interpret the complete proposal.".to_owned()
    }
}

impl From<ScanResult> for ScanResultView {
    fn from(result: ScanResult) -> Self {
        Self {
            schema_version: result.schema_version,
            file_name: result.file_name,
            verdict: result.verdict.as_str().to_owned(),
            findings: result.findings.into_iter().map(Into::into).collect(),
            checks_run: result.checks_run,
            checks_not_applicable: result.checks_not_applicable,
            checks_incomplete: result.checks_incomplete,
            as_of_date: result.as_of_date,
            incomplete_reason: result
                .incomplete_reason
                .as_deref()
                .map(safe_incomplete_reason),
            enforcement_mode: ENFORCEMENT_MODE,
            scan_id: None,
            policy_name: None,
            policy_version: None,
            template_name: None,
        }
    }
}

impl From<Finding> for FindingView {
    fn from(finding: Finding) -> Self {
        let message = finding_message(finding.rule_id.as_str()).to_owned();
        Self {
            rule_id: finding.rule_id.to_string(),
            severity: finding.severity.as_str().to_owned(),
            title: finding.title,
            message,
            evidence: finding.evidence,
            actual_value: finding.actual_value,
            expected_value: finding.expected_value,
            sheet: finding.sheet,
            cell: finding.cell,
            page: finding.page,
            amount: finding.amount,
            currency: finding.currency,
        }
    }
}

fn finding_message(rule_id: &str) -> &'static str {
    match rule_id {
        "CP-POL-001" => "A proposal discount is above the configured company limit.",
        "CP-POL-002" => "The requested payment term is longer than company policy allows.",
        "CP-HYG-001" => "A hidden worksheet contains content that should not leave the company.",
        "CP-MATH-001" => {
            "The declared total differs from the total calculated from the proposal lines."
        }
        "CP-META-001" => "Document metadata refers to a different customer.",
        "CP-DATE-001" => "The proposal validity date is earlier than the check date.",
        _ => "This proposal item needs review.",
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use serde_json::Value;

    use super::*;

    fn repository_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
    }

    fn request(path: impl AsRef<Path>) -> ScanRequest {
        ScanRequest {
            path: path.as_ref().to_string_lossy().into_owned(),
            policy_id: Some(DEFAULT_POLICY_ID.to_owned()),
            as_of: Some("2026-10-30".to_owned()),
        }
    }

    fn completed(response: ScanResponse) -> ScanResultView {
        match response {
            ScanResponse::Completed { result } => *result,
            ScanResponse::Unsupported { .. } => panic!("expected a completed scan"),
        }
    }

    #[test]
    fn gui_response_is_typed_and_serializable() {
        let response = scan_proposal(request(
            repository_root().join("tests/fixtures/Project_Orion_Quote_v0.1.xlsx"),
        ));
        let value = serde_json::to_value(response).expect("GUI response serializes");
        assert_eq!(value.get("kind").and_then(Value::as_str), Some("COMPLETED"));
        assert_eq!(
            value
                .get("result")
                .and_then(|result| result.get("enforcementMode"))
                .and_then(Value::as_str),
            Some("SHADOW")
        );
        let first_finding = value
            .get("result")
            .and_then(|result| result.get("findings"))
            .and_then(Value::as_array)
            .and_then(|findings| findings.first())
            .expect("serialized response contains a finding");
        assert_eq!(
            first_finding.get("actualValue").and_then(Value::as_str),
            Some("18.0%")
        );
        assert_eq!(
            first_finding.get("expectedValue").and_then(Value::as_str),
            Some("At most 10.0%")
        );
        assert!(
            first_finding
                .get("message")
                .and_then(Value::as_str)
                .is_some()
        );
        assert!(
            first_finding
                .get("evidence")
                .and_then(Value::as_str)
                .is_some()
        );
    }

    #[test]
    fn project_orion_remains_critical_with_six_findings() {
        let result = completed(scan_proposal(request(
            repository_root().join("tests/fixtures/Project_Orion_Quote_v0.1.xlsx"),
        )));
        assert_eq!(result.verdict, "CRITICAL");
        assert_eq!(result.findings.len(), 6);
        assert_eq!(result.findings[0].rule_id, "CP-POL-001");
        assert_eq!(result.findings[5].rule_id, "CP-DATE-001");
    }

    #[test]
    fn clean_orion_remains_pass_with_no_findings() {
        let result = completed(scan_proposal(request(
            repository_root().join("tests/fixtures/Project_Orion_Quote_clean.xlsx"),
        )));
        assert_eq!(result.verdict, "PASS");
        assert!(result.findings.is_empty());
    }

    #[test]
    fn pdf_response_serializes_page_evidence_and_dynamic_coverage() {
        let response = scan_proposal(request(
            repository_root().join("tests/fixtures/Project_Orion_Quote_v0.1.pdf"),
        ));
        let value = serde_json::to_value(response).expect("PDF GUI response serializes");
        let result = value.get("result").expect("completed result exists");
        assert_eq!(result.get("checksRun").and_then(Value::as_u64), Some(5));
        assert_eq!(
            result.get("checksNotApplicable").and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            result.get("checksIncomplete").and_then(Value::as_u64),
            Some(0)
        );
        let findings = result
            .get("findings")
            .and_then(Value::as_array)
            .expect("PDF findings are serialized");
        assert!(
            findings
                .iter()
                .any(|finding| finding.get("page").and_then(Value::as_u64) == Some(2))
        );
    }

    #[test]
    fn unsupported_extension_is_rejected_before_scanning() {
        let response = scan_proposal(request(repository_root().join("missing.docx")));
        match response {
            ScanResponse::Unsupported { title, message, .. } => {
                assert_eq!(title, "Unsupported file");
                assert!(message.contains("XLSX") && message.contains("PDF"));
            }
            ScanResponse::Completed { .. } => panic!("DOCX must not reach the engine"),
        }
    }

    #[test]
    fn invalid_xlsx_path_is_incomplete_and_safe() {
        let result = completed(scan_proposal(request(
            repository_root().join("missing-proposal.xlsx"),
        )));
        assert_eq!(result.verdict, "INCOMPLETE");
        let reason = result.incomplete_reason.expect("safe reason is present");
        assert!(reason.contains("could not be opened"));
        assert!(!reason.contains("os error"));
    }

    #[test]
    fn scanning_does_not_modify_the_original_xlsx() {
        let fixture = repository_root().join("tests/fixtures/Project_Orion_Quote_v0.1.xlsx");
        let before = fs::read(&fixture).expect("fixture is readable");
        let _ = scan_proposal(request(&fixture));
        let after = fs::read(&fixture).expect("fixture remains readable");
        assert_eq!(after, before);
    }

    #[test]
    fn autostart_query_errors_become_safe_unavailable_status() {
        let status = autostart_status(Err("registry implementation detail".to_owned()));
        assert_eq!(
            status,
            AutostartStatus {
                enabled: false,
                available: false,
                is_error: true,
                message: Some("The Windows startup preference is temporarily unavailable."),
            }
        );
    }

    #[test]
    fn autostart_set_errors_preserve_the_last_known_setting() {
        let status = autostart_set_status(
            Err("registry write implementation detail".to_owned()),
            || Ok(true),
        );
        assert_eq!(
            status,
            AutostartStatus {
                enabled: true,
                available: true,
                is_error: true,
                message: Some(
                    "The Windows startup preference could not be changed. The previous setting was retained.",
                ),
            }
        );
    }

    #[test]
    fn window_close_hides_but_explicit_quit_exits() {
        assert_eq!(close_action(false), WindowCloseAction::HideToTray);
        assert_eq!(close_action(true), WindowCloseAction::ExitApplication);
    }
}
