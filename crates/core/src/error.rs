use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CommercialPreflightError {
    #[error("could not {operation} the requested file")]
    Io {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("unsupported workbook: {0}")]
    UnsupportedWorkbook(String),
    #[error("invalid or corrupt workbook: {0}")]
    InvalidWorkbook(String),
    #[error("workbook safety limit exceeded: {0}")]
    WorkbookLimit(String),
    #[error("unsupported document: {0}")]
    UnsupportedDocument(String),
    #[error("invalid or uninspectable PDF: {0}")]
    InvalidPdf(String),
    #[error("PDF safety limit exceeded: {0}")]
    PdfLimit(String),
    #[error("invalid company policy: {0}")]
    InvalidPolicy(String),
    #[error("invalid date: {0}")]
    InvalidDate(String),
    #[error("system clock is outside the supported range")]
    Clock,
}

impl CommercialPreflightError {
    pub fn io(operation: &'static str, source: io::Error) -> Self {
        Self::Io { operation, source }
    }

    pub fn incomplete_reason(&self) -> String {
        self.to_string()
    }
}

impl PartialEq for CommercialPreflightError {
    fn eq(&self, other: &Self) -> bool {
        self.to_string() == other.to_string()
    }
}
