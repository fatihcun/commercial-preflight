use std::path::Path;

use commercial_preflight_core::{
    CommercialPreflightError, Date, FieldAliases, NormalizedProposal, NormalizedWorkbook,
    ScanResult,
};
use commercial_preflight_pdf::{parse_pdf, parse_pdf_with_aliases};
use commercial_preflight_policy::Policy;
use commercial_preflight_rules::evaluate;
use commercial_preflight_xlsx::{
    normalize_workbook, parse_xlsx_proposal, parse_xlsx_proposal_with_aliases,
};

pub fn parse_commercial_file(
    path: impl AsRef<Path>,
    aliases: &FieldAliases,
) -> Result<NormalizedProposal, CommercialPreflightError> {
    let path = path.as_ref();
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    if extension.eq_ignore_ascii_case("xlsx") {
        parse_xlsx_proposal_with_aliases(path, aliases)
    } else if extension.eq_ignore_ascii_case("pdf") {
        parse_pdf_with_aliases(path, aliases)
    } else {
        Err(CommercialPreflightError::UnsupportedDocument(
            "only .xlsx and .pdf commercial documents are supported".to_owned(),
        ))
    }
}

pub fn scan_file(path: impl AsRef<Path>, policy: &Policy, as_of: Date) -> ScanResult {
    let path = path.as_ref();
    let file_name = display_file_name(path);
    match scan_file_strict(path, policy, as_of) {
        Ok(result) => result,
        Err(error) => {
            ScanResult::incomplete(file_name, as_of.to_string(), error.incomplete_reason())
        }
    }
}

pub fn scan_file_strict(
    path: impl AsRef<Path>,
    policy: &Policy,
    as_of: Date,
) -> Result<ScanResult, CommercialPreflightError> {
    let path = path.as_ref();
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    let proposal = if extension.eq_ignore_ascii_case("xlsx") {
        parse_xlsx_proposal(path)?
    } else if extension.eq_ignore_ascii_case("pdf") {
        parse_pdf(path)?
    } else {
        return Err(CommercialPreflightError::UnsupportedDocument(
            "only .xlsx and .pdf proposals are supported".to_owned(),
        ));
    };
    Ok(scan_proposal(&proposal, policy, as_of))
}

pub fn scan_file_with_aliases(
    path: impl AsRef<Path>,
    policy: &Policy,
    aliases: &FieldAliases,
    as_of: Date,
) -> ScanResult {
    let path = path.as_ref();
    let file_name = display_file_name(path);
    match scan_file_strict_with_aliases(path, policy, aliases, as_of) {
        Ok(result) => result,
        Err(error) => {
            ScanResult::incomplete(file_name, as_of.to_string(), error.incomplete_reason())
        }
    }
}

pub fn scan_file_strict_with_aliases(
    path: impl AsRef<Path>,
    policy: &Policy,
    aliases: &FieldAliases,
    as_of: Date,
) -> Result<ScanResult, CommercialPreflightError> {
    let path = path.as_ref();
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    let proposal = if extension.eq_ignore_ascii_case("xlsx") {
        parse_xlsx_proposal_with_aliases(path, aliases)?
    } else if extension.eq_ignore_ascii_case("pdf") {
        parse_pdf_with_aliases(path, aliases)?
    } else {
        return Err(CommercialPreflightError::UnsupportedDocument(
            "only .xlsx and .pdf proposals are supported".to_owned(),
        ));
    };
    Ok(scan_proposal(&proposal, policy, as_of))
}

pub fn scan_proposal(proposal: &NormalizedProposal, policy: &Policy, as_of: Date) -> ScanResult {
    evaluate(proposal, policy, as_of)
}

pub fn scan_workbook(
    workbook: &NormalizedWorkbook,
    policy: &Policy,
    file_name: impl Into<String>,
    as_of: Date,
) -> ScanResult {
    let file_name = file_name.into();
    match normalize_workbook(workbook, file_name.clone()) {
        Ok(proposal) => scan_proposal(&proposal, policy, as_of),
        Err(error) => ScanResult::incomplete(file_name, as_of.to_string(), error.to_string()),
    }
}

fn display_file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}
