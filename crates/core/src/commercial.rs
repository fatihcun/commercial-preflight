use crate::{Date, DocumentFormat, LineItem, NormalizedProposal, ProposalField, SourceDocument};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentKind {
    Quote,
    CustomerOrder,
}

impl DocumentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Quote => "QUOTE",
            Self::CustomerOrder => "CUSTOMER_ORDER",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuoteLifecycle {
    Draft,
    Sent,
    AcceptedBaseline,
    Superseded,
}

impl QuoteLifecycle {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Sent => "SENT",
            Self::AcceptedBaseline => "ACCEPTED_BASELINE",
            Self::Superseded => "SUPERSEDED",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Revision(String);

impl Revision {
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let raw = value.into();
        let trimmed = raw.trim();
        let value = trimmed
            .strip_prefix("Revision")
            .or_else(|| trimmed.strip_prefix("revision"))
            .or_else(|| trimmed.strip_prefix("REV"))
            .or_else(|| trimmed.strip_prefix("Rev"))
            .or_else(|| trimmed.strip_prefix("rev"))
            .unwrap_or(trimmed)
            .trim_start_matches([' ', ':', '-', '#'])
            .trim();
        (!value.is_empty()).then(|| Self(value.to_uppercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuoteIdentity {
    pub quote_number: String,
    pub revision: Option<Revision>,
}

impl QuoteIdentity {
    pub fn new(
        quote_number: impl Into<String>,
        revision: Option<impl Into<String>>,
    ) -> Option<Self> {
        let quote_number = quote_number.into().trim().to_uppercase();
        if quote_number.is_empty() {
            return None;
        }
        Some(Self {
            quote_number,
            revision: revision.and_then(Revision::new),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomerIdentity(pub String);

impl CustomerIdentity {
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
        (!value.is_empty()).then_some(Self(value))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommercialTerms {
    pub subtotal: Option<ProposalField<f64>>,
    pub tax: Option<ProposalField<f64>>,
    pub grand_total: Option<ProposalField<f64>>,
    pub payment_term_days: Option<ProposalField<u32>>,
    pub delivery_term: Option<ProposalField<String>>,
    pub delivery_date: Option<ProposalField<Date>>,
    pub freight: Option<ProposalField<f64>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommercialSource {
    pub document: SourceDocument,
    pub source_path: String,
    pub sha256: String,
    pub parsed_at: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommercialDocument {
    pub document_id: String,
    pub kind: DocumentKind,
    pub customer: Option<ProposalField<String>>,
    pub document_date: Option<ProposalField<Date>>,
    pub currency: Option<ProposalField<String>>,
    pub line_items: Vec<LineItem>,
    pub terms: CommercialTerms,
    pub source: CommercialSource,
}

pub type CommercialLineItem = LineItem;

#[derive(Clone, Debug, PartialEq)]
pub struct Quote {
    pub quote_id: String,
    pub identity: QuoteIdentity,
    pub lifecycle: QuoteLifecycle,
    pub document: CommercialDocument,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CustomerOrder {
    pub order_id: String,
    pub order_number: Option<ProposalField<String>>,
    pub quote_reference: Option<ProposalField<String>>,
    pub quote_revision: Option<ProposalField<String>>,
    pub document: CommercialDocument,
}

impl Quote {
    pub fn from_normalized(
        proposal: &NormalizedProposal,
        quote_id: impl Into<String>,
        lifecycle: QuoteLifecycle,
        source_path: impl Into<String>,
        sha256: impl Into<String>,
        parsed_at: i64,
    ) -> Result<Self, &'static str> {
        let identity = QuoteIdentity::new(
            proposal
                .quote_number
                .as_ref()
                .ok_or("quote number was not identified")?
                .value
                .clone(),
            proposal
                .quote_revision
                .as_ref()
                .map(|field| field.value.clone()),
        )
        .ok_or("quote number was not identified")?;
        let quote_id = quote_id.into();
        Ok(Self {
            quote_id: quote_id.clone(),
            identity,
            lifecycle,
            document: commercial_document(
                proposal,
                quote_id,
                DocumentKind::Quote,
                proposal.quote_date.clone(),
                source_path.into(),
                sha256.into(),
                parsed_at,
            ),
        })
    }
}

impl CustomerOrder {
    pub fn from_normalized(
        proposal: &NormalizedProposal,
        order_id: impl Into<String>,
        source_path: impl Into<String>,
        sha256: impl Into<String>,
        parsed_at: i64,
    ) -> Self {
        let order_id = order_id.into();
        Self {
            order_id: order_id.clone(),
            order_number: proposal.order_number.clone(),
            quote_reference: proposal.quote_number.clone(),
            quote_revision: proposal.quote_revision.clone(),
            document: commercial_document(
                proposal,
                order_id,
                DocumentKind::CustomerOrder,
                proposal
                    .order_date
                    .clone()
                    .or_else(|| proposal.quote_date.clone()),
                source_path.into(),
                sha256.into(),
                parsed_at,
            ),
        }
    }
}

fn commercial_document(
    proposal: &NormalizedProposal,
    document_id: String,
    kind: DocumentKind,
    document_date: Option<ProposalField<Date>>,
    source_path: String,
    sha256: String,
    parsed_at: i64,
) -> CommercialDocument {
    CommercialDocument {
        document_id,
        kind,
        customer: proposal.customer.clone(),
        document_date,
        currency: proposal.currency.clone(),
        line_items: proposal.line_items.clone(),
        terms: CommercialTerms {
            subtotal: proposal.subtotal.clone(),
            tax: proposal.tax.clone(),
            grand_total: proposal.grand_total.clone(),
            payment_term_days: proposal.payment_term_days.clone(),
            delivery_term: proposal.delivery_term.clone(),
            delivery_date: proposal.delivery_date.clone(),
            freight: proposal.freight.clone(),
        },
        source: CommercialSource {
            document: proposal.source.clone(),
            source_path,
            sha256,
            parsed_at,
        },
    }
}

pub fn format_name(format: DocumentFormat) -> &'static str {
    format.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_identity_is_canonical_and_distinct() {
        let first = QuoteIdentity::new("q-2026-184", Some("Rev 2")).expect("identity");
        let second = QuoteIdentity::new("Q-2026-184", Some("3")).expect("identity");
        assert_eq!(first.quote_number, second.quote_number);
        assert_ne!(first.revision, second.revision);
        assert_eq!(first.revision.expect("revision").as_str(), "2");
    }
}
