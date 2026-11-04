use commercial_preflight_core::{
    CheckOutcome, CheckStatus, Date, DocumentFormat, Finding, NormalizedProposal, ScanResult,
    Severity, SourceEvidence, Verdict,
};
use commercial_preflight_policy::Policy;

pub const RULE_DISCOUNT: &str = "CP-POL-001";
pub const RULE_PAYMENT: &str = "CP-POL-002";
pub const RULE_HIDDEN_COMMERCIAL: &str = "CP-HYG-001";
pub const RULE_MATH: &str = "CP-MATH-001";
pub const RULE_PRIOR_CUSTOMER: &str = "CP-META-001";
pub const RULE_EXPIRED: &str = "CP-DATE-001";
pub const CHECK_COUNT: usize = 6;

struct Evaluation {
    status: CheckStatus,
    finding: Option<Finding>,
    reason: Option<String>,
}

impl Evaluation {
    fn pass() -> Self {
        Self {
            status: CheckStatus::Pass,
            finding: None,
            reason: None,
        }
    }

    fn finding(finding: Finding) -> Self {
        Self {
            status: CheckStatus::Finding,
            finding: Some(finding),
            reason: None,
        }
    }

    fn not_applicable(reason: impl Into<String>) -> Self {
        Self {
            status: CheckStatus::NotApplicable,
            finding: None,
            reason: Some(reason.into()),
        }
    }

    fn incomplete(reason: impl Into<String>) -> Self {
        Self {
            status: CheckStatus::Incomplete,
            finding: None,
            reason: Some(reason.into()),
        }
    }
}

pub fn evaluate(proposal: &NormalizedProposal, policy: &Policy, as_of: Date) -> ScanResult {
    let evaluations = [
        (
            RULE_DISCOUNT,
            enabled(policy, RULE_DISCOUNT, || check_discount(proposal, policy)),
        ),
        (
            RULE_PAYMENT,
            enabled(policy, RULE_PAYMENT, || check_payment(proposal, policy)),
        ),
        (
            RULE_HIDDEN_COMMERCIAL,
            enabled(policy, RULE_HIDDEN_COMMERCIAL, || {
                check_hidden_commercial(proposal, policy)
            }),
        ),
        (
            RULE_MATH,
            enabled(policy, RULE_MATH, || check_math(proposal, policy)),
        ),
        (
            RULE_PRIOR_CUSTOMER,
            enabled(policy, RULE_PRIOR_CUSTOMER, || {
                check_prior_customer(proposal, policy)
            }),
        ),
        (
            RULE_EXPIRED,
            enabled(policy, RULE_EXPIRED, || {
                check_expired(proposal, policy, as_of)
            }),
        ),
    ];

    let mut findings = evaluations
        .iter()
        .filter_map(|(_, evaluation)| evaluation.finding.clone())
        .collect::<Vec<_>>();
    findings.sort_by(|left, right| {
        rule_order(left.rule_id.as_str())
            .cmp(&rule_order(right.rule_id.as_str()))
            .then_with(|| left.rule_id.cmp(&right.rule_id))
            .then_with(|| left.evidence.cmp(&right.evidence))
    });

    let check_outcomes = evaluations
        .iter()
        .map(|(rule_id, evaluation)| CheckOutcome {
            rule_id: (*rule_id).into(),
            status: evaluation.status,
            reason: evaluation.reason.clone(),
        })
        .collect::<Vec<_>>();
    let checks_run = check_outcomes
        .iter()
        .filter(|outcome| matches!(outcome.status, CheckStatus::Pass | CheckStatus::Finding))
        .count();
    let checks_not_applicable = check_outcomes
        .iter()
        .filter(|outcome| outcome.status == CheckStatus::NotApplicable)
        .count();
    let checks_incomplete = check_outcomes
        .iter()
        .filter(|outcome| outcome.status == CheckStatus::Incomplete)
        .count();
    let incomplete_reason = (checks_incomplete > 0).then(|| {
        check_outcomes
            .iter()
            .filter(|outcome| outcome.status == CheckStatus::Incomplete)
            .map(|outcome| {
                format!(
                    "{}: {}",
                    outcome.rule_id,
                    outcome
                        .reason
                        .as_deref()
                        .unwrap_or("the check could not be completed")
                )
            })
            .collect::<Vec<_>>()
            .join("; ")
    });
    let verdict = if checks_incomplete > 0 {
        Verdict::Incomplete
    } else if findings
        .iter()
        .any(|finding| finding.severity == Severity::Critical)
    {
        Verdict::Critical
    } else if findings.is_empty() {
        Verdict::Pass
    } else {
        Verdict::Review
    };

    ScanResult {
        schema_version: "0.1".to_owned(),
        file_name: proposal.source.filename.clone(),
        verdict,
        findings,
        checks_run,
        checks_not_applicable,
        checks_incomplete,
        check_outcomes,
        as_of_date: as_of.to_string(),
        incomplete_reason,
    }
}

