use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use commercial_preflight_core::{
    CommercialPreflightError, Date, DocumentFormat, FieldAliases, LineItem, NormalizedProposal,
    ProposalField, SourceEvidence, SourceMetadata,
};
use lopdf::{Document, LoadOptions, Object};

pub const MAX_PDF_BYTES: u64 = 20 * 1024 * 1024;
pub const MAX_PDF_OBJECTS: usize = 50_000;
pub const MAX_PDF_PAGES: usize = 200;
pub const MAX_STREAM_DECOMPRESSED_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_PAGE_DECOMPRESSED_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_EXTRACTED_TEXT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_METADATA_VALUE_BYTES: usize = 16 * 1024;

const NO_TEXT_LAYER_REASON: &str = "This PDF does not contain a usable text layer. OCR documents are not supported in this version.";

#[derive(Clone, Debug)]
struct PageText {
    page: u32,
    text: String,
}

#[derive(Clone, Debug)]
struct TextLine {
    page: u32,
    text: String,
}

pub fn parse_pdf(path: impl AsRef<Path>) -> Result<NormalizedProposal, CommercialPreflightError> {
    parse_pdf_with_aliases(path, &FieldAliases::default())
}

pub fn parse_pdf_with_aliases(
    path: impl AsRef<Path>,
    aliases: &FieldAliases,
) -> Result<NormalizedProposal, CommercialPreflightError> {
    let path = path.as_ref();
    validate_path(path)?;
    let bytes =
        fs::read(path).map_err(|source| CommercialPreflightError::io("read PDF", source))?;
    let filename = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    parse_pdf_bytes_with_aliases(&bytes, filename, aliases)
}

pub fn parse_pdf_bytes(
    bytes: &[u8],
    filename: impl Into<String>,
) -> Result<NormalizedProposal, CommercialPreflightError> {
    parse_pdf_bytes_with_aliases(bytes, filename, &FieldAliases::default())
}

pub fn parse_pdf_bytes_with_aliases(
    bytes: &[u8],
    filename: impl Into<String>,
    aliases: &FieldAliases,
) -> Result<NormalizedProposal, CommercialPreflightError> {
    if bytes.is_empty() {
        return Err(invalid_pdf("the file is empty"));
    }
    if bytes.len() as u64 > MAX_PDF_BYTES {
        return Err(CommercialPreflightError::PdfLimit(format!(
            "file exceeds {} MiB",
            MAX_PDF_BYTES / 1024 / 1024
        )));
    }
    if !bytes.starts_with(b"%PDF-") {
        return Err(invalid_pdf("the file signature is not PDF"));
    }

    let filename = filename.into();
    catch_unwind(AssertUnwindSafe(|| {
        parse_pdf_inner(bytes, filename, aliases)
    }))
    .unwrap_or_else(|_| {
        Err(invalid_pdf(
            "the parser stopped safely while inspecting malformed content",
        ))
    })
}

fn validate_path(path: &Path) -> Result<(), CommercialPreflightError> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !extension.eq_ignore_ascii_case("pdf") {
        return Err(CommercialPreflightError::UnsupportedDocument(
            "only .xlsx and .pdf files are supported".to_owned(),
        ));
    }
    let metadata =
        fs::metadata(path).map_err(|source| CommercialPreflightError::io("inspect PDF", source))?;
    if metadata.len() == 0 {
        return Err(invalid_pdf("the file is empty"));
    }
    if metadata.len() > MAX_PDF_BYTES {
        return Err(CommercialPreflightError::PdfLimit(format!(
            "file exceeds {} MiB",
            MAX_PDF_BYTES / 1024 / 1024
        )));
    }
    Ok(())
}

