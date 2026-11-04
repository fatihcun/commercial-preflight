use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use commercial_preflight_core::json::{self, JsonValue};
use commercial_preflight_core::{CommercialPreflightError, RuleId, Severity};

#[derive(Clone, Debug)]
pub struct Policy {
    pub company_name: String,
    pub default_currency: String,
    pub allowed_currencies: Vec<String>,
    pub minimum_gross_margin_pct: f64,
    pub maximum_sales_discount_pct: f64,
    pub maximum_payment_term_days: u32,
    pub maximum_quote_validity_days: u32,
    pub allowed_tax_rates_pct: Vec<f64>,
    pub forbidden_outgoing_terms: Vec<String>,
    severity_by_rule: BTreeMap<RuleId, Severity>,
    controls: RuleControls,
    enforce_maximum_quote_validity: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuleControls {
    pub arithmetic: bool,
    pub discount: bool,
    pub payment_terms: bool,
    pub validity: bool,
    pub customer_reference: bool,
    pub document_hygiene: bool,
}

impl Default for RuleControls {
    fn default() -> Self {
        Self {
            arithmetic: true,
            discount: true,
            payment_terms: true,
            validity: true,
            customer_reference: true,
            document_hygiene: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PolicyConfiguration {
    pub company_name: String,
    pub default_currency: String,
    pub minimum_gross_margin_pct: f64,
    pub maximum_sales_discount_pct: f64,
    pub maximum_payment_term_days: u32,
    pub maximum_quote_validity_days: u32,
    pub discount_severity: Severity,
    pub payment_term_severity: Severity,
    pub controls: RuleControls,
}

impl Policy {
    pub fn severity(&self, rule_id: &str, default: Severity) -> Severity {
        self.severity_by_rule
            .get(&RuleId::from(rule_id))
            .copied()
            .unwrap_or(default)
    }

    pub fn configured(
        configuration: PolicyConfiguration,
    ) -> Result<Self, CommercialPreflightError> {
        validate_non_empty("company.name", &configuration.company_name)?;
        validate_non_empty("company.default_currency", &configuration.default_currency)?;
        validate_percentage(
            "minimum_gross_margin_pct",
            configuration.minimum_gross_margin_pct,
        )?;
        validate_percentage(
            "maximum_sales_discount_pct",
            configuration.maximum_sales_discount_pct,
        )?;
        validate_configurable_severity("discount", configuration.discount_severity)?;
        validate_configurable_severity("payment term", configuration.payment_term_severity)?;

        let mut severity_by_rule = BTreeMap::new();
        severity_by_rule.insert(RuleId::from("CP-POL-001"), configuration.discount_severity);
        severity_by_rule.insert(
            RuleId::from("CP-POL-002"),
            configuration.payment_term_severity,
        );
        severity_by_rule.insert(RuleId::from("CP-MATH-001"), Severity::Critical);

        Ok(Self {
            company_name: configuration.company_name,
            default_currency: configuration.default_currency.to_uppercase(),
            allowed_currencies: Vec::new(),
            minimum_gross_margin_pct: configuration.minimum_gross_margin_pct,
            maximum_sales_discount_pct: configuration.maximum_sales_discount_pct,
            maximum_payment_term_days: configuration.maximum_payment_term_days,
            maximum_quote_validity_days: configuration.maximum_quote_validity_days,
            allowed_tax_rates_pct: Vec::new(),
            forbidden_outgoing_terms: vec![
                "internal cost".to_owned(),
                "purchase price".to_owned(),
                "distributor price".to_owned(),
                "gross margin".to_owned(),
                "internal note".to_owned(),
                "do not send".to_owned(),
                "confidential cost".to_owned(),
            ],
            severity_by_rule,
            controls: configuration.controls,
            enforce_maximum_quote_validity: true,
        })
    }

    pub fn rule_enabled(&self, rule_id: &str) -> bool {
        match rule_id {
            "CP-POL-001" => self.controls.discount,
            "CP-POL-002" => self.controls.payment_terms,
            "CP-HYG-001" => self.controls.document_hygiene,
            "CP-MATH-001" => self.controls.arithmetic,
            "CP-META-001" => self.controls.customer_reference,
            "CP-DATE-001" => self.controls.validity,
            _ => true,
        }
    }

    pub fn controls(&self) -> RuleControls {
        self.controls
    }

    pub fn enforces_maximum_quote_validity(&self) -> bool {
        self.enforce_maximum_quote_validity
    }
}

pub fn load_policy(path: impl AsRef<Path>) -> Result<Policy, CommercialPreflightError> {
    let bytes = fs::read(path.as_ref())
        .map_err(|source| CommercialPreflightError::io("read company policy", source))?;
    parse_policy(&bytes)
}

pub fn parse_policy(bytes: &[u8]) -> Result<Policy, CommercialPreflightError> {
    let raw = json::parse(bytes).map_err(|error| {
        CommercialPreflightError::InvalidPolicy(format!("JSON could not be parsed: {error}"))
    })?;
    let root = object(&raw, "policy root")?;
    let schema_version = string(required(root, "schema_version")?, "schema_version")?;
    if schema_version != "0.1" {
        return Err(invalid(format!(
            "unsupported schema version {schema_version}"
        )));
    }

    let company = object(required(root, "company")?, "company")?;
    let company_name = string(required(company, "name")?, "company.name")?.to_owned();
    let default_currency = string(
        required(company, "default_currency")?,
        "company.default_currency",
    )?
    .to_owned();
    validate_non_empty("company.name", &company_name)?;
    validate_non_empty("company.default_currency", &default_currency)?;
    let allowed_currencies = string_array(
        company.get("allowed_currencies"),
        "company.allowed_currencies",
    )?
    .into_iter()
    .map(|currency| currency.to_uppercase())
    .collect();

    let commercial = object(required(root, "commercial_policy")?, "commercial_policy")?;
    let minimum_gross_margin_pct = number(
        required(commercial, "minimum_gross_margin_pct")?,
        "minimum_gross_margin_pct",
    )?;
    let maximum_sales_discount_pct = number(
        required(commercial, "maximum_sales_discount_pct")?,
        "maximum_sales_discount_pct",
    )?;
    let maximum_payment_term_days = unsigned_integer(
        required(commercial, "maximum_payment_term_days")?,
        "maximum_payment_term_days",
    )?;
    let maximum_quote_validity_days = unsigned_integer(
        required(commercial, "maximum_quote_validity_days")?,
        "maximum_quote_validity_days",
    )?;
    let allowed_tax_rates_pct = number_array(
        commercial.get("allowed_tax_rates_pct"),
        "allowed_tax_rates_pct",
    )?;
    validate_percentage("minimum_gross_margin_pct", minimum_gross_margin_pct)?;
    validate_percentage("maximum_sales_discount_pct", maximum_sales_discount_pct)?;
    for rate in &allowed_tax_rates_pct {
        validate_percentage("allowed_tax_rates_pct", *rate)?;
    }

    let forbidden_outgoing_terms = string_array(
        root.get("forbidden_outgoing_terms"),
        "forbidden_outgoing_terms",
    )?
    .into_iter()
    .map(|term| normalize_text(&term))
    .filter(|term| !term.is_empty())
    .collect();

    let mut severity_by_rule = BTreeMap::new();
    if let Some(value) = root.get("severity_policy") {
        for (name, rule_ids) in object(value, "severity_policy")? {
            let severity = match name.as_str() {
                "CRITICAL" => Severity::Critical,
                "REVIEW" => Severity::Review,
                "INFO" => Severity::Info,
                _ => return Err(invalid(format!("unknown severity {name}"))),
            };
            let rule_ids = rule_ids
                .as_array()
                .ok_or_else(|| invalid(format!("severity_policy.{name} must be an array")))?;
            for (index, rule_id) in rule_ids.iter().enumerate() {
                let rule_id = string(rule_id, &format!("severity_policy.{name}[{index}]"))?;
                validate_non_empty("severity rule ID", rule_id)?;
                severity_by_rule.insert(RuleId::new(rule_id), severity);
            }
        }
    }

    Ok(Policy {
        company_name,
        default_currency: default_currency.to_uppercase(),
        allowed_currencies,
        minimum_gross_margin_pct,
        maximum_sales_discount_pct,
        maximum_payment_term_days,
        maximum_quote_validity_days,
        allowed_tax_rates_pct,
        forbidden_outgoing_terms,
        severity_by_rule,
        controls: RuleControls::default(),
        enforce_maximum_quote_validity: false,
    })
}

fn validate_configurable_severity(
    field: &str,
    severity: Severity,
) -> Result<(), CommercialPreflightError> {
    if severity == Severity::Info {
        return Err(invalid(format!(
            "{field} violations must be REVIEW or CRITICAL"
        )));
    }
    Ok(())
}

fn object<'a>(
    value: &'a JsonValue,
    field: &str,
) -> Result<&'a BTreeMap<String, JsonValue>, CommercialPreflightError> {
    value
        .as_object()
        .ok_or_else(|| invalid(format!("{field} must be an object")))
}

fn required<'a>(
    object: &'a BTreeMap<String, JsonValue>,
    field: &str,
) -> Result<&'a JsonValue, CommercialPreflightError> {
    object
        .get(field)
        .ok_or_else(|| invalid(format!("missing required field {field}")))
}