fn enabled(policy: &Policy, rule_id: &str, evaluate: impl FnOnce() -> Evaluation) -> Evaluation {
    if policy.rule_enabled(rule_id) {
        evaluate()
    } else {
        Evaluation::not_applicable("disabled by the active policy")
    }
}

fn check_discount(proposal: &NormalizedProposal, policy: &Policy) -> Evaluation {
    let line_discount = proposal
        .line_items
        .iter()
        .filter_map(|item| {
            item.discount_percent
                .as_ref()
                .map(|discount| (item, discount))
        })
        .max_by(|(_, left), (_, right)| left.value.total_cmp(&right.value));
    let (discount, description) = if let Some((item, discount)) = line_discount {
        (
            discount,
            item.description
                .as_ref()
                .map(|field| field.value.as_str())
                .unwrap_or("proposal line"),
        )
    } else if let Some(discount) = proposal.discount_percent.as_ref() {
        (discount, "proposal")
    } else {
        return Evaluation::incomplete("a deterministic proposal discount was not available");
    };

    if discount.value <= policy.maximum_sales_discount_pct + 1e-9 {
        return Evaluation::pass();
    }
    let legacy_evidence = format!(
        "{:.1}% discount on {}; policy maximum {:.1}%",
        discount.value, description, policy.maximum_sales_discount_pct
    );
    let mut finding = finding(
        RULE_DISCOUNT,
        policy.severity(RULE_DISCOUNT, Severity::Review),
        "Discount exceeds company policy",
        evidence_text(proposal.source.format, &discount.evidence, legacy_evidence),
        &discount.evidence,
    );
    finding.actual_value = Some(format!("{:.1}%", discount.value));
    finding.expected_value = Some(format!("At most {:.1}%", policy.maximum_sales_discount_pct));
    Evaluation::finding(finding)
}

fn check_payment(proposal: &NormalizedProposal, policy: &Policy) -> Evaluation {
    let Some(payment) = &proposal.payment_term_days else {
        return Evaluation::incomplete("the payment term could not be determined");
    };
    if payment.value <= policy.maximum_payment_term_days {
        return Evaluation::pass();
    }
    let legacy_evidence = format!(
        "{} days; policy maximum {} days",
        payment.value, policy.maximum_payment_term_days
    );
    let mut finding = finding(
        RULE_PAYMENT,
        policy.severity(RULE_PAYMENT, Severity::Review),
        "Payment term exceeds company policy",
        evidence_text(proposal.source.format, &payment.evidence, legacy_evidence),
        &payment.evidence,
    );
    finding.actual_value = Some(format!("{} days", payment.value));
    finding.expected_value = Some(format!("At most {} days", policy.maximum_payment_term_days));
    Evaluation::finding(finding)
}

fn check_hidden_commercial(proposal: &NormalizedProposal, policy: &Policy) -> Evaluation {
    if proposal.source.format != DocumentFormat::Xlsx {
        return Evaluation::not_applicable(
            "hidden worksheet inspection only applies to spreadsheet documents",
        );
    }
    let Some(signal) = proposal.document_hygiene_signals.iter().find(|signal| {
        policy
            .forbidden_outgoing_terms
            .iter()
            .any(|term| signal.searchable_text.contains(term))
    }) else {
        return Evaluation::pass();
    };
    let sheet = signal
        .evidence
        .sheet
        .as_deref()
        .unwrap_or("hidden worksheet");
    let mut finding = finding(
        RULE_HIDDEN_COMMERCIAL,
        policy.severity(RULE_HIDDEN_COMMERCIAL, Severity::Review),
        "Hidden worksheet contains commercial-sensitive content",
        format!("Hidden sheet: {sheet}"),
        &signal.evidence,
    );
    finding.actual_value = Some(format!("Hidden sheet: {sheet}"));
    finding.expected_value = Some("No hidden commercial-sensitive content".to_owned());
    Evaluation::finding(finding)
}

