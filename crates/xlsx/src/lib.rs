use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use commercial_preflight_core::{
    Cell, CellAddress, CellValue, CommercialPreflightError, Date, DocumentFormat,
    DocumentHygieneSignal, FieldAliases, LineItem, NormalizedProposal, NormalizedWorkbook,
    ProposalField, Sheet, SheetVisibility, SourceEvidence, SourceMetadata,
};

const MAX_ARCHIVE_BYTES: u64 = 50 * 1024 * 1024;
const MAX_HELPER_OUTPUT_BYTES: usize = 128 * 1024 * 1024;
const HELPER_SCRIPT: &str = include_str!("extract.ps1");

pub fn parse_xlsx(path: impl AsRef<Path>) -> Result<NormalizedWorkbook, CommercialPreflightError> {
    let path = path.as_ref();
    validate_file(path)?;
    let protocol = run_extractor(path)?;
    parse_protocol(&protocol)
}

pub fn parse_xlsx_proposal(
    path: impl AsRef<Path>,
) -> Result<NormalizedProposal, CommercialPreflightError> {
    parse_xlsx_proposal_with_aliases(path, &FieldAliases::default())
}

pub fn parse_xlsx_proposal_with_aliases(
    path: impl AsRef<Path>,
    aliases: &FieldAliases,
) -> Result<NormalizedProposal, CommercialPreflightError> {
    let path = path.as_ref();
    let workbook = parse_xlsx(path)?;
    let filename = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    normalize_workbook_with_aliases(&workbook, filename, aliases)
}

pub fn normalize_workbook(
    workbook: &NormalizedWorkbook,
    filename: impl Into<String>,
) -> Result<NormalizedProposal, CommercialPreflightError> {
    normalize_workbook_with_aliases(workbook, filename, &FieldAliases::default())
}

pub fn normalize_workbook_with_aliases(
    workbook: &NormalizedWorkbook,
    filename: impl Into<String>,
    aliases: &FieldAliases,
) -> Result<NormalizedProposal, CommercialPreflightError> {
    let offer = workbook
        .sheets
        .iter()
        .find(|sheet| {
            sheet.visibility == SheetVisibility::Visible
                && find_label(sheet, &aliases.customer).is_some()
        })
        .ok_or_else(|| {
            CommercialPreflightError::InvalidWorkbook(
                "no visible offer worksheet containing a Customer label".to_owned(),
            )
        })?;

    let mut proposal = NormalizedProposal::new(DocumentFormat::Xlsx, filename);
    proposal.source.metadata_inspected = true;
    add_metadata(&mut proposal, "title", workbook.metadata.title.as_deref());
    add_metadata(
        &mut proposal,
        "subject",
        workbook.metadata.subject.as_deref(),
    );
    add_metadata(
        &mut proposal,
        "description",
        workbook.metadata.description.as_deref(),
    );
    add_metadata(
        &mut proposal,
        "keywords",
        workbook.metadata.keywords.as_deref(),
    );
    add_metadata(
        &mut proposal,
        "category",
        workbook.metadata.category.as_deref(),
    );

    proposal.customer = text_field(offer, &aliases.customer);
    proposal.quote_number = text_field(offer, &aliases.quote_number).or_else(|| {
        text_field(
            offer,
            &owned_labels(&["Quote Reference", "Quotation Reference", "Teklif Referansı"]),
        )
    });
    if let Some(reference) = proposal.quote_number.as_mut()
        && let Some((number, revision)) = split_quote_reference(&reference.value)
    {
        reference.value = number;
        proposal.quote_revision = Some(ProposalField::new(revision, reference.evidence.clone()));
    }
    proposal.quote_revision = text_field(
        offer,
        &owned_labels(&["Quote Revision", "Revision", "Rev", "Teklif Revizyonu"]),
    )
    .or(proposal.quote_revision);
    proposal.order_number = text_field(
        offer,
        &owned_labels(&[
            "PO Number",
            "PO No",
            "Order Number",
            "Customer PO",
            "Sipariş No",
        ]),
    );
    proposal.quote_date = date_field(offer, &owned_labels(&["Quote Date", "Quotation Date"]));
    proposal.order_date = date_field(
        offer,
        &owned_labels(&["Order Date", "PO Date", "Sipariş Tarihi"]),
    );
    proposal.valid_until = date_field(offer, &aliases.valid_until);
    proposal.payment_term_days = payment_days_field(offer, &aliases.payment_term);
    proposal.currency = text_field(offer, &owned_labels(&["Currency", "Para Birimi"]))
        .map(|field| ProposalField::new(field.value.trim().to_uppercase(), field.evidence));
    proposal.subtotal = number_field(offer, &aliases.subtotal);
    proposal.tax_percent = percent_field(offer, &aliases.tax);
    proposal.tax = number_field(offer, &aliases.tax);
    proposal.grand_total = number_field(offer, &aliases.grand_total);
    proposal.delivery_term = text_field(
        offer,
        &owned_labels(&["Delivery", "Delivery Term", "Delivery Terms", "Teslimat"]),
    );
    proposal.delivery_date = date_field(
        offer,
        &owned_labels(&["Delivery Date", "Requested Delivery Date", "Teslim Tarihi"]),
    );
    proposal.freight = number_field(
        offer,
        &owned_labels(&["Freight", "Shipping", "Freight / Shipping", "Nakliye"]),
    );
    proposal.line_items = normalize_line_items(offer, aliases);
    proposal.discount_percent = proposal
        .line_items
        .iter()
        .filter_map(|item| item.discount_percent.as_ref())
        .max_by(|left, right| left.value.total_cmp(&right.value))
        .cloned();

    proposal.document_hygiene_signals = workbook
        .sheets
        .iter()
        .filter(|sheet| sheet.visibility != SheetVisibility::Visible)
        .map(|sheet| DocumentHygieneSignal {
            searchable_text: sheet_text(sheet),
            evidence: SourceEvidence::spreadsheet(sheet.name.clone(), None),
        })
        .collect();

    Ok(proposal)
}