fn string<'a>(value: &'a JsonValue, field: &str) -> Result<&'a str, CommercialPreflightError> {
    value
        .as_str()
        .ok_or_else(|| invalid(format!("{field} must be a string")))
}

fn number(value: &JsonValue, field: &str) -> Result<f64, CommercialPreflightError> {
    value
        .as_f64()
        .ok_or_else(|| invalid(format!("{field} must be a finite number")))
}

fn unsigned_integer(value: &JsonValue, field: &str) -> Result<u32, CommercialPreflightError> {
    let value = number(value, field)?;
    if value.fract() != 0.0 || !(0.0..=f64::from(u32::MAX)).contains(&value) {
        return Err(invalid(format!("{field} must be a non-negative integer")));
    }
    Ok(value as u32)
}

fn string_array(
    value: Option<&JsonValue>,
    field: &str,
) -> Result<Vec<String>, CommercialPreflightError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    value
        .as_array()
        .ok_or_else(|| invalid(format!("{field} must be an array")))?
        .iter()
        .enumerate()
        .map(|(index, value)| string(value, &format!("{field}[{index}]")).map(ToOwned::to_owned))
        .collect()
}

fn number_array(
    value: Option<&JsonValue>,
    field: &str,
) -> Result<Vec<f64>, CommercialPreflightError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    value
        .as_array()
        .ok_or_else(|| invalid(format!("{field} must be an array")))?
        .iter()
        .enumerate()
        .map(|(index, value)| number(value, &format!("{field}[{index}]")))
        .collect()
}