fn check_math(proposal: &NormalizedProposal, policy: &Policy) -> Evaluation {
    if proposal.line_items.is_empty() {
        return Evaluation::incomplete("deterministic line-item arithmetic was not available");
    }
    let mut subtotal = 0.0;
    for item in &proposal.line_items {
        let (Some(quantity), Some(unit_price), Some(discount)) = (
            item.quantity.as_ref(),
            item.unit_price.as_ref(),
            item.discount_percent.as_ref(),
        ) else {
            return Evaluation::incomplete(
                "one or more line items had ambiguous arithmetic inputs",
            );
        };
        subtotal += quantity.value * unit_price.value * (1.0 - discount.value / 100.0);
    }
    let Some(tax_percent) = &proposal.tax_percent else {
        return Evaluation::incomplete("the proposal tax rate could not be determined");
    };
    let Some(declared_total) = &proposal.grand_total else {
        return Evaluation::incomplete("the declared grand total could not be determined");
    };
    let expected_total = subtotal * (1.0 + tax_percent.value / 100.0);
    let delta = expected_total - declared_total.value;
    if delta.abs() < 0.01 {
        return Evaluation::pass();
    }
    let currency = proposal
        .currency
        .as_ref()
        .map(|field| field.value.clone())
        .unwrap_or_else(|| policy.default_currency.clone());
    let direction = if delta > 0.0 { "below" } else { "above" };
    let legacy_evidence = format!(
        "Declared total is {currency} {} {direction} calculated expected total",
        format_grouped_integer(delta.abs())
    );
    let mut finding = finding(
        RULE_MATH,
        policy.severity(RULE_MATH, Severity::Critical),
        "Declared grand total does not match calculated total",
        evidence_text(
            proposal.source.format,
            &declared_total.evidence,
            legacy_evidence,
        ),
        &declared_total.evidence,
    );
    finding.amount = Some(delta.abs());
    finding.currency = Some(currency.clone());
    finding.actual_value = Some(format!(
        "{currency} {}",
        format_grouped_number(declared_total.value)
    ));
    finding.expected_value = Some(format!(
        "{currency} {}",
        format_grouped_number(expected_total)
    ));
    Evaluation::finding(finding)
}

fn check_prior_customer(proposal: &NormalizedProposal, policy: &Policy) -> Evaluation {
    if !proposal.source.metadata_inspected {
        return Evaluation::incomplete("document metadata could not be inspected");
    }
    let Some(customer) = &proposal.customer else {
        return Evaluation::incomplete(
            "the current customer was unavailable for metadata comparison",
        );
    };
    let prior = proposal
        .previous_customer_references
        .iter()
        .map(|field| (field.value.clone(), &field.evidence))
        .next()
        .or_else(|| {
            proposal.source.metadata.iter().find_map(|metadata| {
                extract_prior_customer(&metadata.value)
                    .map(|customer| (customer, &metadata.evidence))
            })
        });
    let Some((prior_customer, evidence)) = prior else {
        return Evaluation::pass();
    };
    if customer
        .value
        .to_lowercase()
        .contains(&prior_customer.to_lowercase())
    {
        return Evaluation::pass();
    }
    let legacy_evidence = format!(
        "Workbook metadata contains {prior_customer} while proposal customer is {}",
        customer.value
    );
    let mut finding = finding(
        RULE_PRIOR_CUSTOMER,
        policy.severity(RULE_PRIOR_CUSTOMER, Severity::Review),
        "Prior-customer metadata detected",
        evidence_text(proposal.source.format, evidence, legacy_evidence),
        evidence,
    );
    finding.actual_value = Some(prior_customer);
    finding.expected_value = Some(customer.value.clone());
    Evaluation::finding(finding)
}