fn add_metadata(proposal: &mut NormalizedProposal, name: &str, value: Option<&str>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        proposal.source.metadata.push(SourceMetadata {
            name: name.to_owned(),
            value: value.to_owned(),
            evidence: SourceEvidence::metadata(None),
        });
    }
}

fn normalize(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn owned_labels(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn find_label<'a>(sheet: &'a Sheet, labels: &[String]) -> Option<&'a Cell> {
    sheet.cells.iter().find(|cell| {
        cell.value.display_text().is_some_and(|value| {
            let value = normalize(&value);
            labels.iter().any(|label| value == normalize(label))
        })
    })
}

fn right_cell<'a>(sheet: &'a Sheet, cell: &Cell) -> Option<&'a Cell> {
    sheet.cell(cell.address.row, cell.address.column.checked_add(1)?)
}

fn field_from_cell<T>(sheet: &Sheet, cell: &Cell, value: T) -> ProposalField<T> {
    ProposalField::new(
        value,
        SourceEvidence::spreadsheet(sheet.name.clone(), Some(cell.address.coordinate())),
    )
}

fn text_field(sheet: &Sheet, labels: &[String]) -> Option<ProposalField<String>> {
    let value_cell = right_cell(sheet, find_label(sheet, labels)?)?;
    let value = value_cell.value.display_text()?;
    (!value.trim().is_empty()).then(|| field_from_cell(sheet, value_cell, value.trim().to_owned()))
}

fn number_field(sheet: &Sheet, labels: &[String]) -> Option<ProposalField<f64>> {
    let value_cell = right_cell(sheet, find_label(sheet, labels)?)?;
    Some(field_from_cell(
        sheet,
        value_cell,
        value_cell.value.number()?,
    ))
}

fn percent_field(sheet: &Sheet, labels: &[String]) -> Option<ProposalField<f64>> {
    number_field(sheet, labels)
        .map(|field| ProposalField::new(percent_from_excel(field.value), field.evidence))
}