fn validate_non_empty(field: &str, value: &str) -> Result<(), CommercialPreflightError> {
    if value.trim().is_empty() {
        return Err(invalid(format!("{field} must not be empty")));
    }
    Ok(())
}

fn validate_percentage(field: &str, value: f64) -> Result<(), CommercialPreflightError> {
    if !value.is_finite() || !(0.0..=100.0).contains(&value) {
        return Err(invalid(format!("{field} must be between 0 and 100")));
    }
    Ok(())
}

fn normalize_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn invalid(message: String) -> CommercialPreflightError {
    CommercialPreflightError::InvalidPolicy(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &[u8] = br#"{
        "schema_version":"0.1",
        "company":{"name":"Example Industries","default_currency":"eur","allowed_currencies":["EUR"]},
        "commercial_policy":{
            "minimum_gross_margin_pct":15,
            "maximum_sales_discount_pct":10,
            "maximum_payment_term_days":60,
            "maximum_quote_validity_days":30,
            "allowed_tax_rates_pct":[0,20]
        },
        "forbidden_outgoing_terms":["Internal   Cost"],
        "severity_policy":{"CRITICAL":["CP-MATH-001"],"REVIEW":["CP-POL-001"]}
    }"#;

    #[test]
    fn parses_critical_severity() {
        let policy = parse_policy(VALID).expect("valid policy");
        assert_eq!(policy.default_currency, "EUR");
        assert_eq!(policy.forbidden_outgoing_terms, ["internal cost"]);
        assert_eq!(
            policy.severity("CP-MATH-001", Severity::Review),
            Severity::Critical
        );
    }

    #[test]
    fn rejects_unknown_schema_and_fractional_days() {
        let invalid_schema = String::from_utf8(VALID.to_vec())
            .expect("fixture is UTF-8")
            .replace("\"0.1\"", "\"9.9\"");
        assert!(parse_policy(invalid_schema.as_bytes()).is_err());
        let fractional_days = String::from_utf8(VALID.to_vec())
            .expect("fixture is UTF-8")
            .replace(
                "\"maximum_payment_term_days\":60",
                "\"maximum_payment_term_days\":60.5",
            );
        assert!(parse_policy(fractional_days.as_bytes()).is_err());
    }

    #[test]
    fn configured_policy_rejects_pass_equivalent_severity() {
        let result = Policy::configured(PolicyConfiguration {
            company_name: "Example Technology".to_owned(),
            default_currency: "TRY".to_owned(),
            minimum_gross_margin_pct: 15.0,
            maximum_sales_discount_pct: 10.0,
            maximum_payment_term_days: 60,
            maximum_quote_validity_days: 30,
            discount_severity: Severity::Info,
            payment_term_severity: Severity::Critical,
            controls: RuleControls::default(),
        });
        assert!(result.is_err());
    }

    #[test]
    fn configured_policy_keeps_safe_rule_controls_authoritative() {
        let controls = RuleControls {
            discount: false,
            ..RuleControls::default()
        };
        let policy = Policy::configured(PolicyConfiguration {
            company_name: "Example Technology".to_owned(),
            default_currency: "TRY".to_owned(),
            minimum_gross_margin_pct: 15.0,
            maximum_sales_discount_pct: 10.0,
            maximum_payment_term_days: 60,
            maximum_quote_validity_days: 30,
            discount_severity: Severity::Review,
            payment_term_severity: Severity::Critical,
            controls,
        })
        .expect("configuration is valid");
        assert!(!policy.rule_enabled("CP-POL-001"));
        assert!(policy.rule_enabled("CP-MATH-001"));
    }
}