fn parse_pdf_inner(
    bytes: &[u8],
    filename: String,
    aliases: &FieldAliases,
) -> Result<NormalizedProposal, CommercialPreflightError> {
    let options = LoadOptions {
        strict: true,
        max_decompressed_size: Some(MAX_STREAM_DECOMPRESSED_BYTES),
        ..Default::default()
    };
    let document = Document::load_mem_with_options(bytes, options)
        .map_err(|error| invalid_pdf(&bounded_error(&error.to_string())))?;
    if document.objects.len() > MAX_PDF_OBJECTS {
        return Err(CommercialPreflightError::PdfLimit(format!(
            "document contains more than {MAX_PDF_OBJECTS} objects"
        )));
    }
    let pages = document.get_pages();
    if pages.is_empty() {
        return Err(invalid_pdf(NO_TEXT_LAYER_REASON));
    }
    if pages.len() > MAX_PDF_PAGES {
        return Err(CommercialPreflightError::PdfLimit(format!(
            "document contains more than {MAX_PDF_PAGES} pages"
        )));
    }

    let mut extracted_bytes = 0usize;
    let mut page_text = Vec::with_capacity(pages.len());
    for page in pages.keys().copied() {
        let text = document
            .extract_text_with_limit(&[page], MAX_PAGE_DECOMPRESSED_BYTES)
            .map_err(|error| invalid_pdf(&bounded_error(&error.to_string())))?;
        extracted_bytes = extracted_bytes.saturating_add(text.len());
        if extracted_bytes > MAX_EXTRACTED_TEXT_BYTES {
            return Err(CommercialPreflightError::PdfLimit(format!(
                "extracted text exceeds {} MiB",
                MAX_EXTRACTED_TEXT_BYTES / 1024 / 1024
            )));
        }
        page_text.push(PageText { page, text });
    }
    let meaningful_characters = page_text
        .iter()
        .flat_map(|page| page.text.chars())
        .filter(|character| character.is_alphanumeric())
        .count();
    if meaningful_characters < 20 {
        return Err(invalid_pdf(NO_TEXT_LAYER_REASON));
    }

    let lines = page_text
        .iter()
        .flat_map(|page| {
            page.text.lines().filter_map(move |line| {
                let text = collapse_whitespace(line);
                (!text.is_empty()).then_some(TextLine {
                    page: page.page,
                    text,
                })
            })
        })
        .collect::<Vec<_>>();
    let mut proposal = NormalizedProposal::new(DocumentFormat::Pdf, filename);
    proposal.source.metadata_inspected = true;
    proposal.source.metadata = extract_metadata(&document)?;
    proposal.customer = labeled_text(&lines, &aliases.customer);
    proposal.quote_number = labeled_text(&lines, &aliases.quote_number).or_else(|| {
        labeled_text(
            &lines,
            &owned_labels(&["Quote Reference", "Quotation Reference", "Teklif Referansı"]),
        )
    });
    if let Some(reference) = proposal.quote_number.as_mut()
        && let Some((number, revision)) = split_quote_reference(&reference.value)
    {
        reference.value = number;
        proposal.quote_revision = Some(ProposalField::new(revision, reference.evidence.clone()));
    }
    proposal.quote_revision = labeled_text(
        &lines,
        &owned_labels(&["Quote Revision", "Revision", "Rev", "Teklif Revizyonu"]),
    )
    .or(proposal.quote_revision);
    proposal.order_number = labeled_text(
        &lines,
        &owned_labels(&[
            "PO Number",
            "PO No",
            "Order Number",
            "Customer PO",
            "Sipariş No",
        ]),
    );
    proposal.quote_date = labeled_date(&lines, &owned_labels(&["Quote Date", "Quotation Date"]));
    proposal.order_date = labeled_date(
        &lines,
        &owned_labels(&["Order Date", "PO Date", "Sipariş Tarihi"]),
    );
    proposal.valid_until = labeled_date(&lines, &aliases.valid_until);
    proposal.payment_term_days = labeled_days(&lines, &aliases.payment_term);
    proposal.discount_percent = labeled_percent(&lines, &aliases.discount);
    proposal.currency = labeled_text(&lines, &owned_labels(&["Currency", "Para Birimi"]))
        .map(|field| ProposalField::new(field.value.to_uppercase(), field.evidence));
    proposal.subtotal = labeled_number(&lines, &aliases.subtotal);
    proposal.tax_percent = labeled_percent(&lines, &aliases.tax);
    proposal.tax = labeled_number(&lines, &aliases.tax);
    proposal.grand_total = labeled_number(&lines, &aliases.grand_total);
    proposal.delivery_term = labeled_text(
        &lines,
        &owned_labels(&["Delivery", "Delivery Term", "Delivery Terms", "Teslimat"]),
    );
    proposal.delivery_date = labeled_date(
        &lines,
        &owned_labels(&["Delivery Date", "Requested Delivery Date", "Teslim Tarihi"]),
    );
    proposal.freight = labeled_number(
        &lines,
        &owned_labels(&["Freight", "Shipping", "Freight / Shipping", "Nakliye"]),
    );
    proposal.line_items = lines.iter().filter_map(parse_line_item).collect();
    Ok(proposal)
}

