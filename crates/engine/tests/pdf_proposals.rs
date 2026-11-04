use std::fs;
use std::panic::catch_unwind;
use std::path::{Path, PathBuf};

use commercial_preflight_core::{
    CheckStatus, Date, DocumentFormat, LineItem, NormalizedProposal, ProposalField, SourceEvidence,
    Verdict,
};
use commercial_preflight_engine::{scan_file, scan_file_strict, scan_proposal};
use commercial_preflight_policy::{Policy, load_policy};
use commercial_preflight_rules::{
    RULE_DISCOUNT, RULE_EXPIRED, RULE_HIDDEN_COMMERCIAL, RULE_MATH, RULE_PAYMENT,
    RULE_PRIOR_CUSTOMER,
};
use sha2::{Digest, Sha256};

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    repository_root().join("tests/fixtures").join(name)
}

fn policy() -> Policy {
    load_policy(repository_root().join("config/company_policy_orion.json"))
        .expect("regression policy loads")
}

fn as_of() -> Date {
    Date::new(2026, 10, 30).expect("fixed regression date is valid")
}

fn temporary_pdf(name: &str, bytes: &[u8]) -> PathBuf {
    let directory = repository_root()
        .join("target")
        .join("sprint-c-test-inputs");
    fs::create_dir_all(&directory).expect("test input directory can be created");
    let path = directory.join(format!("{}-{name}", std::process::id()));
    fs::write(&path, bytes).expect("temporary PDF can be written");
    path
}

fn status(result: &commercial_preflight_core::ScanResult, rule_id: &str) -> CheckStatus {
    result
        .check_outcomes
        .iter()
        .find(|outcome| outcome.rule_id.as_str() == rule_id)
        .unwrap_or_else(|| panic!("missing check outcome for {rule_id}"))
        .status
}

#[test]
fn problematic_pdf_matches_its_applicable_five_finding_contract() {
    let result = scan_file_strict(fixture("Project_Orion_Quote_v0.1.pdf"), &policy(), as_of())
        .expect("text PDF parses");
    assert_eq!(result.verdict, Verdict::Critical);
    assert_eq!(result.findings.len(), 5);
    assert_eq!(result.checks_run, 5);
    assert_eq!(result.checks_not_applicable, 1);
    assert_eq!(result.checks_incomplete, 0);
    assert_eq!(
        result
            .findings
            .iter()
            .map(|finding| finding.rule_id.as_str())
            .collect::<Vec<_>>(),
        vec![
            RULE_DISCOUNT,
            RULE_PAYMENT,
            RULE_MATH,
            RULE_PRIOR_CUSTOMER,
            RULE_EXPIRED,
        ]
    );
    assert_eq!(
        status(&result, RULE_HIDDEN_COMMERCIAL),
        CheckStatus::NotApplicable
    );
}

#[test]
fn clean_text_pdf_passes_all_applicable_checks() {
    let result = scan_file_strict(fixture("Project_Orion_Quote_clean.pdf"), &policy(), as_of())
        .expect("clean text PDF parses");
    assert_eq!(result.verdict, Verdict::Pass);
    assert!(result.findings.is_empty());
    assert_eq!(result.checks_run, 5);
    assert_eq!(result.checks_not_applicable, 1);
    assert_eq!(result.checks_incomplete, 0);
}

#[test]
fn pdf_findings_use_pdf_native_page_evidence() {
    let result = scan_file_strict(fixture("Project_Orion_Quote_v0.1.pdf"), &policy(), as_of())
        .expect("text PDF parses");
    let discount = result
        .findings
        .iter()
        .find(|finding| finding.rule_id.as_str() == RULE_DISCOUNT)
        .expect("discount finding exists");
    assert_eq!(discount.page, Some(2));
    assert!(discount.sheet.is_none());
    assert!(discount.cell.is_none());
    assert!(discount.evidence.contains("Discount: 18%"));
}

#[test]
fn scanned_and_empty_pdfs_are_incomplete_instead_of_pass() {
    for name in [
        "Project_Orion_Quote_scanned.pdf",
        "Project_Orion_Quote_empty.pdf",
    ] {
        let result = scan_file(fixture(name), &policy(), as_of());
        assert_eq!(result.verdict, Verdict::Incomplete, "{name}");
        assert!(result.findings.is_empty(), "{name}");
        assert!(
            result
                .incomplete_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("usable text layer")),
            "{name}"
        );
    }
}