fn check_expired(proposal: &NormalizedProposal, policy: &Policy, as_of: Date) -> Evaluation {
    let Some(valid_until) = &proposal.valid_until else {
        return Evaluation::incomplete("the proposal validity date could not be determined");
    };
    if valid_until.value < as_of {
        let legacy_evidence = format!("Valid Until = {}; scan date = {as_of}", valid_until.value);
        let mut finding = finding(
            RULE_EXPIRED,
            policy.severity(RULE_EXPIRED, Severity::Review),
            "Quote validity has expired",
            evidence_text(
                proposal.source.format,
                &valid_until.evidence,
                legacy_evidence,
            ),
            &valid_until.evidence,
        );
        finding.actual_value = Some(valid_until.value.to_string());
        finding.expected_value = Some(format!("{as_of} or later"));
        return Evaluation::finding(finding);
    }
    if !policy.enforces_maximum_quote_validity() {
        return Evaluation::pass();
    }
    let Some(quote_date) = &proposal.quote_date else {
        return Evaluation::incomplete(
            "the quote date was not available to verify the validity duration",
        );
    };
    let duration = quote_date.value.days_until(valid_until.value);
    if duration <= i64::from(policy.maximum_quote_validity_days) {
        return Evaluation::pass();
    }
    let mut finding = finding(
        RULE_EXPIRED,
        policy.severity(RULE_EXPIRED, Severity::Review),
        "Quote validity exceeds company policy",
        evidence_text(
            proposal.source.format,
            &valid_until.evidence,
            format!(
                "Quote date = {}; valid until = {}; policy maximum {} days",
                quote_date.value, valid_until.value, policy.maximum_quote_validity_days
            ),
        ),
        &valid_until.evidence,
    );
    finding.actual_value = Some(format!("{duration} days"));
    finding.expected_value = Some(format!(
        "At most {} days",
        policy.maximum_quote_validity_days
    ));
    Evaluation::finding(finding)
}

fn evidence_text(format: DocumentFormat, evidence: &SourceEvidence, fallback: String) -> String {
    if format == DocumentFormat::Pdf {
        evidence
            .text
            .as_deref()
            .map(collapse_whitespace)
            .filter(|text| !text.is_empty())
            .unwrap_or(fallback)
    } else {
        fallback
    }
}

fn finding(
    rule_id: &'static str,
    severity: Severity,
    title: &'static str,
    evidence_text: String,
    evidence: &SourceEvidence,
) -> Finding {
    Finding {
        rule_id: rule_id.into(),
        severity,
        title: title.to_owned(),
        evidence: evidence_text,
        actual_value: None,
        expected_value: None,
        sheet: evidence.sheet.clone(),
        cell: evidence.cell.clone(),
        page: evidence.page,
        amount: None,
        currency: None,
    }
}