fn extract_metadata(document: &Document) -> Result<Vec<SourceMetadata>, CommercialPreflightError> {
    let Some(info_object) = document.trailer.get(b"Info").ok() else {
        return Ok(Vec::new());
    };
    let info_object = match info_object {
        Object::Reference(id) => document
            .get_object(*id)
            .map_err(|error| invalid_pdf(&bounded_error(&error.to_string())))?,
        object => object,
    };
    let Ok(dictionary) = info_object.as_dict() else {
        return Ok(Vec::new());
    };
    let mut metadata = Vec::new();
    for key in [b"Title".as_slice(), b"Subject", b"Keywords", b"Author"] {
        let Ok(value) = dictionary.get(key) else {
            continue;
        };
        let Some(value) = pdf_string(value) else {
            continue;
        };
        if value.len() > MAX_METADATA_VALUE_BYTES {
            return Err(CommercialPreflightError::PdfLimit(
                "a metadata value is too large".to_owned(),
            ));
        }
        if !value.trim().is_empty() {
            let excerpt = collapse_whitespace(&value);
            metadata.push(SourceMetadata {
                name: String::from_utf8_lossy(key).into_owned(),
                value,
                evidence: SourceEvidence::metadata(Some(excerpt)),
            });
        }
    }
    Ok(metadata)
}

fn pdf_string(object: &Object) -> Option<String> {
    let bytes = object.as_str().ok()?;
    if let Some(utf16) = bytes.strip_prefix(&[0xfe, 0xff]) {
        let (chunks, remainder) = utf16.as_chunks::<2>();
        if !remainder.is_empty() {
            return None;
        }
        let units = chunks
            .iter()
            .map(|chunk| u16::from_be_bytes(*chunk))
            .collect::<Vec<_>>();
        String::from_utf16(&units).ok()
    } else {
        Some(String::from_utf8_lossy(bytes).into_owned())
    }
}

fn owned_labels(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn labeled_text(lines: &[TextLine], labels: &[String]) -> Option<ProposalField<String>> {
    let (line, value) = labeled_value(lines, labels)?;
    (!value.is_empty()).then(|| field(line, value.to_owned()))
}

fn labeled_date(lines: &[TextLine], labels: &[String]) -> Option<ProposalField<Date>> {
    let (line, value) = labeled_value(lines, labels)?;
    Some(field(line, Date::parse_supported(value).ok()?))
}

fn labeled_days(lines: &[TextLine], labels: &[String]) -> Option<ProposalField<u32>> {
    let (line, value) = labeled_value(lines, labels)?;
    let days = value
        .split(|character: char| !character.is_ascii_digit())
        .find(|token| (1..=4).contains(&token.len()))?
        .parse()
        .ok()?;
    Some(field(line, days))
}

fn labeled_percent(lines: &[TextLine], labels: &[String]) -> Option<ProposalField<f64>> {
    let (line, value) = labeled_value(lines, labels)?;
    Some(field(line, parse_number(value.trim_end_matches('%'))?))
}

fn labeled_number(lines: &[TextLine], labels: &[String]) -> Option<ProposalField<f64>> {
    let (line, value) = labeled_value(lines, labels)?;
    Some(field(line, parse_number(value)?))
}

fn labeled_value<'a>(lines: &'a [TextLine], labels: &[String]) -> Option<(&'a TextLine, &'a str)> {
    lines.iter().find_map(|line| {
        let (label, value) = line.text.split_once(':')?;
        labels
            .iter()
            .any(|candidate| label.trim().eq_ignore_ascii_case(candidate))
            .then_some((line, value.trim()))
    })
}