fn date_field(sheet: &Sheet, labels: &[String]) -> Option<ProposalField<Date>> {
    let value_cell = right_cell(sheet, find_label(sheet, labels)?)?;
    Some(field_from_cell(sheet, value_cell, value_cell.value.date()?))
}

fn payment_days_field(sheet: &Sheet, labels: &[String]) -> Option<ProposalField<u32>> {
    let value_cell = right_cell(sheet, find_label(sheet, labels)?)?;
    let days = match &value_cell.value {
        CellValue::Number(value) if *value >= 0.0 && *value <= f64::from(u32::MAX) => *value as u32,
        CellValue::Text(value) => digit_token(value)?,
        _ => return None,
    };
    Some(field_from_cell(sheet, value_cell, days))
}

fn digit_token(value: &str) -> Option<u32> {
    value
        .split(|character: char| !character.is_ascii_digit())
        .find(|token| (1..=4).contains(&token.len()))?
        .parse()
        .ok()
}

#[derive(Clone, Copy)]
struct LineItemTable {
    header_row: u32,
    sku_column: Option<u32>,
    description_column: u32,
    quantity_column: u32,
    uom_column: Option<u32>,
    unit_price_column: u32,
    discount_column: u32,
    line_total_column: Option<u32>,
}

fn find_line_item_table(sheet: &Sheet, aliases: &FieldAliases) -> Option<LineItemTable> {
    let max_row = sheet.max_row()?;
    let max_column = sheet.max_column()?;
    for row in 0..=max_row {
        let description_column =
            find_column(sheet, row, max_column, &["description", "item description"]);
        let quantity_column = find_column(sheet, row, max_column, &["qty", "quantity"]);
        let unit_price_column = find_column(
            sheet,
            row,
            max_column,
            &["unit price (€)", "unit price", "price"],
        );
        let discount_aliases = aliases
            .discount
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let discount_column = find_column(sheet, row, max_column, &discount_aliases);
        if let (
            Some(description_column),
            Some(quantity_column),
            Some(unit_price_column),
            Some(discount_column),
        ) = (
            description_column,
            quantity_column,
            unit_price_column,
            discount_column,
        ) {
            return Some(LineItemTable {
                header_row: row,
                sku_column: find_column(sheet, row, max_column, &["sku", "product code", "code"]),
                description_column,
                quantity_column,
                uom_column: find_column(sheet, row, max_column, &["uom", "unit"]),
                unit_price_column,
                discount_column,
                line_total_column: find_column(
                    sheet,
                    row,
                    max_column,
                    &["line total", "total price", "amount"],
                ),
            });
        }
    }
    None
}

fn find_column(sheet: &Sheet, row: u32, max_column: u32, candidates: &[&str]) -> Option<u32> {
    (0..=max_column).find(|column| {
        sheet
            .cell(row, *column)
            .and_then(|cell| cell.value.display_text())
            .is_some_and(|value| {
                let value = normalize(&value);
                candidates
                    .iter()
                    .any(|candidate| value == normalize(candidate))
            })
    })
}

