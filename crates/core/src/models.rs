use std::fmt;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct RuleId(String);

impl RuleId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for RuleId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl fmt::Display for RuleId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Critical,
    Review,
    Info,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Critical => "CRITICAL",
            Self::Review => "REVIEW",
            Self::Info => "INFO",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Pass,
    Review,
    Critical,
    Incomplete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckStatus {
    Pass,
    Finding,
    NotApplicable,
    Incomplete,
}

impl CheckStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Finding => "FINDING",
            Self::NotApplicable => "NOT_APPLICABLE",
            Self::Incomplete => "INCOMPLETE",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CheckOutcome {
    pub rule_id: RuleId,
    pub status: CheckStatus,
    pub reason: Option<String>,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Review => "REVIEW",
            Self::Critical => "CRITICAL",
            Self::Incomplete => "INCOMPLETE",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnforcementMode {
    Shadow,
    Warn,
    SoftBlock,
    HardBlock,
}

impl EnforcementMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shadow => "SHADOW",
            Self::Warn => "WARN",
            Self::SoftBlock => "SOFT_BLOCK",
            Self::HardBlock => "HARD_BLOCK",
        }
    }
}

impl fmt::Display for EnforcementMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl fmt::Display for Verdict {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    pub rule_id: RuleId,
    pub severity: Severity,
    pub title: String,
    pub evidence: String,
    pub actual_value: Option<String>,
    pub expected_value: Option<String>,
    pub sheet: Option<String>,
    pub cell: Option<String>,
    pub page: Option<u32>,
    pub amount: Option<f64>,
    pub currency: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScanResult {
    pub schema_version: String,
    pub file_name: String,
    pub verdict: Verdict,
    pub findings: Vec<Finding>,
    pub checks_run: usize,
    pub checks_not_applicable: usize,
    pub checks_incomplete: usize,
    pub check_outcomes: Vec<CheckOutcome>,
    pub as_of_date: String,
    pub incomplete_reason: Option<String>,
}

impl ScanResult {
    pub fn incomplete(
        file_name: impl Into<String>,
        as_of_date: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: "0.1".to_owned(),
            file_name: file_name.into(),
            verdict: Verdict::Incomplete,
            findings: Vec::new(),
            checks_run: 0,
            checks_not_applicable: 0,
            checks_incomplete: 1,
            check_outcomes: Vec::new(),
            as_of_date: as_of_date.into(),
            incomplete_reason: Some(reason.into()),
        }
    }

    pub fn to_json_pretty(&self) -> String {
        let mut output = String::new();
        output.push_str("{\n");
        push_json_field(&mut output, 1, "schema_version", &self.schema_version, true);
        push_json_field(&mut output, 1, "file_name", &self.file_name, true);
        push_json_field(&mut output, 1, "verdict", self.verdict.as_str(), true);
        push_number_field(
            &mut output,
            1,
            "checks_run",
            &self.checks_run.to_string(),
            true,
        );
        if self.checks_not_applicable > 0 || self.checks_incomplete > 0 {
            push_number_field(
                &mut output,
                1,
                "checks_not_applicable",
                &self.checks_not_applicable.to_string(),
                true,
            );
            push_number_field(
                &mut output,
                1,
                "checks_incomplete",
                &self.checks_incomplete.to_string(),
                true,
            );
        }
        push_json_field(&mut output, 1, "as_of_date", &self.as_of_date, true);
        output.push_str("  \"findings\": [");
        if self.findings.is_empty() {
            output.push(']');
        } else {
            output.push('\n');
            for (index, finding) in self.findings.iter().enumerate() {
                push_finding(&mut output, finding);
                if index + 1 != self.findings.len() {
                    output.push(',');
                }
                output.push('\n');
            }
            output.push_str("  ]");
        }
        if let Some(reason) = &self.incomplete_reason {
            output.push_str(",\n");
            push_json_field(&mut output, 1, "incomplete_reason", reason, false);
        }
        output.push_str("\n}");
        output
    }
}