fn extract_prior_customer(metadata: &str) -> Option<String> {
    let lower = metadata
        .bytes()
        .map(|byte| byte.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let lower = String::from_utf8(lower).ok()?;
    if let Some(prior) = find_word(&lower, "prior") {
        let value_start = skip_ascii_whitespace(metadata, prior + "prior".len());
        if let Some(relative_end) = lower[value_start..].find(" proposal") {
            let value = metadata[value_start..value_start + relative_end].trim();
            if !value.is_empty() {
                return Some(collapse_whitespace(value));
            }
        }
    }
    for prefix in ["previous customer", "prior customer"] {
        if let Some(start) = find_word(&lower, prefix) {
            let mut value_start = skip_ascii_whitespace(metadata, start + prefix.len());
            if matches!(metadata.as_bytes().get(value_start), Some(b':' | b'-')) {
                value_start = skip_ascii_whitespace(metadata, value_start + 1);
                let value_end = metadata[value_start..]
                    .find([';', '|', ','])
                    .map(|offset| value_start + offset)
                    .unwrap_or(metadata.len());
                let value = metadata[value_start..value_end].trim();
                if !value.is_empty() {
                    return Some(collapse_whitespace(value));
                }
            }
        }
    }
    None
}

fn find_word(haystack: &str, needle: &str) -> Option<usize> {
    haystack.match_indices(needle).find_map(|(index, _)| {
        let before = index
            .checked_sub(1)
            .and_then(|at| haystack.as_bytes().get(at));
        let after = haystack.as_bytes().get(index + needle.len());
        if before.is_none_or(|byte| !is_word_byte(*byte))
            && after.is_none_or(|byte| !is_word_byte(*byte))
        {
            Some(index)
        } else {
            None
        }
    })
}

fn is_word_byte(value: u8) -> bool {
    value.is_ascii_alphanumeric() || value == b'_'
}

fn skip_ascii_whitespace(value: &str, mut index: usize) -> usize {
    while value
        .as_bytes()
        .get(index)
        .is_some_and(u8::is_ascii_whitespace)
    {
        index += 1;
    }
    index
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn format_grouped_integer(value: f64) -> String {
    let digits = format!("{:.0}", value.abs());
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(character);
    }
    grouped
}

fn format_grouped_number(value: f64) -> String {
    let fixed = format!("{value:.2}");
    let trimmed = fixed.trim_end_matches('0').trim_end_matches('.');
    let (sign, unsigned) = trimmed
        .strip_prefix('-')
        .map_or(("", trimmed), |unsigned| ("-", unsigned));
    let (integer, fraction) = unsigned
        .split_once('.')
        .map_or((unsigned, None), |(integer, fraction)| {
            (integer, Some(fraction))
        });
    let mut grouped = String::with_capacity(trimmed.len() + trimmed.len() / 3);
    grouped.push_str(sign);
    for (index, character) in integer.chars().enumerate() {
        if index > 0 && (integer.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(character);
    }
    if let Some(fraction) = fraction {
        grouped.push('.');
        grouped.push_str(fraction);
    }
    grouped
}

fn rule_order(rule_id: &str) -> u8 {
    match rule_id {
        RULE_DISCOUNT => 10,
        RULE_PAYMENT => 20,
        RULE_HIDDEN_COMMERCIAL => 30,
        RULE_MATH => 40,
        RULE_PRIOR_CUSTOMER => 50,
        RULE_EXPIRED => 60,
        _ => u8::MAX,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use commercial_preflight_core::{NormalizedProposal, ProposalField};
    use commercial_preflight_policy::{PolicyConfiguration, RuleControls};

    #[test]
    fn extracts_prior_customer_without_surrounding_metadata() {
        assert_eq!(
            extract_prior_customer("Quote | Prior Example Legacy Machinery Proposal | Internal"),
            Some("Example Legacy Machinery".to_owned())
        );
        assert_eq!(
            extract_prior_customer("Previous customer: Old Co; quote"),
            Some("Old Co".to_owned())
        );
    }

    #[test]
    fn groups_currency_amounts() {
        assert_eq!(format_grouped_integer(4_250.0), "4,250");
        assert_eq!(format_grouped_number(164_500.0), "164,500");
        assert_eq!(format_grouped_number(1_234.5), "1,234.5");
    }

    #[test]
    fn proposal_field_type_keeps_evidence_with_value() {
        let field = ProposalField::new(18.0, SourceEvidence::pdf(2, "Discount: 18%"));
        assert_eq!(field.value, 18.0);
        assert_eq!(field.evidence.page, Some(2));
    }

    #[test]
    fn database_policy_enforces_maximum_quote_validity() {
        let controls = RuleControls {
            arithmetic: false,
            discount: false,
            payment_terms: false,
            validity: true,
            customer_reference: false,
            document_hygiene: false,
        };
        let policy = Policy::configured(PolicyConfiguration {
            company_name: "Example Technology".to_owned(),
            default_currency: "EUR".to_owned(),
            minimum_gross_margin_pct: 15.0,
            maximum_sales_discount_pct: 10.0,
            maximum_payment_term_days: 60,
            maximum_quote_validity_days: 30,
            discount_severity: Severity::Review,
            payment_term_severity: Severity::Critical,
            controls,
        })
        .expect("valid policy");
        let evidence = SourceEvidence::pdf(1, "Valid Until: 2026-12-31");
        let mut proposal = NormalizedProposal::new(DocumentFormat::Pdf, "proposal.pdf");
        proposal.quote_date = Some(ProposalField::new(
            Date::new(2026, 10, 30).expect("valid date"),
            evidence.clone(),
        ));
        proposal.valid_until = Some(ProposalField::new(
            Date::new(2026, 12, 31).expect("valid date"),
            evidence,
        ));
        let result = evaluate(
            &proposal,
            &policy,
            Date::new(2026, 10, 30).expect("valid date"),
        );
        assert_eq!(result.verdict, Verdict::Review);
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].rule_id.as_str(), RULE_EXPIRED);
        assert_eq!(result.findings[0].actual_value.as_deref(), Some("62 days"));
    }
}