fn normalize_line_items(sheet: &Sheet, aliases: &FieldAliases) -> Vec<LineItem> {
    let Some(table) = find_line_item_table(sheet, aliases) else {
        return Vec::new();
    };
    let Some(max_row) = sheet.max_row() else {
        return Vec::new();
    };
    let mut items = Vec::new();
    for row in table.header_row.saturating_add(1)..=max_row {
        let description_cell = sheet.cell(row, table.description_column);
        let quantity_cell = sheet.cell(row, table.quantity_column);
        let unit_price_cell = sheet.cell(row, table.unit_price_column);
        if description_cell.is_none() && quantity_cell.is_none() && unit_price_cell.is_none() {
            if !items.is_empty() {
                break;
            }
            continue;
        }
        let Some(quantity) = quantity_cell.and_then(|cell| cell.value.number()) else {
            if !items.is_empty() {
                break;
            }
            continue;
        };
        let Some(unit_price) = unit_price_cell.and_then(|cell| cell.value.number()) else {
            if !items.is_empty() {
                break;
            }
            continue;
        };
        let discount_cell = sheet.cell(row, table.discount_column);
        let discount = discount_cell
            .and_then(|cell| cell.value.number())
            .map(percent_from_excel)
            .unwrap_or(0.0);
        let line_total_cell = table
            .line_total_column
            .and_then(|column| sheet.cell(row, column));
        items.push(LineItem {
            sku: table
                .sku_column
                .and_then(|column| sheet.cell(row, column))
                .and_then(|cell| {
                    cell.value
                        .display_text()
                        .filter(|value| !value.trim().is_empty())
                        .map(|value| field_from_cell(sheet, cell, value.trim().to_owned()))
                }),
            description: description_cell
                .and_then(|cell| cell.value.display_text().map(|value| (cell, value)))
                .map(|(cell, value)| field_from_cell(sheet, cell, value)),
            quantity: quantity_cell.map(|cell| field_from_cell(sheet, cell, quantity)),
            uom: table
                .uom_column
                .and_then(|column| sheet.cell(row, column))
                .and_then(|cell| {
                    cell.value
                        .display_text()
                        .filter(|value| !value.trim().is_empty())
                        .map(|value| field_from_cell(sheet, cell, value.trim().to_uppercase()))
                }),
            unit_price: unit_price_cell.map(|cell| field_from_cell(sheet, cell, unit_price)),
            discount_percent: Some(field_from_cell(
                sheet,
                discount_cell.unwrap_or(unit_price_cell.expect("unit price cell is present")),
                discount,
            )),
            line_total: line_total_cell.and_then(|cell| {
                cell.value
                    .number()
                    .map(|value| field_from_cell(sheet, cell, value))
            }),
        });
    }
    items
}

fn split_quote_reference(value: &str) -> Option<(String, String)> {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
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

fn percent_from_excel(value: f64) -> f64 {
    if value.abs() <= 1.0 {
        value * 100.0
    } else {
        value
    }
}

fn sheet_text(sheet: &Sheet) -> String {
    let mut values = Vec::with_capacity(sheet.cells.len() + 1);
    values.push(sheet.name.clone());
    values.extend(
        sheet
            .cells
            .iter()
            .filter_map(|cell| cell.value.display_text()),
    );
    values.join("\n").to_lowercase()
}

fn validate_file(path: &Path) -> Result<(), CommercialPreflightError> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !extension.eq_ignore_ascii_case("xlsx") {
        return Err(CommercialPreflightError::UnsupportedWorkbook(
            "only .xlsx files are supported".to_owned(),
        ));
    }
    let metadata = fs::metadata(path)
        .map_err(|source| CommercialPreflightError::io("inspect workbook", source))?;
    if metadata.len() == 0 {
        return Err(CommercialPreflightError::InvalidWorkbook(
            "the file is empty".to_owned(),
        ));
    }
    if metadata.len() > MAX_ARCHIVE_BYTES {
        return Err(CommercialPreflightError::WorkbookLimit(format!(
            "compressed file exceeds {} MiB",
            MAX_ARCHIVE_BYTES / 1024 / 1024
        )));
    }
    Ok(())
}

#[cfg(windows)]
fn run_extractor(path: &Path) -> Result<Vec<u8>, CommercialPreflightError> {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let absolute = fs::canonicalize(path)
        .map_err(|source| CommercialPreflightError::io("resolve workbook path", source))?;
    let child = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            HELPER_SCRIPT,
        ])
        .env("COMMERCIAL_PREFLIGHT_XLSX_PATH", absolute.as_os_str())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|source| CommercialPreflightError::io("start XLSX parser", source))?;
    let output = child
        .wait_with_output()
        .map_err(|source| CommercialPreflightError::io("wait for XLSX parser", source))?;
    if !output.status.success() {
        return Err(CommercialPreflightError::InvalidWorkbook(
            safe_parser_error(&String::from_utf8_lossy(&output.stderr)),
        ));
    }
    if output.stdout.len() > MAX_HELPER_OUTPUT_BYTES {
        return Err(CommercialPreflightError::WorkbookLimit(
            "normalized workbook output is too large".to_owned(),
        ));
    }
    Ok(output.stdout)
}

