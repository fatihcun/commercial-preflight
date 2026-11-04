use crate::Date;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct CellAddress {
    pub row: u32,
    pub column: u32,
}

impl CellAddress {
    pub fn new(row: u32, column: u32) -> Self {
        Self { row, column }
    }

    pub fn coordinate(self) -> String {
        let mut column_number = u64::from(self.column) + 1;
        let mut letters = Vec::new();
        while column_number > 0 {
            let remainder = (column_number - 1) % 26;
            letters.push((b'A' + u8::try_from(remainder).unwrap_or(0)) as char);
            column_number = (column_number - 1) / 26;
        }
        letters.reverse();
        format!(
            "{}{}",
            letters.into_iter().collect::<String>(),
            self.row + 1
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum CellValue {
    Text(String),
    Number(f64),
    Boolean(bool),
    Date(Date),
    Formula,
    Error,
}

impl CellValue {
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value),
            _ => None,
        }
    }

    pub fn number(&self) -> Option<f64> {
        match self {
            Self::Number(value) if value.is_finite() => Some(*value),
            _ => None,
        }
    }

    pub fn date(&self) -> Option<Date> {
        match self {
            Self::Date(value) => Some(*value),
            Self::Text(value) => Date::parse_supported(value).ok(),
            _ => None,
        }
    }

    pub fn display_text(&self) -> Option<String> {
        match self {
            Self::Text(value) => Some(value.clone()),
            Self::Number(value) => Some(value.to_string()),
            Self::Boolean(value) => Some(value.to_string()),
            Self::Date(value) => Some(value.to_string()),
            Self::Formula | Self::Error => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub address: CellAddress,
    pub value: CellValue,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SheetVisibility {
    Visible,
    Hidden,
    VeryHidden,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sheet {
    pub name: String,
    pub visibility: SheetVisibility,
    pub cells: Vec<Cell>,
}

impl Sheet {
    pub fn cell(&self, row: u32, column: u32) -> Option<&Cell> {
        let address = CellAddress::new(row, column);
        self.cells
            .binary_search_by_key(&address, |cell| cell.address)
            .ok()
            .map(|index| &self.cells[index])
    }

    pub fn max_row(&self) -> Option<u32> {
        self.cells.iter().map(|cell| cell.address.row).max()
    }

    pub fn max_column(&self) -> Option<u32> {
        self.cells.iter().map(|cell| cell.address.column).max()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkbookMetadata {
    pub title: Option<String>,
    pub subject: Option<String>,
    pub description: Option<String>,
    pub keywords: Option<String>,
    pub category: Option<String>,
}

impl WorkbookMetadata {
    pub fn searchable_text(&self) -> String {
        [
            self.title.as_deref(),
            self.subject.as_deref(),
            self.description.as_deref(),
            self.keywords.as_deref(),
            self.category.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" | ")
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NormalizedWorkbook {
    pub sheets: Vec<Sheet>,
    pub metadata: WorkbookMetadata,
}

impl NormalizedWorkbook {
    pub fn sheet(&self, name: &str) -> Option<&Sheet> {
        self.sheets.iter().find(|sheet| sheet.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinates_are_stable() {
        assert_eq!(CellAddress::new(0, 0).coordinate(), "A1");
        assert_eq!(CellAddress::new(24, 7).coordinate(), "H25");
        assert_eq!(CellAddress::new(0, 26).coordinate(), "AA1");
    }
}