fn parse_line_item(line: &TextLine) -> Option<LineItem> {
    let value = line.text.strip_prefix("Item: ")?;
    let segments = value.split(" | ").collect::<Vec<_>>();
    let sku = segment_values(&segments, &["SKU", "Product Code", "Code"]);
    let description =
        segment_values(&segments, &["Description", "Item Description"]).or_else(|| {
            segments
                .iter()
                .find(|segment| !segment.contains(':'))
                .copied()
        });
    let quantity = segment_values(&segments, &["Quantity", "Qty"])?;
    let uom = segment_values(&segments, &["UOM", "Unit"]);
    let unit_price = segment_values(&segments, &["Unit Price", "Price"])?;
    let discount = segment_values(&segments, &["Discount", "Disc"]);
    let line_total = segment_values(&segments, &["Line Total", "Amount", "Total"]);
    Some(LineItem {
        sku: sku.map(|value| field(line, value.to_owned())),
        description: description.map(|value| field(line, value.trim().to_owned())),
        quantity: Some(field(line, parse_number(quantity)?)),
        uom: uom.map(|value| field(line, value.to_uppercase())),
        unit_price: Some(field(line, parse_number(unit_price)?)),
        discount_percent: discount
            .and_then(|value| parse_number(value.trim_end_matches('%')))
            .map(|value| field(line, value)),
        line_total: line_total
            .and_then(parse_number)
            .map(|value| field(line, value)),
    })
}

fn segment_values<'a>(segments: &'a [&str], labels: &[&str]) -> Option<&'a str> {
    segments.iter().find_map(|segment| {
        labels
            .iter()
            .find_map(|label| segment_value(segment, label))
    })
}

fn split_quote_reference(value: &str) -> Option<(String, String)> {
    let normalized = collapse_whitespace(value);
    for marker in [" Revision ", " REV ", " Rev ", " rev "] {
        if let Some((number, revision)) = normalized.split_once(marker) {
            let revision = revision.trim_start_matches([':', '#', '-']).trim();
            if !number.trim().is_empty() && !revision.is_empty() {
                return Some((number.trim().to_owned(), revision.to_owned()));
            }
        }
    }
    None
}

fn segment_value<'a>(segment: &'a str, label: &str) -> Option<&'a str> {
    let (actual, value) = segment.split_once(':')?;
    actual
        .trim()
        .eq_ignore_ascii_case(label)
        .then(|| value.trim())
}

fn field<T>(line: &TextLine, value: T) -> ProposalField<T> {
    ProposalField::new(value, SourceEvidence::pdf(line.page, line.text.clone()))
}

fn parse_number(value: &str) -> Option<f64> {
    let compact = value
        .trim()
        .trim_start_matches(['$', '€', '£'])
        .replace(',', "");
    let value = compact.parse::<f64>().ok()?;
    value.is_finite().then_some(value)
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn bounded_error(error: &str) -> String {
    let normalized = collapse_whitespace(error);
    let mut chars = normalized.chars();
    let short = chars.by_ref().take(240).collect::<String>();
    if chars.next().is_some() {
        format!("{short}...")
    } else if short.is_empty() {
        "the parser rejected the PDF".to_owned()
    } else {
        short
    }
}

fn invalid_pdf(reason: &str) -> CommercialPreflightError {
    CommercialPreflightError::InvalidPdf(reason.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_pdf_signatures() {
        assert!(parse_pdf_bytes(b"not a pdf", "fake.pdf").is_err());
    }

    #[test]
    fn rejects_oversized_pdf_before_parsing() {
        let mut bytes = vec![0; MAX_PDF_BYTES as usize + 1];
        bytes[..5].copy_from_slice(b"%PDF-");
        assert!(matches!(
            parse_pdf_bytes(&bytes, "large.pdf"),
            Err(CommercialPreflightError::PdfLimit(_))
        ));
    }

    #[test]
    fn malformed_pdf_is_error_safe() {
        let malformed = b"%PDF-1.7\n1 0 obj << /Type /Catalog >> endobj\n";
        let result = catch_unwind(|| parse_pdf_bytes(malformed, "malformed.pdf"));
        assert!(result.is_ok());
        assert!(result.expect("parser call completed").is_err());
    }

    #[test]
    fn parses_supported_item_lines_only() {
        let line = TextLine {
            page: 2,
            text:
                "Item: Gateway | Quantity: 2 | Unit Price: 10.00 | Discount: 5% | Line Total: 19.00"
                    .to_owned(),
        };
        let item = parse_line_item(&line).expect("supported line is deterministic");
        assert_eq!(item.quantity.expect("quantity").value, 2.0);
        assert_eq!(
            item.discount_percent.expect("discount").evidence.page,
            Some(2)
        );
    }
}