#[cfg(not(windows))]
fn run_extractor(_path: &Path) -> Result<Vec<u8>, CommercialPreflightError> {
    Err(CommercialPreflightError::UnsupportedWorkbook(
        "the dependency-free XLSX parser currently requires Windows".to_owned(),
    ))
}

fn parse_protocol(bytes: &[u8]) -> Result<NormalizedWorkbook, CommercialPreflightError> {
    let text = std::str::from_utf8(bytes).map_err(|_| protocol_error())?;
    let mut lines = text.lines();
    if lines.next() != Some("PREFLIGHT1") {
        return Err(protocol_error());
    }
    let mut workbook = NormalizedWorkbook::default();
    let mut current_sheet: Option<Sheet> = None;
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        match fields.as_slice() {
            ["M", name, value] => {
                let value = decode_text(value)?;
                match *name {
                    "title" => workbook.metadata.title = Some(value),
                    "subject" => workbook.metadata.subject = Some(value),
                    "description" => workbook.metadata.description = Some(value),
                    "keywords" => workbook.metadata.keywords = Some(value),
                    "category" => workbook.metadata.category = Some(value),
                    _ => return Err(protocol_error()),
                }
            }
            ["S", visibility, name] => {
                if current_sheet.is_some() {
                    return Err(protocol_error());
                }
                current_sheet = Some(Sheet {
                    name: decode_text(name)?,
                    visibility: match *visibility {
                        "V" => SheetVisibility::Visible,
                        "H" => SheetVisibility::Hidden,
                        "X" => SheetVisibility::VeryHidden,
                        _ => return Err(protocol_error()),
                    },
                    cells: Vec::new(),
                });
            }
            ["C", row, column, kind, payload] => {
                let sheet = current_sheet.as_mut().ok_or_else(protocol_error)?;
                let row = row.parse::<u32>().map_err(|_| protocol_error())?;
                let column = column.parse::<u32>().map_err(|_| protocol_error())?;
                let payload = decode_text(payload)?;
                let value = match *kind {
                    "T" => CellValue::Text(payload),
                    "N" => CellValue::Number(payload.parse::<f64>().map_err(|_| protocol_error())?),
                    "B" => CellValue::Boolean(match payload.as_str() {
                        "1" => true,
                        "0" => false,
                        _ => return Err(protocol_error()),
                    }),
                    "D" => CellValue::Date(
                        Date::parse_supported(&payload).map_err(|_| protocol_error())?,
                    ),
                    "F" => CellValue::Formula,
                    "X" => CellValue::Error,
                    _ => return Err(protocol_error()),
                };
                sheet.cells.push(Cell {
                    address: CellAddress::new(row, column),
                    value,
                });
            }
            ["E"] => {
                let mut sheet = current_sheet.take().ok_or_else(protocol_error)?;
                sheet.cells.sort_by_key(|cell| cell.address);
                if sheet
                    .cells
                    .windows(2)
                    .any(|cells| cells[0].address == cells[1].address)
                {
                    return Err(CommercialPreflightError::InvalidWorkbook(
                        "a worksheet contains duplicate cell addresses".to_owned(),
                    ));
                }
                workbook.sheets.push(sheet);
            }
            _ => return Err(protocol_error()),
        }
    }
    if current_sheet.is_some() {
        return Err(protocol_error());
    }
    if workbook.sheets.is_empty() {
        return Err(CommercialPreflightError::InvalidWorkbook(
            "the workbook contains no worksheets".to_owned(),
        ));
    }
    Ok(workbook)
}

fn decode_text(value: &str) -> Result<String, CommercialPreflightError> {
    let bytes = decode_base64(value).ok_or_else(protocol_error)?;
    String::from_utf8(bytes).map_err(|_| protocol_error())
}