fn push_finding(output: &mut String, finding: &Finding) {
    output.push_str("    {\n");
    push_json_field(output, 3, "rule_id", finding.rule_id.as_str(), true);
    push_json_field(output, 3, "severity", finding.severity.as_str(), true);
    push_json_field(output, 3, "title", &finding.title, true);
    let has_optional = finding.sheet.is_some()
        || finding.cell.is_some()
        || finding.page.is_some()
        || finding.amount.is_some()
        || finding.currency.is_some();
    push_json_field(output, 3, "evidence", &finding.evidence, has_optional);
    let optional_count = [
        finding.sheet.is_some(),
        finding.cell.is_some(),
        finding.page.is_some(),
        finding.amount.is_some(),
        finding.currency.is_some(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();
    let mut emitted = 0;
    if let Some(value) = &finding.sheet {
        emitted += 1;
        push_json_field(output, 3, "sheet", value, emitted != optional_count);
    }
    if let Some(value) = &finding.cell {
        emitted += 1;
        push_json_field(output, 3, "cell", value, emitted != optional_count);
    }
    if let Some(value) = finding.page {
        emitted += 1;
        push_number_field(
            output,
            3,
            "page",
            &value.to_string(),
            emitted != optional_count,
        );
    }
    if let Some(value) = finding.amount {
        emitted += 1;
        let value = if value.fract() == 0.0 {
            format!("{value:.1}")
        } else {
            value.to_string()
        };
        push_number_field(output, 3, "amount", &value, emitted != optional_count);
    }
    if let Some(value) = &finding.currency {
        push_json_field(output, 3, "currency", value, false);
    }
    output.push_str("    }");
}

fn push_json_field(output: &mut String, indent: usize, name: &str, value: &str, comma: bool) {
    output.push_str(&"  ".repeat(indent));
    push_json_string(output, name);
    output.push_str(": ");
    push_json_string(output, value);
    if comma {
        output.push(',');
    }
    output.push('\n');
}

fn push_number_field(output: &mut String, indent: usize, name: &str, value: &str, comma: bool) {
    output.push_str(&"  ".repeat(indent));
    push_json_string(output, name);
    output.push_str(": ");
    output.push_str(value);
    if comma {
        output.push(',');
    }
    output.push('\n');
}

fn push_json_string(output: &mut String, value: &str) {
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\u{08}' => output.push_str("\\b"),
            '\u{0c}' => output.push_str("\\f"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            value if value <= '\u{1f}' => {
                output.push_str(&format!("\\u{:04x}", u32::from(value)));
            }
            value => output.push(value),
        }
    }
    output.push('"');
}

#[cfg(test)]
mod json_tests {
    use super::*;

    #[test]
    fn serializes_an_empty_result_as_valid_json() {
        let result = ScanResult {
            schema_version: "0.1".to_owned(),
            file_name: "a\"b.xlsx".to_owned(),
            verdict: Verdict::Pass,
            findings: Vec::new(),
            checks_run: 6,
            checks_not_applicable: 0,
            checks_incomplete: 0,
            check_outcomes: Vec::new(),
            as_of_date: "2026-10-30".to_owned(),
            incomplete_reason: None,
        };
        assert!(crate::json::parse(result.to_json_pretty().as_bytes()).is_ok());
    }

    #[test]
    fn enforcement_modes_are_separate_from_analysis_verdicts() {
        assert_eq!(Verdict::Critical.as_str(), "CRITICAL");
        assert_eq!(EnforcementMode::Shadow.as_str(), "SHADOW");
        assert_eq!(EnforcementMode::Warn.as_str(), "WARN");
        assert_eq!(EnforcementMode::SoftBlock.as_str(), "SOFT_BLOCK");
        assert_eq!(EnforcementMode::HardBlock.as_str(), "HARD_BLOCK");
    }
}
