use std::collections::{BTreeMap, BTreeSet};

use commercial_preflight_core::{
    CustomerOrder, LineItem, ProposalField, Quote, QuoteLifecycle, Revision, SourceEvidence,
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ReconciliationStatus {
    Matched,
    Review,
    Critical,
    Incomplete,
}

impl ReconciliationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Matched => "MATCHED",
            Self::Review => "REVIEW",
            Self::Critical => "CRITICAL",
            Self::Incomplete => "INCOMPLETE",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DifferenceSeverity {
    Review,
    Critical,
}

impl DifferenceSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Review => "REVIEW",
            Self::Critical => "CRITICAL",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DifferenceId {
    CustomerMismatch,
    QuoteReferenceMismatch,
    QuoteRevisionMismatch,
    CurrencyMismatch,
    MissingOrderedLine,
    ExtraOrderedLine,
    QuantityChanged,
    UomChanged,
    UnitPriceChanged,
    DiscountChanged,
    LineTotalMismatch,
    SubtotalChanged,
    TaxChanged,
    GrandTotalChanged,
    PaymentTermsChanged,
    DeliveryTermsChanged,
    FreightChanged,
    AmbiguousLineMatch,
}

impl DifferenceId {
    pub const ALL: [Self; 18] = [
        Self::CustomerMismatch,
        Self::QuoteReferenceMismatch,
        Self::QuoteRevisionMismatch,
        Self::CurrencyMismatch,
        Self::MissingOrderedLine,
        Self::ExtraOrderedLine,
        Self::QuantityChanged,
        Self::UomChanged,
        Self::UnitPriceChanged,
        Self::DiscountChanged,
        Self::LineTotalMismatch,
        Self::SubtotalChanged,
        Self::TaxChanged,
        Self::GrandTotalChanged,
        Self::PaymentTermsChanged,
        Self::DeliveryTermsChanged,
        Self::FreightChanged,
        Self::AmbiguousLineMatch,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Self::CustomerMismatch => "CP-REC-001",
            Self::QuoteReferenceMismatch => "CP-REC-002",
            Self::QuoteRevisionMismatch => "CP-REC-003",
            Self::CurrencyMismatch => "CP-REC-004",
            Self::MissingOrderedLine => "CP-REC-005",
            Self::ExtraOrderedLine => "CP-REC-006",
            Self::QuantityChanged => "CP-REC-007",
            Self::UomChanged => "CP-REC-008",
            Self::UnitPriceChanged => "CP-REC-009",
            Self::DiscountChanged => "CP-REC-010",
            Self::LineTotalMismatch => "CP-REC-011",
            Self::SubtotalChanged => "CP-REC-012",
            Self::TaxChanged => "CP-REC-013",
            Self::GrandTotalChanged => "CP-REC-014",
            Self::PaymentTermsChanged => "CP-REC-015",
            Self::DeliveryTermsChanged => "CP-REC-016",
            Self::FreightChanged => "CP-REC-017",
            Self::AmbiguousLineMatch => "CP-REC-018",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::CustomerMismatch => "Customer mismatch",
            Self::QuoteReferenceMismatch => "Quote reference mismatch",
            Self::QuoteRevisionMismatch => "Quote revision mismatch",
            Self::CurrencyMismatch => "Currency mismatch",
            Self::MissingOrderedLine => "Missing ordered line",
            Self::ExtraOrderedLine => "Extra ordered line",
            Self::QuantityChanged => "Quantity changed",
            Self::UomChanged => "Unit of measure changed",
            Self::UnitPriceChanged => "Unit price changed",
            Self::DiscountChanged => "Discount changed",
            Self::LineTotalMismatch => "Line total mismatch",
            Self::SubtotalChanged => "Subtotal changed",
            Self::TaxChanged => "Tax changed",
            Self::GrandTotalChanged => "Grand total changed",
            Self::PaymentTermsChanged => "Payment terms changed",
            Self::DeliveryTermsChanged => "Delivery terms changed",
            Self::FreightChanged => "Freight changed",
            Self::AmbiguousLineMatch => "Ambiguous line match",
        }
    }

    pub fn default_severity(self) -> DifferenceSeverity {
        match self {
            Self::CustomerMismatch
            | Self::QuoteRevisionMismatch
            | Self::CurrencyMismatch
            | Self::MissingOrderedLine
            | Self::ExtraOrderedLine
            | Self::UnitPriceChanged => DifferenceSeverity::Critical,
            _ => DifferenceSeverity::Review,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReconciliationPolicy {
    severity_overrides: BTreeMap<DifferenceId, DifferenceSeverity>,
    pub amount_tolerance: f64,
    pub quantity_tolerance: f64,
}

impl Default for ReconciliationPolicy {
    fn default() -> Self {
        Self {
            severity_overrides: BTreeMap::new(),
            amount_tolerance: 0.005,
            quantity_tolerance: 0.000_001,
        }
    }
}

impl ReconciliationPolicy {
    pub fn with_severity(mut self, id: DifferenceId, severity: DifferenceSeverity) -> Self {
        self.severity_overrides.insert(id, severity);
        self
    }

    pub fn severity(&self, id: DifferenceId) -> DifferenceSeverity {
        self.severity_overrides
            .get(&id)
            .copied()
            .unwrap_or_else(|| id.default_severity())
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReconciliationEvidence {
    pub quote: Option<SourceEvidence>,
    pub order: Option<SourceEvidence>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReconciliationDifference {
    pub id: DifferenceId,
    pub severity: DifferenceSeverity,
    pub subject: String,
    pub quote_value: Option<String>,
    pub order_value: Option<String>,
    pub evidence: ReconciliationEvidence,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReconciliationResult {
    pub status: ReconciliationStatus,
    pub quote_id: String,
    pub order_id: String,
    pub differences: Vec<ReconciliationDifference>,
    pub incomplete_reasons: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssociationKind {
    ExactRevision,
    UnambiguousActive,
    OldRevision,
    Manual,
    MissingReference,
    UnknownQuote,
    UnknownRevision,
    Ambiguous,
}

impl AssociationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExactRevision => "EXACT_REVISION",
            Self::UnambiguousActive => "UNAMBIGUOUS_ACTIVE",
            Self::OldRevision => "OLD_REVISION",
            Self::Manual => "MANUAL",
            Self::MissingReference => "MISSING_REFERENCE",
            Self::UnknownQuote => "UNKNOWN_QUOTE",
            Self::UnknownRevision => "UNKNOWN_REVISION",
            Self::Ambiguous => "AMBIGUOUS",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuoteAssociation {
    pub kind: AssociationKind,
    pub quote_id: Option<String>,
    pub candidate_quote_ids: Vec<String>,
}

pub fn match_quote(order: &CustomerOrder, quotes: &[Quote]) -> QuoteAssociation {
    let Some(reference) = order.quote_reference.as_ref() else {
        return association(AssociationKind::MissingReference, None, Vec::new());
    };
    let reference = normalize_identifier(&reference.value);
    let candidates = quotes
        .iter()
        .filter(|quote| normalize_identifier(&quote.identity.quote_number) == reference)
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return association(AssociationKind::UnknownQuote, None, Vec::new());
    }
    if let Some(order_revision) = order
        .quote_revision
        .as_ref()
        .and_then(|field| Revision::new(field.value.clone()))
    {
        let matching = candidates
            .iter()
            .copied()
            .filter(|quote| {
                quote.lifecycle != QuoteLifecycle::Draft
                    && quote.identity.revision.as_ref() == Some(&order_revision)
            })
            .collect::<Vec<_>>();
        if matching.len() == 1 {
            let quote = matching[0];
            let kind = if quote.lifecycle == QuoteLifecycle::Superseded {
                AssociationKind::OldRevision
            } else {
                AssociationKind::ExactRevision
            };
            return association(kind, Some(quote.quote_id.clone()), Vec::new());
        }
        if matching.len() > 1 {
            return association(
                AssociationKind::Ambiguous,
                None,
                matching
                    .iter()
                    .map(|quote| quote.quote_id.clone())
                    .collect(),
            );
        }
        return association(
            AssociationKind::UnknownRevision,
            None,
            candidates
                .iter()
                .map(|quote| quote.quote_id.clone())
                .collect(),
        );
    }
    let active = candidates
        .iter()
        .copied()
        .filter(|quote| {
            matches!(
                quote.lifecycle,
                QuoteLifecycle::Sent | QuoteLifecycle::AcceptedBaseline
            )
        })
        .collect::<Vec<_>>();
    if active.len() == 1 {
        association(
            AssociationKind::UnambiguousActive,
            Some(active[0].quote_id.clone()),
            Vec::new(),
        )
    } else {
        association(
            AssociationKind::Ambiguous,
            None,
            active.iter().map(|quote| quote.quote_id.clone()).collect(),
        )
    }
}

fn association(
    kind: AssociationKind,
    quote_id: Option<String>,
    candidate_quote_ids: Vec<String>,
) -> QuoteAssociation {
    QuoteAssociation {
        kind,
        quote_id,
        candidate_quote_ids,
    }
}

pub fn reconcile(
    quote: &Quote,
    order: &CustomerOrder,
    policy: &ReconciliationPolicy,
) -> ReconciliationResult {
    let mut builder = ResultBuilder::new(quote, order, policy);
    builder.compare_identity();
    builder.compare_lines();
    builder.compare_terms();
    builder.finish()
}

pub fn old_revision_result(
    associated_quote: &Quote,
    order: &CustomerOrder,
    active_revision: Option<&str>,
    policy: &ReconciliationPolicy,
) -> ReconciliationResult {
    let order_revision = order
        .quote_revision
        .as_ref()
        .map(|field| field.value.clone());
    ReconciliationResult {
        status: match policy.severity(DifferenceId::QuoteRevisionMismatch) {
            DifferenceSeverity::Critical => ReconciliationStatus::Critical,
            DifferenceSeverity::Review => ReconciliationStatus::Review,
        },
        quote_id: associated_quote.quote_id.clone(),
        order_id: order.order_id.clone(),
        differences: vec![ReconciliationDifference {
            id: DifferenceId::QuoteRevisionMismatch,
            severity: policy.severity(DifferenceId::QuoteRevisionMismatch),
            subject: "Quote revision".to_owned(),
            quote_value: active_revision.map(str::to_owned),
            order_value: order_revision,
            evidence: ReconciliationEvidence {
                quote: None,
                order: order
                    .quote_revision
                    .as_ref()
                    .map(|field| field.evidence.clone()),
            },
        }],
        incomplete_reasons: Vec::new(),
    }
}

struct ResultBuilder<'a> {
    quote: &'a Quote,
    order: &'a CustomerOrder,
    policy: &'a ReconciliationPolicy,
    differences: Vec<ReconciliationDifference>,
    incomplete: BTreeSet<String>,
}

impl<'a> ResultBuilder<'a> {
    fn new(quote: &'a Quote, order: &'a CustomerOrder, policy: &'a ReconciliationPolicy) -> Self {
        Self {
            quote,
            order,
            policy,
            differences: Vec::new(),
            incomplete: BTreeSet::new(),
        }
    }

    fn compare_identity(&mut self) {
        self.compare_required_text(
            DifferenceId::CustomerMismatch,
            "Customer",
            self.quote.document.customer.as_ref(),
            self.order.document.customer.as_ref(),
        );
        self.compare_required_text(
            DifferenceId::CurrencyMismatch,
            "Currency",
            self.quote.document.currency.as_ref(),
            self.order.document.currency.as_ref(),
        );
        let order_reference = self.order.quote_reference.as_ref();
        match order_reference {
            Some(reference)
                if normalize_identifier(&reference.value)
                    != normalize_identifier(&self.quote.identity.quote_number) =>
            {
                self.push(
                    DifferenceId::QuoteReferenceMismatch,
                    "Quote reference",
                    Some(self.quote.identity.quote_number.clone()),
                    Some(reference.value.clone()),
                    None,
                    Some(reference.evidence.clone()),
                );
            }
            Some(_) => {}
            None => self.incomplete("Quote reference was not identified"),
        }

        match (
            self.quote.identity.revision.as_ref(),
            self.order
                .quote_revision
                .as_ref()
                .and_then(|field| Revision::new(field.value.clone())),
        ) {
            (Some(quote_revision), Some(order_revision)) if quote_revision != &order_revision => {
                self.push(
                    DifferenceId::QuoteRevisionMismatch,
                    "Quote revision",
                    Some(quote_revision.as_str().to_owned()),
                    Some(order_revision.as_str().to_owned()),
                    None,
                    self.order
                        .quote_revision
                        .as_ref()
                        .map(|field| field.evidence.clone()),
                );
            }
            (Some(_), None) => self.incomplete("Quote revision was not identified on the order"),
            _ => {}
        }
    }

    fn compare_lines(&mut self) {
        let quote_lines = &self.quote.document.line_items;
        let order_lines = &self.order.document.line_items;
        if quote_lines.is_empty() || order_lines.is_empty() {
            self.incomplete("Commercial line items could not be safely parsed");
            return;
        }
        let quote_has_sku = quote_lines.iter().any(|line| line.sku.is_some());
        let order_has_sku = order_lines.iter().any(|line| line.sku.is_some());
        let sku_mode = quote_has_sku || order_has_sku;
        if sku_mode
            && (!quote_lines.iter().all(|line| line.sku.is_some())
                || !order_lines.iter().all(|line| line.sku.is_some()))
        {
            self.incomplete("Mixed SKU coverage prevents safe line association");
            return;
        }
        let quote_index = line_index(quote_lines, sku_mode);
        let order_index = line_index(order_lines, sku_mode);
        if quote_index.ambiguous || order_index.ambiguous {
            self.push(
                DifferenceId::AmbiguousLineMatch,
                "Line association",
                None,
                None,
                None,
                None,
            );
            self.incomplete("Duplicate or ambiguous commercial line identifiers");
            return;
        }
        if quote_index.missing_key || order_index.missing_key {
            self.incomplete("A line lacks an unambiguous SKU or description");
            return;
        }
        for (key, quote_position) in &quote_index.positions {
            match order_index.positions.get(key) {
                Some(order_position) => self.compare_line(
                    &quote_lines[*quote_position],
                    &order_lines[*order_position],
                    line_subject(&quote_lines[*quote_position]),
                ),
                None => self.push(
                    DifferenceId::MissingOrderedLine,
                    &line_subject(&quote_lines[*quote_position]),
                    Some("Present".to_owned()),
                    Some("Missing".to_owned()),
                    line_evidence(&quote_lines[*quote_position]),
                    None,
                ),
            }
        }
        for (key, order_position) in &order_index.positions {
            if !quote_index.positions.contains_key(key) {
                self.push(
                    DifferenceId::ExtraOrderedLine,
                    &line_subject(&order_lines[*order_position]),
                    Some("Missing".to_owned()),
                    Some("Present".to_owned()),
                    None,
                    line_evidence(&order_lines[*order_position]),
                );
            }
        }
    }

    fn compare_line(&mut self, quote: &LineItem, order: &LineItem, subject: String) {
        self.compare_required_number(
            DifferenceId::QuantityChanged,
            &subject,
            quote.quantity.as_ref(),
            order.quantity.as_ref(),
            self.policy.quantity_tolerance,
        );
        self.compare_optional_text(
            DifferenceId::UomChanged,
            &subject,
            quote.uom.as_ref(),
            order.uom.as_ref(),
        );
        self.compare_required_number(
            DifferenceId::UnitPriceChanged,
            &subject,
            quote.unit_price.as_ref(),
            order.unit_price.as_ref(),
            self.policy.amount_tolerance,
        );
        self.compare_optional_number(
            DifferenceId::DiscountChanged,
            &subject,
            quote.discount_percent.as_ref(),
            order.discount_percent.as_ref(),
            self.policy.amount_tolerance,
        );
        self.compare_optional_number(
            DifferenceId::LineTotalMismatch,
            &subject,
            quote.line_total.as_ref(),
            order.line_total.as_ref(),
            self.policy.amount_tolerance,
        );
    }

    fn compare_terms(&mut self) {
        let quote = &self.quote.document.terms;
        let order = &self.order.document.terms;
        self.compare_optional_number(
            DifferenceId::SubtotalChanged,
            "Subtotal",
            quote.subtotal.as_ref(),
            order.subtotal.as_ref(),
            self.policy.amount_tolerance,
        );
        self.compare_optional_number(
            DifferenceId::TaxChanged,
            "Tax",
            quote.tax.as_ref(),
            order.tax.as_ref(),
            self.policy.amount_tolerance,
        );
        self.compare_required_number(
            DifferenceId::GrandTotalChanged,
            "Grand total",
            quote.grand_total.as_ref(),
            order.grand_total.as_ref(),
            self.policy.amount_tolerance,
        );
        self.compare_optional_u32(
            DifferenceId::PaymentTermsChanged,
            "Payment",
            quote.payment_term_days.as_ref(),
            order.payment_term_days.as_ref(),
        );
        self.compare_optional_text(
            DifferenceId::DeliveryTermsChanged,
            "Delivery",
            quote.delivery_term.as_ref(),
            order.delivery_term.as_ref(),
        );
        self.compare_optional_date(
            DifferenceId::DeliveryTermsChanged,
            "Delivery date",
            quote.delivery_date.as_ref(),
            order.delivery_date.as_ref(),
        );
        self.compare_optional_number(
            DifferenceId::FreightChanged,
            "Freight",
            quote.freight.as_ref(),
            order.freight.as_ref(),
            self.policy.amount_tolerance,
        );
    }

    fn compare_required_text(
        &mut self,
        id: DifferenceId,
        subject: &str,
        quote: Option<&ProposalField<String>>,
        order: Option<&ProposalField<String>>,
    ) {
        match (quote, order) {
            (Some(quote), Some(order))
                if normalize_text(&quote.value) != normalize_text(&order.value) =>
            {
                self.push_fields(id, subject, quote, order);
            }
            (Some(_), Some(_)) => {}
            _ => self.incomplete(format!("{subject} could not be compared")),
        }
    }

    fn compare_optional_text(
        &mut self,
        id: DifferenceId,
        subject: &str,
        quote: Option<&ProposalField<String>>,
        order: Option<&ProposalField<String>>,
    ) {
        if quote.is_none() && order.is_none() {
            return;
        }
        match (quote, order) {
            (Some(quote), Some(order))
                if normalize_text(&quote.value) == normalize_text(&order.value) => {}
            _ => self.push(
                id,
                subject,
                quote.map(|field| field.value.clone()),
                order.map(|field| field.value.clone()),
                quote.map(|field| field.evidence.clone()),
                order.map(|field| field.evidence.clone()),
            ),
        }
    }

    fn compare_required_number(
        &mut self,
        id: DifferenceId,
        subject: &str,
        quote: Option<&ProposalField<f64>>,
        order: Option<&ProposalField<f64>>,
        tolerance: f64,
    ) {
        match (quote, order) {
            (Some(quote), Some(order)) if (quote.value - order.value).abs() > tolerance => {
                self.push_number_fields(id, subject, quote, order);
            }
            (Some(_), Some(_)) => {}
            _ => self.incomplete(format!("{subject} could not be compared")),
        }
    }

    fn compare_optional_number(
        &mut self,
        id: DifferenceId,
        subject: &str,
        quote: Option<&ProposalField<f64>>,
        order: Option<&ProposalField<f64>>,
        tolerance: f64,
    ) {
        if quote.is_none() && order.is_none() {
            return;
        }
        match (quote, order) {
            (Some(quote), Some(order)) if (quote.value - order.value).abs() <= tolerance => {}
            (Some(quote), Some(order)) => self.push_number_fields(id, subject, quote, order),
            _ => self.push(
                id,
                subject,
                quote.map(|field| display_number(field.value)),
                order.map(|field| display_number(field.value)),
                quote.map(|field| field.evidence.clone()),
                order.map(|field| field.evidence.clone()),
            ),
        }
    }

    fn compare_optional_u32(
        &mut self,
        id: DifferenceId,
        subject: &str,
        quote: Option<&ProposalField<u32>>,
        order: Option<&ProposalField<u32>>,
    ) {
        if quote.is_none() && order.is_none() {
            return;
        }
        match (quote, order) {
            (Some(quote), Some(order)) if quote.value == order.value => {}
            _ => self.push(
                id,
                subject,
                quote.map(|field| format!("{} days", field.value)),
                order.map(|field| format!("{} days", field.value)),
                quote.map(|field| field.evidence.clone()),
                order.map(|field| field.evidence.clone()),
            ),
        }
    }

    fn compare_optional_date(
        &mut self,
        id: DifferenceId,
        subject: &str,
        quote: Option<&ProposalField<commercial_preflight_core::Date>>,
        order: Option<&ProposalField<commercial_preflight_core::Date>>,
    ) {
        if quote.is_none() && order.is_none() {
            return;
        }
        match (quote, order) {
            (Some(quote), Some(order)) if quote.value == order.value => {}
            _ => self.push(
                id,
                subject,
                quote.map(|field| field.value.to_string()),
                order.map(|field| field.value.to_string()),
                quote.map(|field| field.evidence.clone()),
                order.map(|field| field.evidence.clone()),
            ),
        }
    }

    fn push_fields(
        &mut self,
        id: DifferenceId,
        subject: &str,
        quote: &ProposalField<String>,
        order: &ProposalField<String>,
    ) {
        self.push(
            id,
            subject,
            Some(quote.value.clone()),
            Some(order.value.clone()),
            Some(quote.evidence.clone()),
            Some(order.evidence.clone()),
        );
    }

    fn push_number_fields(
        &mut self,
        id: DifferenceId,
        subject: &str,
        quote: &ProposalField<f64>,
        order: &ProposalField<f64>,
    ) {
        self.push(
            id,
            subject,
            Some(display_number(quote.value)),
            Some(display_number(order.value)),
            Some(quote.evidence.clone()),
            Some(order.evidence.clone()),
        );
    }

    fn push(
        &mut self,
        id: DifferenceId,
        subject: &str,
        quote_value: Option<String>,
        order_value: Option<String>,
        quote_evidence: Option<SourceEvidence>,
        order_evidence: Option<SourceEvidence>,
    ) {
        self.differences.push(ReconciliationDifference {
            id,
            severity: self.policy.severity(id),
            subject: subject.to_owned(),
            quote_value,
            order_value,
            evidence: ReconciliationEvidence {
                quote: quote_evidence,
                order: order_evidence,
            },
        });
    }

    fn incomplete(&mut self, reason: impl Into<String>) {
        self.incomplete.insert(reason.into());
    }

    fn finish(self) -> ReconciliationResult {
        let status = if !self.incomplete.is_empty() {
            ReconciliationStatus::Incomplete
        } else if self
            .differences
            .iter()
            .any(|difference| difference.severity == DifferenceSeverity::Critical)
        {
            ReconciliationStatus::Critical
        } else if self.differences.is_empty() {
            ReconciliationStatus::Matched
        } else {
            ReconciliationStatus::Review
        };
        ReconciliationResult {
            status,
            quote_id: self.quote.quote_id.clone(),
            order_id: self.order.order_id.clone(),
            differences: self.differences,
            incomplete_reasons: self.incomplete.into_iter().collect(),
        }
    }
}

struct LineIndex {
    positions: BTreeMap<String, usize>,
    ambiguous: bool,
    missing_key: bool,
}

fn line_index(lines: &[LineItem], sku_mode: bool) -> LineIndex {
    let mut positions = BTreeMap::new();
    let mut ambiguous = false;
    let mut missing_key = false;
    for (position, line) in lines.iter().enumerate() {
        let key = if sku_mode {
            line.sku
                .as_ref()
                .map(|field| normalize_identifier(&field.value))
        } else {
            line.description
                .as_ref()
                .map(|field| normalize_text(&field.value))
        };
        let Some(key) = key.filter(|key| !key.is_empty()) else {
            missing_key = true;
            continue;
        };
        if positions.insert(key, position).is_some() {
            ambiguous = true;
        }
    }
    LineIndex {
        positions,
        ambiguous,
        missing_key,
    }
}

fn line_subject(line: &LineItem) -> String {
    line.sku
        .as_ref()
        .map(|field| field.value.clone())
        .or_else(|| line.description.as_ref().map(|field| field.value.clone()))
        .unwrap_or_else(|| "Commercial line".to_owned())
}

fn line_evidence(line: &LineItem) -> Option<SourceEvidence> {
    line.sku
        .as_ref()
        .or(line.description.as_ref())
        .map(|field| field.evidence.clone())
}

fn normalize_identifier(value: &str) -> String {
    value.trim().to_uppercase()
}

fn normalize_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn display_number(value: f64) -> String {
    if value.fract().abs() < f64::EPSILON {
        format!("{value:.0}")
    } else {
        format!("{value:.4}").trim_end_matches('0').to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use commercial_preflight_core::{
        DocumentFormat, NormalizedProposal, ProposalField, QuoteLifecycle, SourceEvidence,
    };

    fn field<T>(value: T) -> ProposalField<T> {
        ProposalField::new(value, SourceEvidence::metadata(None))
    }

    fn line(sku: &str, description: &str, quantity: f64, uom: &str, price: f64) -> LineItem {
        LineItem {
            sku: Some(field(sku.to_owned())),
            description: Some(field(description.to_owned())),
            quantity: Some(field(quantity)),
            uom: Some(field(uom.to_owned())),
            unit_price: Some(field(price)),
            discount_percent: Some(field(0.0)),
            line_total: Some(field(quantity * price)),
        }
    }

    fn proposal(number: &str, revision: &str, total: f64) -> NormalizedProposal {
        let mut proposal = NormalizedProposal::new(DocumentFormat::Pdf, "synthetic.pdf");
        proposal.customer = Some(field("Example Electrical".to_owned()));
        proposal.quote_number = Some(field(number.to_owned()));
        proposal.quote_revision = Some(field(revision.to_owned()));
        proposal.currency = Some(field("EUR".to_owned()));
        proposal.payment_term_days = Some(field(30));
        proposal.delivery_term = Some(field("3 weeks".to_owned()));
        proposal.subtotal = Some(field(total));
        proposal.tax = Some(field(0.0));
        proposal.grand_total = Some(field(total));
        proposal.line_items = vec![
            line("FG-200G", "Firewall Gateway", 2.0, "EA", 8400.0),
            line("SUP-3Y", "Support 3Y", 2.0, "EA", 2100.0),
        ];
        proposal
    }

    fn quote(revision: &str, lifecycle: QuoteLifecycle) -> Quote {
        Quote::from_normalized(
            &proposal("Q-2026-184", revision, 21000.0),
            format!("quote-{revision}"),
            lifecycle,
            "quote.pdf",
            "a".repeat(64),
            1,
        )
        .expect("quote")
    }

    fn order(revision: Option<&str>) -> CustomerOrder {
        let mut proposal = proposal("Q-2026-184", revision.unwrap_or(""), 21000.0);
        proposal.order_number = Some(field("PO-45590".to_owned()));
        proposal.quote_revision = revision.map(|value| field(value.to_owned()));
        CustomerOrder::from_normalized(&proposal, "order-1", "po.pdf", "b".repeat(64), 2)
    }

    fn ids(result: &ReconciliationResult) -> BTreeSet<DifferenceId> {
        result
            .differences
            .iter()
            .map(|difference| difference.id)
            .collect()
    }

    #[test]
    fn taxonomy_codes_are_stable_and_unique() {
        let codes = DifferenceId::ALL.map(DifferenceId::code);
        assert_eq!(codes[0], "CP-REC-001");
        assert_eq!(codes[17], "CP-REC-018");
        assert_eq!(codes.into_iter().collect::<BTreeSet<_>>().len(), 18);
    }

    #[test]
    fn exact_reference_and_revision_match() {
        let quote = quote("3", QuoteLifecycle::Sent);
        let association = match_quote(&order(Some("Rev 3")), &[quote]);
        assert_eq!(association.kind, AssociationKind::ExactRevision);
    }

    #[test]
    fn old_revision_is_explicitly_detected() {
        let old = quote("2", QuoteLifecycle::Superseded);
        let current = quote("3", QuoteLifecycle::Sent);
        let association = match_quote(&order(Some("2")), &[old, current]);
        assert_eq!(association.kind, AssociationKind::OldRevision);
        assert_eq!(association.quote_id.as_deref(), Some("quote-2"));
    }

    #[test]
    fn unknown_quote_never_creates_an_association() {
        let mut order = order(Some("3"));
        order.quote_reference = Some(field("Q-UNKNOWN".to_owned()));
        let association = match_quote(&order, &[quote("3", QuoteLifecycle::Sent)]);
        assert_eq!(association.kind, AssociationKind::UnknownQuote);
        assert!(association.quote_id.is_none());
    }

    #[test]
    fn unknown_revision_never_uses_the_latest_quote() {
        let association = match_quote(&order(Some("9")), &[quote("3", QuoteLifecycle::Sent)]);
        assert_eq!(association.kind, AssociationKind::UnknownRevision);
        assert!(association.quote_id.is_none());
    }

    #[test]
    fn missing_revision_requires_one_unambiguous_active_quote() {
        let ambiguous = match_quote(
            &order(None),
            &[
                quote("2", QuoteLifecycle::Sent),
                quote("3", QuoteLifecycle::AcceptedBaseline),
            ],
        );
        assert_eq!(ambiguous.kind, AssociationKind::Ambiguous);
        let exact = match_quote(&order(None), &[quote("3", QuoteLifecycle::Sent)]);
        assert_eq!(exact.kind, AssociationKind::UnambiguousActive);
    }

    #[test]
    fn exact_order_is_matched_with_zero_differences() {
        let result = reconcile(
            &quote("3", QuoteLifecycle::Sent),
            &order(Some("3")),
            &ReconciliationPolicy::default(),
        );
        assert_eq!(result.status, ReconciliationStatus::Matched);
        assert!(result.differences.is_empty());
    }

    #[test]
    fn changed_order_reports_quantity_terms_and_total() {
        let quote = quote("3", QuoteLifecycle::Sent);
        let mut order = order(Some("3"));
        order.document.line_items[1].quantity = Some(field(1.0));
        order.document.line_items[1].line_total = Some(field(2100.0));
        order.document.terms.payment_term_days = Some(field(60));
        order.document.terms.delivery_term = Some(field("2 weeks".to_owned()));
        order.document.terms.grand_total = Some(field(18900.0));
        let result = reconcile(&quote, &order, &ReconciliationPolicy::default());
        assert_eq!(result.status, ReconciliationStatus::Review);
        let ids = ids(&result);
        assert!(ids.contains(&DifferenceId::QuantityChanged));
        assert!(ids.contains(&DifferenceId::LineTotalMismatch));
        assert!(ids.contains(&DifferenceId::PaymentTermsChanged));
        assert!(ids.contains(&DifferenceId::DeliveryTermsChanged));
        assert!(ids.contains(&DifferenceId::GrandTotalChanged));
    }

    #[test]
    fn sku_is_authoritative_and_missing_extra_lines_are_critical() {
        let quote = quote("3", QuoteLifecycle::Sent);
        let mut order = order(Some("3"));
        order.document.line_items.remove(1);
        order
            .document
            .line_items
            .push(line("OTHER", "Support 3Y", 2.0, "EA", 2100.0));
        let result = reconcile(&quote, &order, &ReconciliationPolicy::default());
        assert_eq!(result.status, ReconciliationStatus::Critical);
        assert!(ids(&result).contains(&DifferenceId::MissingOrderedLine));
        assert!(ids(&result).contains(&DifferenceId::ExtraOrderedLine));
    }

    #[test]
    fn quantity_uom_price_currency_and_terms_have_distinct_ids() {
        let quote = quote("3", QuoteLifecycle::Sent);
        let mut order = order(Some("3"));
        order.document.line_items[0].quantity = Some(field(3.0));
        order.document.line_items[0].uom = Some(field("BOX".to_owned()));
        order.document.line_items[0].unit_price = Some(field(8500.0));
        order.document.currency = Some(field("USD".to_owned()));
        order.document.terms.payment_term_days = Some(field(60));
        order.document.terms.delivery_term = Some(field("2 weeks".to_owned()));
        let found = ids(&reconcile(&quote, &order, &ReconciliationPolicy::default()));
        for expected in [
            DifferenceId::QuantityChanged,
            DifferenceId::UomChanged,
            DifferenceId::UnitPriceChanged,
            DifferenceId::CurrencyMismatch,
            DifferenceId::PaymentTermsChanged,
            DifferenceId::DeliveryTermsChanged,
        ] {
            assert!(found.contains(&expected));
        }
    }

    #[test]
    fn duplicate_sku_is_incomplete_and_never_guessed() {
        let quote = quote("3", QuoteLifecycle::Sent);
        let mut order = order(Some("3"));
        order.document.line_items[1].sku = Some(field("FG-200G".to_owned()));
        let result = reconcile(&quote, &order, &ReconciliationPolicy::default());
        assert_eq!(result.status, ReconciliationStatus::Incomplete);
        assert!(ids(&result).contains(&DifferenceId::AmbiguousLineMatch));
    }

    #[test]
    fn description_matching_is_exact_and_only_used_without_skus() {
        let mut quote = quote("3", QuoteLifecycle::Sent);
        let mut order = order(Some("3"));
        for line in &mut quote.document.line_items {
            line.sku = None;
        }
        for line in &mut order.document.line_items {
            line.sku = None;
        }
        assert_eq!(
            reconcile(&quote, &order, &ReconciliationPolicy::default()).status,
            ReconciliationStatus::Matched
        );
        order.document.line_items[1].description = Some(field("Support three years".to_owned()));
        let changed = reconcile(&quote, &order, &ReconciliationPolicy::default());
        assert_eq!(changed.status, ReconciliationStatus::Critical);
    }

    #[test]
    fn mixed_sku_coverage_is_incomplete_instead_of_falling_back_to_description() {
        let quote = quote("3", QuoteLifecycle::Sent);
        let mut order = order(Some("3"));
        order.document.line_items[1].sku = None;
        let result = reconcile(&quote, &order, &ReconciliationPolicy::default());
        assert_eq!(result.status, ReconciliationStatus::Incomplete);
        assert!(
            result
                .incomplete_reasons
                .contains(&"Mixed SKU coverage prevents safe line association".to_owned())
        );
    }

    #[test]
    fn missing_required_comparison_data_is_incomplete() {
        let quote = quote("3", QuoteLifecycle::Sent);
        let mut order = order(Some("3"));
        order.document.currency = None;
        let result = reconcile(&quote, &order, &ReconciliationPolicy::default());
        assert_eq!(result.status, ReconciliationStatus::Incomplete);
        assert!(!result.incomplete_reasons.is_empty());
    }

    #[test]
    fn policy_can_raise_quantity_to_critical_without_ui_logic() {
        let quote = quote("3", QuoteLifecycle::Sent);
        let mut order = order(Some("3"));
        order.document.line_items[0].quantity = Some(field(3.0));
        let policy = ReconciliationPolicy::default()
            .with_severity(DifferenceId::QuantityChanged, DifferenceSeverity::Critical);
        assert_eq!(
            reconcile(&quote, &order, &policy).status,
            ReconciliationStatus::Critical
        );
    }
}