fn decode_base64(value: &str) -> Option<Vec<u8>> {
    if !value.len().is_multiple_of(4) {
        return None;
    }
    let mut output = Vec::with_capacity(value.len() / 4 * 3);
    for index in (0..value.len()).step_by(4) {
        let chunk = &value.as_bytes()[index..index + 4];
        let a = base64_value(chunk[0])?;
        let b = base64_value(chunk[1])?;
        let c = if chunk[2] == b'=' {
            64
        } else {
            base64_value(chunk[2])?
        };
        let d = if chunk[3] == b'=' {
            64
        } else {
            base64_value(chunk[3])?
        };
        if c == 64 && d != 64 {
            return None;
        }
        output.push((a << 2) | (b >> 4));
        if c != 64 {
            output.push((b << 4) | (c >> 2));
        }
        if d != 64 {
            output.push((c << 6) | d);
        }
    }
    Some(output)
}

fn base64_value(value: u8) -> Option<u8> {
    match value {
        b'A'..=b'Z' => Some(value - b'A'),
        b'a'..=b'z' => Some(value - b'a' + 26),
        b'0'..=b'9' => Some(value - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn protocol_error() -> CommercialPreflightError {
    CommercialPreflightError::InvalidWorkbook(
        "the XLSX parser returned invalid normalized data".to_owned(),
    )
}

fn safe_parser_error(error: &str) -> String {
    let normalized = error.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = normalized.chars();
    let short = chars.by_ref().take(240).collect::<String>();
    if chars.next().is_some() {
        format!("{short}…")
    } else if short.is_empty() {
        "parser rejected the workbook".to_owned()
    } else {
        short
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn base64_decoder_handles_utf8_payloads() {
        assert_eq!(
            decode_text("RXhhbXBsZSDFng==").expect("valid base64"),
            "Example Ş"
        );
        assert!(decode_text("bad").is_err());
    }

    #[test]
    fn parser_errors_are_bounded() {
        let long = "word ".repeat(100);
        assert!(safe_parser_error(&long).chars().count() <= 241);
    }

    #[test]
    fn protocol_rejects_cells_before_sheets() {
        assert!(parse_protocol(b"PREFLIGHT1\nC\t0\t0\tT\tQQ==\n").is_err());
    }

    #[test]
    fn parses_the_regression_fixture() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/Project_Orion_Quote_v0.1.xlsx");
        let output = run_extractor(&fixture).expect("extractor succeeds");
        let workbook = parse_protocol(&output).expect("protocol is valid");
        assert_eq!(workbook.sheets.len(), 3);
    }

    #[test]
    fn deterministic_turkish_aliases_normalize_company_fields() {
        let workbook = NormalizedWorkbook {
            metadata: Default::default(),
            sheets: vec![Sheet {
                name: "Teklif".to_owned(),
                visibility: SheetVisibility::Visible,
                cells: vec![
                    Cell {
                        address: CellAddress::new(0, 0),
                        value: CellValue::Text("Müşteri".to_owned()),
                    },
                    Cell {
                        address: CellAddress::new(0, 1),
                        value: CellValue::Text("Example Technology".to_owned()),
                    },
                    Cell {
                        address: CellAddress::new(1, 0),
                        value: CellValue::Text("Teklif No".to_owned()),
                    },
                    Cell {
                        address: CellAddress::new(1, 1),
                        value: CellValue::Text("T-1042".to_owned()),
                    },
                    Cell {
                        address: CellAddress::new(2, 0),
                        value: CellValue::Text("Ödeme Vadesi".to_owned()),
                    },
                    Cell {
                        address: CellAddress::new(2, 1),
                        value: CellValue::Text("60 gün".to_owned()),
                    },
                ],
            }],
        };
        let proposal =
            normalize_workbook_with_aliases(&workbook, "teklif.xlsx", &FieldAliases::default())
                .expect("deterministic aliases normalize");
        assert_eq!(
            proposal.customer.expect("customer").value,
            "Example Technology"
        );
        assert_eq!(proposal.quote_number.expect("quote number").value, "T-1042");
        assert_eq!(proposal.payment_term_days.expect("payment term").value, 60);
    }
}
