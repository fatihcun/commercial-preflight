use crate::Date;

/// Deterministic labels recognized while normalizing a proposal.
///
/// These values are configuration, not hints: extractors only match aliases
/// after case-folding and whitespace normalization. No fuzzy inference is used.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldAliases {
    pub customer: Vec<String>,
    pub quote_number: Vec<String>,
    pub payment_term: Vec<String>,
    pub discount: Vec<String>,
    pub subtotal: Vec<String>,
    pub tax: Vec<String>,
    pub grand_total: Vec<String>,
    pub valid_until: Vec<String>,
}

impl Default for FieldAliases {
    fn default() -> Self {
        Self {
            customer: aliases(&["Customer", "Customer Name", "Müşteri", "Firma"]),
            quote_number: aliases(&[
                "Quote No",
                "Quote Number",
                "Quotation No",
                "Proposal No",
                "Teklif No",
                "Teklif Numarası",
            ]),
            payment_term: aliases(&["Payment Term", "Payment Terms", "Ödeme Vadesi", "Vade"]),
            discount: aliases(&["Discount", "Discount %", "Disc %", "İskonto", "İndirim"]),
            subtotal: aliases(&["Subtotal", "Sub Total", "Ara Toplam"]),
            tax: aliases(&[
                "VAT / Tax",
                "VAT",
                "VAT %",
                "Tax",
                "Tax %",
                "Tax Amount",
                "VAT Amount",
                "KDV",
                "KDV %",
            ]),
            grand_total: aliases(&[
                "Declared Grand Total",
                "Grand Total",
                "Total",
                "Genel Toplam",
            ]),
            valid_until: aliases(&[
                "Valid Until",
                "Validity",
                "Validity Date",
                "Valid Thru",
                "Geçerlilik Tarihi",
                "Teklif Geçerlilik Tarihi",
            ]),
        }
    }
}

fn aliases(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

impl FieldAliases {
    pub fn field(&self, key: &str) -> Option<&[String]> {
        match key {
            "customer" => Some(&self.customer),
            "quote_number" => Some(&self.quote_number),
            "payment_term" => Some(&self.payment_term),
            "discount" => Some(&self.discount),
            "subtotal" => Some(&self.subtotal),
            "tax" => Some(&self.tax),
            "grand_total" => Some(&self.grand_total),
            "valid_until" => Some(&self.valid_until),
            _ => None,
        }
    }

    pub fn field_mut(&mut self, key: &str) -> Option<&mut Vec<String>> {
        match key {
            "customer" => Some(&mut self.customer),
            "quote_number" => Some(&mut self.quote_number),
            "payment_term" => Some(&mut self.payment_term),
            "discount" => Some(&mut self.discount),
            "subtotal" => Some(&mut self.subtotal),
            "tax" => Some(&mut self.tax),
            "grand_total" => Some(&mut self.grand_total),
            "valid_until" => Some(&mut self.valid_until),
            _ => None,
        }
    }

    pub fn fields(&self) -> [(&'static str, &[String]); 8] {
        [
            ("customer", &self.customer),
            ("quote_number", &self.quote_number),
            ("payment_term", &self.payment_term),
            ("discount", &self.discount),
            ("subtotal", &self.subtotal),
            ("tax", &self.tax),
            ("grand_total", &self.grand_total),
            ("valid_until", &self.valid_until),
        ]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentFormat {
    Xlsx,
    Pdf,
}

impl DocumentFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Xlsx => "XLSX",
            Self::Pdf => "PDF",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SourceEvidence {
    pub sheet: Option<String>,
    pub cell: Option<String>,
    pub page: Option<u32>,
    pub text: Option<String>,
}

impl SourceEvidence {
    pub fn spreadsheet(sheet: impl Into<String>, cell: Option<String>) -> Self {
        Self {
            sheet: Some(sheet.into()),
            cell,
            page: None,
            text: None,
        }
    }

    pub fn pdf(page: u32, text: impl Into<String>) -> Self {
        Self {
            sheet: None,
            cell: None,
            page: Some(page),
            text: Some(text.into()),
        }
    }

    pub fn metadata(text: Option<String>) -> Self {
        Self {
            sheet: None,
            cell: None,
            page: None,
            text,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProposalField<T> {
    pub value: T,
    pub evidence: SourceEvidence,
}

impl<T> ProposalField<T> {
    pub fn new(value: T, evidence: SourceEvidence) -> Self {
        Self { value, evidence }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceMetadata {
    pub name: String,
    pub value: String,
    pub evidence: SourceEvidence,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceDocument {
    pub format: DocumentFormat,
    pub filename: String,
    pub metadata: Vec<SourceMetadata>,
    pub metadata_inspected: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LineItem {
    pub sku: Option<ProposalField<String>>,
    pub description: Option<ProposalField<String>>,
    pub quantity: Option<ProposalField<f64>>,
    pub uom: Option<ProposalField<String>>,
    pub unit_price: Option<ProposalField<f64>>,
    pub discount_percent: Option<ProposalField<f64>>,
    pub line_total: Option<ProposalField<f64>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DocumentHygieneSignal {
    pub searchable_text: String,
    pub evidence: SourceEvidence,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NormalizedProposal {
    pub source: SourceDocument,
    pub customer: Option<ProposalField<String>>,
    pub quote_number: Option<ProposalField<String>>,
    pub quote_revision: Option<ProposalField<String>>,
    pub order_number: Option<ProposalField<String>>,
    pub quote_date: Option<ProposalField<Date>>,
    pub order_date: Option<ProposalField<Date>>,
    pub valid_until: Option<ProposalField<Date>>,
    pub payment_term_days: Option<ProposalField<u32>>,
    pub discount_percent: Option<ProposalField<f64>>,
    pub currency: Option<ProposalField<String>>,
    pub line_items: Vec<LineItem>,
    pub subtotal: Option<ProposalField<f64>>,
    pub tax_percent: Option<ProposalField<f64>>,
    pub tax: Option<ProposalField<f64>>,
    pub grand_total: Option<ProposalField<f64>>,
    pub delivery_term: Option<ProposalField<String>>,
    pub delivery_date: Option<ProposalField<Date>>,
    pub freight: Option<ProposalField<f64>>,
    pub previous_customer_references: Vec<ProposalField<String>>,
    pub document_hygiene_signals: Vec<DocumentHygieneSignal>,
}

impl NormalizedProposal {
    pub fn new(format: DocumentFormat, filename: impl Into<String>) -> Self {
        Self {
            source: SourceDocument {
                format,
                filename: filename.into(),
                metadata: Vec::new(),
                metadata_inspected: false,
            },
            customer: None,
            quote_number: None,
            quote_revision: None,
            order_number: None,
            quote_date: None,
            order_date: None,
            valid_until: None,
            payment_term_days: None,
            discount_percent: None,
            currency: None,
            line_items: Vec::new(),
            subtotal: None,
            tax_percent: None,
            tax: None,
            grand_total: None,
            delivery_term: None,
            delivery_date: None,
            freight: None,
            previous_customer_references: Vec::new(),
            document_hygiene_signals: Vec::new(),
        }
    }
}