#[test]
fn malformed_pdf_never_panics_and_becomes_incomplete() {
    let path = temporary_pdf(
        "malformed.pdf",
        b"%PDF-1.7\n1 0 obj << /Type /Catalog >> endobj\n",
    );
    let attempt = catch_unwind(|| scan_file(&path, &policy(), as_of()));
    let result = attempt.expect("malformed PDF must not unwind");
    assert_eq!(result.verdict, Verdict::Incomplete);
    assert!(result.incomplete_reason.is_some());
    fs::remove_file(path).expect("temporary malformed PDF can be removed");
}

#[test]
fn oversized_pdf_is_safely_incomplete() {
    let mut bytes = vec![0; commercial_preflight_pdf::MAX_PDF_BYTES as usize + 1];
    bytes[..8].copy_from_slice(b"%PDF-1.7");
    let path = temporary_pdf("oversized.pdf", &bytes);
    let result = scan_file(&path, &policy(), as_of());
    assert_eq!(result.verdict, Verdict::Incomplete);
    assert!(
        result
            .incomplete_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("safety limit"))
    );
    fs::remove_file(path).expect("temporary oversized PDF can be removed");
}

#[test]
fn unsupported_extension_is_rejected_by_the_strict_engine_boundary() {
    let error = scan_file_strict(repository_root().join("README.md"), &policy(), as_of())
        .expect_err("unsupported extension is rejected");
    assert!(matches!(
        error,
        commercial_preflight_core::CommercialPreflightError::UnsupportedDocument(_)
    ));
}

#[test]
fn scanning_pdf_preserves_its_sha256() {
    let path = fixture("Project_Orion_Quote_v0.1.pdf");
    let before = Sha256::digest(fs::read(&path).expect("PDF fixture is readable"));
    let _ = scan_file_strict(&path, &policy(), as_of()).expect("PDF scan completes");
    let after = Sha256::digest(fs::read(&path).expect("PDF fixture remains readable"));
    assert_eq!(before, after);
}

#[test]
fn scanning_xlsx_preserves_its_sha256() {
    let path = fixture("Project_Orion_Quote_v0.1.xlsx");
    let before = Sha256::digest(fs::read(&path).expect("XLSX fixture is readable"));
    let _ = scan_file_strict(&path, &policy(), as_of()).expect("XLSX scan completes");
    let after = Sha256::digest(fs::read(&path).expect("XLSX fixture remains readable"));
    assert_eq!(before, after);
}

#[test]
fn ambiguous_arithmetic_is_incomplete_and_never_a_guessed_pass() {
    let evidence = SourceEvidence::pdf(1, "Synthetic deterministic field");
    let mut proposal = NormalizedProposal::new(DocumentFormat::Pdf, "ambiguous.pdf");
    proposal.source.metadata_inspected = true;
    proposal.customer = Some(ProposalField::new(
        "Example Energy Ltd.".to_owned(),
        evidence.clone(),
    ));
    proposal.payment_term_days = Some(ProposalField::new(90, evidence.clone()));
    proposal.discount_percent = Some(ProposalField::new(18.0, evidence.clone()));
    proposal.valid_until = Some(ProposalField::new(
        Date::new(2026, 12, 31).expect("valid date"),
        evidence.clone(),
    ));
    proposal.tax_percent = Some(ProposalField::new(20.0, evidence.clone()));
    proposal.grand_total = Some(ProposalField::new(120.0, evidence.clone()));
    proposal.line_items.push(LineItem {
        sku: None,
        description: Some(ProposalField::new(
            "Ambiguous item".to_owned(),
            evidence.clone(),
        )),
        quantity: Some(ProposalField::new(1.0, evidence.clone())),
        uom: None,
        unit_price: None,
        discount_percent: Some(ProposalField::new(18.0, evidence)),
        line_total: None,
    });

    let result = scan_proposal(&proposal, &policy(), as_of());
    assert_eq!(result.verdict, Verdict::Incomplete);
    assert_eq!(status(&result, RULE_MATH), CheckStatus::Incomplete);
    assert_eq!(result.checks_incomplete, 1);
    assert_eq!(result.findings.len(), 2);
    assert!(
        result
            .findings
            .iter()
            .any(|finding| finding.rule_id.as_str() == RULE_DISCOUNT)
    );
    assert!(
        result
            .findings
            .iter()
            .any(|finding| finding.rule_id.as_str() == RULE_PAYMENT)
    );
}
