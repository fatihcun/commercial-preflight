use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use commercial_preflight_core::{
    CustomerOrder, Date, DocumentFormat, FieldAliases, LineItem, NormalizedProposal, ProposalField,
    Quote, QuoteLifecycle, Severity, SourceEvidence,
};
use commercial_preflight_policy::{Policy, PolicyConfiguration, RuleControls};
use commercial_preflight_reconciliation::{QuoteAssociation, ReconciliationResult};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const LATEST_MIGRATION: i64 = 2;
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("database operation failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("local data could not be read: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid local configuration: {0}")]
    Validation(String),
    #[error("required local configuration was not found")]
    NotFound,
}

pub type StorageResult<T> = Result<T, StorageError>;

#[derive(Clone, Debug)]
pub struct LocalStore {
    path: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CompanyProfile {
    pub company_id: String,
    pub company_name: String,
    pub default_currency: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CompanyDraft {
    pub company_name: String,
    pub default_currency: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuleControlSettings {
    pub arithmetic: bool,
    pub discount: bool,
    pub payment_terms: bool,
    pub validity: bool,
    pub customer_reference: bool,
    pub document_hygiene: bool,
}

impl Default for RuleControlSettings {
    fn default() -> Self {
        RuleControls::default().into()
    }
}

impl From<RuleControlSettings> for RuleControls {
    fn from(value: RuleControlSettings) -> Self {
        Self {
            arithmetic: value.arithmetic,
            discount: value.discount,
            payment_terms: value.payment_terms,
            validity: value.validity,
            customer_reference: value.customer_reference,
            document_hygiene: value.document_hygiene,
        }
    }
}

impl From<RuleControls> for RuleControlSettings {
    fn from(value: RuleControls) -> Self {
        Self {
            arithmetic: value.arithmetic,
            discount: value.discount,
            payment_terms: value.payment_terms,
            validity: value.validity,
            customer_reference: value.customer_reference,
            document_hygiene: value.document_hygiene,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyDraft {
    pub name: String,
    pub maximum_discount_percent: f64,
    pub maximum_payment_term_days: u32,
    pub minimum_margin_percent: Option<f64>,
    pub maximum_quote_validity_days: u32,
    pub controls: RuleControlSettings,
    pub discount_severity: String,
    pub payment_term_severity: String,
}

impl Default for PolicyDraft {
    fn default() -> Self {
        Self {
            name: "Standard Sales Policy".to_owned(),
            maximum_discount_percent: 10.0,
            maximum_payment_term_days: 60,
            minimum_margin_percent: Some(15.0),
            maximum_quote_validity_days: 30,
            controls: RuleControlSettings::default(),
            discount_severity: "REVIEW".to_owned(),
            payment_term_severity: "CRITICAL".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PolicyProfile {
    pub id: String,
    pub company_id: String,
    pub name: String,
    pub version: u32,
    pub created_at: i64,
    pub updated_at: i64,
    pub active: bool,
    pub archived: bool,
    pub maximum_discount_percent: f64,
    pub maximum_payment_term_days: u32,
    pub minimum_margin_percent: Option<f64>,
    pub maximum_quote_validity_days: u32,
    pub controls: RuleControlSettings,
    pub discount_severity: String,
    pub payment_term_severity: String,
}

impl PolicyProfile {
    pub fn to_policy(&self, company: &CompanyProfile) -> StorageResult<Policy> {
        Policy::configured(PolicyConfiguration {
            company_name: company.company_name.clone(),
            default_currency: company.default_currency.clone(),
            minimum_gross_margin_pct: self.minimum_margin_percent.unwrap_or(0.0),
            maximum_sales_discount_pct: self.maximum_discount_percent,
            maximum_payment_term_days: self.maximum_payment_term_days,
            maximum_quote_validity_days: self.maximum_quote_validity_days,
            discount_severity: parse_severity(&self.discount_severity)?,
            payment_term_severity: parse_severity(&self.payment_term_severity)?,
            controls: self.controls.into(),
        })
        .map_err(|error| StorageError::Validation(error.to_string()))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AliasSet {
    pub customer: Vec<String>,
    pub quote_number: Vec<String>,
    pub payment_term: Vec<String>,
    pub discount: Vec<String>,
    pub subtotal: Vec<String>,
    pub tax: Vec<String>,
    pub grand_total: Vec<String>,
    pub valid_until: Vec<String>,
}

impl Default for AliasSet {
    fn default() -> Self {
        FieldAliases::default().into()
    }
}

impl From<FieldAliases> for AliasSet {
    fn from(value: FieldAliases) -> Self {
        Self {
            customer: value.customer,
            quote_number: value.quote_number,
            payment_term: value.payment_term,
            discount: value.discount,
            subtotal: value.subtotal,
            tax: value.tax,
            grand_total: value.grand_total,
            valid_until: value.valid_until,
        }
    }
}

impl From<AliasSet> for FieldAliases {
    fn from(value: AliasSet) -> Self {
        Self {
            customer: value.customer,
            quote_number: value.quote_number,
            payment_term: value.payment_term,
            discount: value.discount,
            subtotal: value.subtotal,
            tax: value.tax,
            grand_total: value.grand_total,
            valid_until: value.valid_until,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateDraft {
    pub name: String,
    pub aliases: AliasSet,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProposalTemplateProfile {
    pub id: String,
    pub company_id: String,
    pub name: String,
    pub is_default: bool,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub aliases: AliasSet,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingRequest {
    pub company: CompanyDraft,
    pub policy: PolicyDraft,
    pub template_name: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigurationDefaults {
    pub policy: PolicyDraft,
    pub aliases: AliasSet,
    pub template_name: String,
}

impl Default for ConfigurationDefaults {
    fn default() -> Self {
        Self {
            policy: PolicyDraft::default(),
            aliases: AliasSet::default(),
            template_name: "Standard Proposal".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupState {
    pub onboarding_required: bool,
    pub company: Option<CompanyProfile>,
    pub policy: Option<PolicyProfile>,
    pub template: Option<ProposalTemplateProfile>,
    pub database_path: String,
    pub schema_version: i64,
}

#[derive(Clone, Debug)]
pub struct NewScanRecord {
    pub scan_id: String,
    pub timestamp: i64,
    pub filename: String,
    pub sha256: String,
    pub document_format: String,
    pub company_id: String,
    pub policy_profile_id: String,
    pub policy_version: u32,
    pub template_profile_id: String,
    pub verdict: String,
    pub finding_count: usize,
    pub rule_ids: Vec<String>,
    pub checks_completed: usize,
    pub checks_not_applicable: usize,
    pub checks_incomplete: usize,
    pub duration_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScanHistoryEntry {
    pub scan_id: String,
    pub timestamp: i64,
    pub filename: String,
    pub sha256: String,
    pub document_format: String,
    pub company_id: String,
    pub policy_profile_id: String,
    pub policy_name: String,
    pub policy_version: u32,
    pub template_profile_id: String,
    pub template_name: String,
    pub verdict: String,
    pub finding_count: usize,
    pub rule_ids: Vec<String>,
    pub checks_completed: usize,
    pub checks_not_applicable: usize,
    pub checks_incomplete: usize,
    pub duration_ms: u64,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryFilter {
    pub verdict: Option<String>,
    pub document_format: Option<String>,
    pub from_timestamp: Option<i64>,
    pub to_timestamp: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CountByKey {
    pub key: String,
    pub count: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ManagementAggregates {
    pub total_proposals: u64,
    pub pass_count: u64,
    pub review_count: u64,
    pub critical_count: u64,
    pub incomplete_count: u64,
    pub findings_by_rule: Vec<CountByKey>,
    pub scans_by_format: Vec<CountByKey>,
    pub weekly_counts: Vec<CountByKey>,
    pub monthly_counts: Vec<CountByKey>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuoteRecord {
    pub quote_id: String,
    pub quote_number: String,
    pub revision: Option<String>,
    pub customer: Option<String>,
    pub document_date: Option<String>,
    pub currency: Option<String>,
    pub grand_total: Option<f64>,
    pub lifecycle: String,
    pub filename: String,
    pub source_path: String,
    pub source_available: bool,
    pub format: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OrderSummary {
    pub order_id: String,
    pub received_at: i64,
    pub order_number: Option<String>,
    pub customer: Option<String>,
    pub quote_id: Option<String>,
    pub quote_number: Option<String>,
    pub quote_revision: Option<String>,
    pub status: String,
    pub difference_count: usize,
    pub association_kind: Option<String>,
    pub reviewed_at: Option<i64>,
    pub source_path: String,
    pub source_available: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DifferenceRecord {
    pub code: String,
    pub title: String,
    pub severity: String,
    pub subject: String,
    pub quote_value: Option<String>,
    pub order_value: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OrderDetail {
    pub summary: OrderSummary,
    pub filename: String,
    pub format: String,
    pub reviewed_status: Option<String>,
    pub differences: Vec<DifferenceRecord>,
    pub incomplete_reasons: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FolderIntakeSettings {
    pub quote_folder: Option<String>,
    pub order_folder: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AuditEvent {
    pub event_id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub event_type: String,
    pub status_snapshot: Option<String>,
    pub created_at: i64,
}

#[derive(Clone, Debug)]
pub struct IncompleteIntakeRecord {
    pub document_kind: String,
    pub source_kind: String,
    pub sha256: String,
    pub source_path: String,
    pub filename: String,
    pub document_format: String,
    pub reason: String,
}

impl LocalStore {
    pub fn open(path: impl Into<PathBuf>) -> StorageResult<Self> {
        let store = Self { path: path.into() };
        if let Some(parent) = store.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut connection = store.connection()?;
        migrate(&mut connection)?;
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn connection(&self) -> StorageResult<Connection> {
        let connection = Connection::open(&self.path)?;
        connection.execute_batch("PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;")?;
        Ok(connection)
    }

    pub fn migration_version(&self) -> StorageResult<i64> {
        self.connection()?
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    pub fn setup_state(&self) -> StorageResult<SetupState> {
        let company = self.active_company()?;
        let policy = match &company {
            Some(company) => self.active_policy(&company.company_id)?,
            None => None,
        };
        let template = match &company {
            Some(company) => self.default_template(&company.company_id)?,
            None => None,
        };
        Ok(SetupState {
            onboarding_required: company.is_none() || policy.is_none() || template.is_none(),
            company,
            policy,
            template,
            database_path: self.path.to_string_lossy().into_owned(),
            schema_version: self.migration_version()?,
        })
    }

    pub fn complete_onboarding(&self, request: &OnboardingRequest) -> StorageResult<SetupState> {
        validate_company(&request.company)?;
        validate_policy(&request.policy)?;
        validate_name("template name", &request.template_name)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let existing: i64 =
            transaction.query_row("SELECT COUNT(*) FROM companies", [], |row| row.get(0))?;
        if existing != 0 {
            return Err(StorageError::Validation(
                "company setup has already been completed".to_owned(),
            ));
        }
        let company = insert_company(&transaction, &request.company)?;
        let policy = insert_policy(&transaction, &company.company_id, &request.policy, true)?;
        let template = insert_template(
            &transaction,
            &company.company_id,
            &TemplateDraft {
                name: request.template_name.clone(),
                aliases: AliasSet::default(),
            },
            true,
        )?;
        set_setting(&transaction, "active_company_id", &company.company_id)?;
        set_setting(&transaction, "active_policy_id", &policy.id)?;
        set_setting(&transaction, "default_template_id", &template.id)?;
        transaction.commit()?;
        self.setup_state()
    }

    pub fn active_company(&self) -> StorageResult<Option<CompanyProfile>> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT c.company_id, c.company_name, c.default_currency, c.created_at, c.updated_at
                 FROM companies c
                 JOIN app_settings s ON s.value = c.company_id AND s.key = 'active_company_id'",
                [],
                company_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn update_company(
        &self,
        company_id: &str,
        draft: &CompanyDraft,
    ) -> StorageResult<CompanyProfile> {
        validate_company(draft)?;
        let connection = self.connection()?;
        let changed = connection.execute(
            "UPDATE companies SET company_name = ?1, default_currency = ?2, updated_at = ?3 WHERE company_id = ?4",
            params![draft.company_name.trim(), draft.default_currency.trim().to_uppercase(), now_millis(), company_id],
        )?;
        if changed == 0 {
            return Err(StorageError::NotFound);
        }
        connection
            .query_row(
                "SELECT company_id, company_name, default_currency, created_at, updated_at FROM companies WHERE company_id = ?1",
                [company_id],
                company_from_row,
            )
            .map_err(Into::into)
    }

    pub fn list_policies(&self, company_id: &str) -> StorageResult<Vec<PolicyProfile>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT p.policy_id, p.company_id, p.name, p.current_version, p.created_at, p.updated_at,
                    p.active, p.archived, v.maximum_discount_percent, v.maximum_payment_term_days,
                    v.minimum_margin_percent, v.maximum_quote_validity_days, v.arithmetic_enabled,
                    v.discount_enabled, v.payment_terms_enabled, v.validity_enabled,
                    v.customer_reference_enabled, v.document_hygiene_enabled,
                    v.discount_severity, v.payment_term_severity
             FROM policy_profiles p JOIN policy_versions v
               ON v.policy_id = p.policy_id AND v.version = p.current_version
             WHERE p.company_id = ?1 ORDER BY p.active DESC, p.updated_at DESC",
        )?;
        let rows = statement.query_map([company_id], policy_from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn active_policy(&self, company_id: &str) -> StorageResult<Option<PolicyProfile>> {
        Ok(self
            .list_policies(company_id)?
            .into_iter()
            .find(|policy| policy.active && !policy.archived))
    }

    pub fn create_policy(
        &self,
        company_id: &str,
        draft: &PolicyDraft,
    ) -> StorageResult<PolicyProfile> {
        validate_policy(draft)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let has_active: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM policy_profiles WHERE company_id = ?1 AND active = 1 AND archived = 0)",
            [company_id],
            |row| row.get(0),
        )?;
        let profile = insert_policy(&transaction, company_id, draft, !has_active)?;
        if !has_active {
            set_setting(&transaction, "active_policy_id", &profile.id)?;
        }
        transaction.commit()?;
        Ok(profile)
    }

    pub fn update_policy(
        &self,
        policy_id: &str,
        draft: &PolicyDraft,
    ) -> StorageResult<PolicyProfile> {
        validate_policy(draft)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let (company_id, current_version, created_at, active, archived): (String, u32, i64, bool, bool) = transaction
            .query_row(
                "SELECT company_id, current_version, created_at, active, archived FROM policy_profiles WHERE policy_id = ?1",
                [policy_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .optional()?
            .ok_or(StorageError::NotFound)?;
        if archived {
            return Err(StorageError::Validation(
                "an archived policy cannot be edited".to_owned(),
            ));
        }
        let version = current_version.saturating_add(1);
        let updated_at = now_millis();
        insert_policy_version(&transaction, policy_id, version, draft, updated_at)?;
        transaction.execute(
            "UPDATE policy_profiles SET name = ?1, current_version = ?2, updated_at = ?3 WHERE policy_id = ?4",
            params![draft.name.trim(), version, updated_at, policy_id],
        )?;
        transaction.commit()?;
        Ok(PolicyProfile {
            id: policy_id.to_owned(),
            company_id,
            name: draft.name.trim().to_owned(),
            version,
            created_at,
            updated_at,
            active,
            archived,
            maximum_discount_percent: draft.maximum_discount_percent,
            maximum_payment_term_days: draft.maximum_payment_term_days,
            minimum_margin_percent: draft.minimum_margin_percent,
            maximum_quote_validity_days: draft.maximum_quote_validity_days,
            controls: draft.controls,
            discount_severity: normalize_severity(&draft.discount_severity)?,
            payment_term_severity: normalize_severity(&draft.payment_term_severity)?,
        })
    }

    pub fn set_active_policy(&self, company_id: &str, policy_id: &str) -> StorageResult<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let valid: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM policy_profiles WHERE policy_id = ?1 AND company_id = ?2 AND archived = 0)",
            params![policy_id, company_id],
            |row| row.get(0),
        )?;
        if !valid {
            return Err(StorageError::NotFound);
        }
        transaction.execute(
            "UPDATE policy_profiles SET active = 0 WHERE company_id = ?1",
            [company_id],
        )?;
        transaction.execute(
            "UPDATE policy_profiles SET active = 1 WHERE policy_id = ?1",
            [policy_id],
        )?;
        set_setting(&transaction, "active_policy_id", policy_id)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn archive_policy(&self, policy_id: &str) -> StorageResult<()> {
        let connection = self.connection()?;
        let active: bool = connection
            .query_row(
                "SELECT active FROM policy_profiles WHERE policy_id = ?1",
                [policy_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(StorageError::NotFound)?;
        if active {
            return Err(StorageError::Validation(
                "the active policy cannot be archived".to_owned(),
            ));
        }
        connection.execute(
            "UPDATE policy_profiles SET archived = 1, updated_at = ?1 WHERE policy_id = ?2",
            params![now_millis(), policy_id],
        )?;
        Ok(())
    }

    pub fn list_templates(&self, company_id: &str) -> StorageResult<Vec<ProposalTemplateProfile>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT template_id, company_id, name, is_default, enabled, created_at, updated_at
             FROM template_profiles WHERE company_id = ?1 ORDER BY is_default DESC, name",
        )?;
        let rows = statement.query_map([company_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, bool>(3)?,
                row.get::<_, bool>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })?;
        let basics = rows.collect::<Result<Vec<_>, _>>()?;
        basics
            .into_iter()
            .map(|basic| template_with_aliases(&connection, basic))
            .collect()
    }

    pub fn default_template(
        &self,
        company_id: &str,
    ) -> StorageResult<Option<ProposalTemplateProfile>> {
        Ok(self
            .list_templates(company_id)?
            .into_iter()
            .find(|template| template.is_default && template.enabled))
    }

    pub fn create_template(
        &self,
        company_id: &str,
        draft: &TemplateDraft,
    ) -> StorageResult<ProposalTemplateProfile> {
        validate_template(draft)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let has_default: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM template_profiles WHERE company_id = ?1 AND is_default = 1 AND enabled = 1)",
            [company_id],
            |row| row.get(0),
        )?;
        let template = insert_template(&transaction, company_id, draft, !has_default)?;
        if !has_default {
            set_setting(&transaction, "default_template_id", &template.id)?;
        }
        transaction.commit()?;
        Ok(template)
    }

    pub fn update_template(
        &self,
        template_id: &str,
        draft: &TemplateDraft,
    ) -> StorageResult<ProposalTemplateProfile> {
        validate_template(draft)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let basic: (String, bool, bool, i64) = transaction
            .query_row(
                "SELECT company_id, is_default, enabled, created_at FROM template_profiles WHERE template_id = ?1",
                [template_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?
            .ok_or(StorageError::NotFound)?;
        let updated_at = now_millis();
        transaction.execute(
            "UPDATE template_profiles SET name = ?1, updated_at = ?2 WHERE template_id = ?3",
            params![draft.name.trim(), updated_at, template_id],
        )?;
        transaction.execute(
            "DELETE FROM field_aliases WHERE template_id = ?1",
            [template_id],
        )?;
        insert_aliases(&transaction, template_id, &draft.aliases)?;
        transaction.commit()?;
        Ok(ProposalTemplateProfile {
            id: template_id.to_owned(),
            company_id: basic.0,
            name: draft.name.trim().to_owned(),
            is_default: basic.1,
            enabled: basic.2,
            created_at: basic.3,
            updated_at,
            aliases: draft.aliases.clone(),
        })
    }

    pub fn set_default_template(&self, company_id: &str, template_id: &str) -> StorageResult<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let valid: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM template_profiles WHERE template_id = ?1 AND company_id = ?2 AND enabled = 1)",
            params![template_id, company_id],
            |row| row.get(0),
        )?;
        if !valid {
            return Err(StorageError::NotFound);
        }
        transaction.execute(
            "UPDATE template_profiles SET is_default = 0 WHERE company_id = ?1",
            [company_id],
        )?;
        transaction.execute(
            "UPDATE template_profiles SET is_default = 1 WHERE template_id = ?1",
            [template_id],
        )?;
        set_setting(&transaction, "default_template_id", template_id)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn disable_template(&self, template_id: &str) -> StorageResult<()> {
        let connection = self.connection()?;
        let is_default: bool = connection
            .query_row(
                "SELECT is_default FROM template_profiles WHERE template_id = ?1",
                [template_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(StorageError::NotFound)?;
        if is_default {
            return Err(StorageError::Validation(
                "the default template cannot be disabled".to_owned(),
            ));
        }
        connection.execute(
            "UPDATE template_profiles SET enabled = 0, updated_at = ?1 WHERE template_id = ?2",
            params![now_millis(), template_id],
        )?;
        Ok(())
    }

    pub fn insert_scan(&self, scan: &NewScanRecord) -> StorageResult<()> {
        validate_hash(&scan.sha256)?;
        self.connection()?.execute(
            "INSERT INTO scan_history (
                scan_id, timestamp, filename, sha256, document_format, company_id,
                policy_profile_id, policy_version, template_profile_id, verdict, finding_count,
                rule_ids, checks_completed, checks_not_applicable, checks_incomplete, duration_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                scan.scan_id,
                scan.timestamp,
                scan.filename,
                scan.sha256,
                scan.document_format,
                scan.company_id,
                scan.policy_profile_id,
                scan.policy_version,
                scan.template_profile_id,
                scan.verdict,
                scan.finding_count,
                scan.rule_ids.join("\n"),
                scan.checks_completed,
                scan.checks_not_applicable,
                scan.checks_incomplete,
                scan.duration_ms,
            ],
        )?;
        Ok(())
    }

    pub fn history(&self, filter: &HistoryFilter) -> StorageResult<Vec<ScanHistoryEntry>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT h.scan_id, h.timestamp, h.filename, h.sha256, h.document_format, h.company_id,
                    h.policy_profile_id, p.name, h.policy_version, h.template_profile_id, t.name,
                    h.verdict, h.finding_count, h.rule_ids, h.checks_completed,
                    h.checks_not_applicable, h.checks_incomplete, h.duration_ms
             FROM scan_history h
             JOIN policy_profiles p ON p.policy_id = h.policy_profile_id
             JOIN template_profiles t ON t.template_id = h.template_profile_id
             WHERE (?1 IS NULL OR h.verdict = ?1)
               AND (?2 IS NULL OR h.document_format = ?2)
               AND (?3 IS NULL OR h.timestamp >= ?3)
               AND (?4 IS NULL OR h.timestamp <= ?4)
             ORDER BY h.timestamp DESC",
        )?;
        let rows = statement.query_map(
            params![
                filter.verdict.as_deref(),
                filter.document_format.as_deref(),
                filter.from_timestamp,
                filter.to_timestamp,
            ],
            |row| {
                let rule_ids: String = row.get(13)?;
                Ok(ScanHistoryEntry {
                    scan_id: row.get(0)?,
                    timestamp: row.get(1)?,
                    filename: row.get(2)?,
                    sha256: row.get(3)?,
                    document_format: row.get(4)?,
                    company_id: row.get(5)?,
                    policy_profile_id: row.get(6)?,
                    policy_name: row.get(7)?,
                    policy_version: row.get(8)?,
                    template_profile_id: row.get(9)?,
                    template_name: row.get(10)?,
                    verdict: row.get(11)?,
                    finding_count: row.get(12)?,
                    rule_ids: split_rule_ids(&rule_ids),
                    checks_completed: row.get(14)?,
                    checks_not_applicable: row.get(15)?,
                    checks_incomplete: row.get(16)?,
                    duration_ms: row.get(17)?,
                })
            },
        )?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn clear_history(&self) -> StorageResult<usize> {
        self.connection()?
            .execute("DELETE FROM scan_history", [])
            .map_err(Into::into)
    }

    pub fn aggregates(&self) -> StorageResult<ManagementAggregates> {
        let connection = self.connection()?;
        let total_proposals = scalar_count(&connection, "SELECT COUNT(*) FROM scan_history")?;
        let verdicts = grouped_counts(
            &connection,
            "SELECT verdict, COUNT(*) FROM scan_history GROUP BY verdict",
        )?;
        let scans_by_format = grouped_counts(
            &connection,
            "SELECT document_format, COUNT(*) FROM scan_history GROUP BY document_format ORDER BY document_format",
        )?;
        let weekly_counts = grouped_counts(
            &connection,
            "SELECT strftime('%Y-W%W', timestamp / 1000, 'unixepoch', 'localtime'), COUNT(*) FROM scan_history GROUP BY 1 ORDER BY 1",
        )?;
        let monthly_counts = grouped_counts(
            &connection,
            "SELECT strftime('%Y-%m', timestamp / 1000, 'unixepoch', 'localtime'), COUNT(*) FROM scan_history GROUP BY 1 ORDER BY 1",
        )?;
        let mut findings = BTreeMap::<String, u64>::new();
        let mut statement =
            connection.prepare("SELECT rule_ids FROM scan_history WHERE rule_ids <> ''")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        for row in rows {
            for rule_id in split_rule_ids(&row?) {
                *findings.entry(rule_id).or_default() += 1;
            }
        }
        let verdict_count = |key: &str| {
            verdicts
                .iter()
                .find(|entry| entry.key == key)
                .map_or(0, |entry| entry.count)
        };
        Ok(ManagementAggregates {
            total_proposals,
            pass_count: verdict_count("PASS"),
            review_count: verdict_count("REVIEW"),
            critical_count: verdict_count("CRITICAL"),
            incomplete_count: verdict_count("INCOMPLETE"),
            findings_by_rule: findings
                .into_iter()
                .map(|(key, count)| CountByKey { key, count })
                .collect(),
            scans_by_format,
            weekly_counts,
            monthly_counts,
        })
    }
}

impl LocalStore {
    pub fn has_document_hash(&self, sha256: &str) -> StorageResult<bool> {
        validate_hash(sha256)?;
        self.connection()?
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM document_intake WHERE sha256 = ?1)",
                [sha256],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    pub fn known_intake_hashes(&self) -> StorageResult<Vec<String>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT sha256 FROM document_intake")?;
        let rows = statement.query_map([], |row| row.get(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn register_quote(&self, quote: &Quote, source_kind: &str) -> StorageResult<bool> {
        validate_hash(&quote.document.source.sha256)?;
        validate_source_kind(source_kind)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let intake_id = new_id("intake");
        let inserted = transaction.execute(
            "INSERT OR IGNORE INTO document_intake (
                intake_id, document_kind, source_kind, sha256, source_path, filename,
                document_format, received_at, parse_status, parse_error
             ) VALUES (?1, 'QUOTE', ?2, ?3, ?4, ?5, ?6, ?7, 'COMPLETED', NULL)",
            params![
                intake_id,
                source_kind,
                quote.document.source.sha256,
                quote.document.source.source_path,
                quote.document.source.document.filename,
                quote.document.source.document.format.as_str(),
                quote.document.source.parsed_at,
            ],
        )?;
        if inserted == 0 {
            return Ok(false);
        }
        let now = quote.document.source.parsed_at;
        let revision = quote
            .identity
            .revision
            .as_ref()
            .map(|revision| revision.as_str())
            .unwrap_or_default();
        transaction.execute(
            "INSERT INTO quotes (
                quote_id, intake_id, quote_number, revision, customer, document_date, currency,
                subtotal, tax, grand_total, payment_term_days, delivery_term, delivery_date,
                freight, lifecycle, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?16)",
            params![
                quote.quote_id,
                intake_id,
                quote.identity.quote_number,
                revision,
                field_value(&quote.document.customer),
                date_field_value(&quote.document.document_date),
                field_value(&quote.document.currency),
                number_field_value(&quote.document.terms.subtotal),
                number_field_value(&quote.document.terms.tax),
                number_field_value(&quote.document.terms.grand_total),
                u32_field_value(&quote.document.terms.payment_term_days),
                field_value(&quote.document.terms.delivery_term),
                date_field_value(&quote.document.terms.delivery_date),
                number_field_value(&quote.document.terms.freight),
                quote.lifecycle.as_str(),
                now,
            ],
        )?;
        insert_quote_lines(&transaction, &quote.quote_id, &quote.document.line_items)?;
        let superseded = if matches!(
            quote.lifecycle,
            QuoteLifecycle::Sent | QuoteLifecycle::AcceptedBaseline
        ) {
            transaction.execute(
                "UPDATE quotes SET lifecycle = 'SUPERSEDED', updated_at = ?1
                 WHERE quote_number = ?2 AND quote_id <> ?3
                   AND lifecycle IN ('SENT', 'ACCEPTED_BASELINE')",
                params![now, quote.identity.quote_number, quote.quote_id],
            )?
        } else {
            0
        };
        insert_audit(
            &transaction,
            "QUOTE",
            &quote.quote_id,
            "QUOTE_REGISTERED",
            Some(quote.lifecycle.as_str()),
            now,
        )?;
        if superseded > 0 {
            insert_audit(
                &transaction,
                "QUOTE",
                &quote.quote_id,
                "QUOTE_REVISION_SUPERSEDED",
                Some(quote.lifecycle.as_str()),
                now,
            )?;
        }
        transaction.commit()?;
        Ok(true)
    }

    pub fn set_quote_lifecycle(
        &self,
        quote_id: &str,
        lifecycle: QuoteLifecycle,
    ) -> StorageResult<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let quote_number: String = transaction
            .query_row(
                "SELECT quote_number FROM quotes WHERE quote_id = ?1",
                [quote_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(StorageError::NotFound)?;
        let now = now_millis();
        if matches!(
            lifecycle,
            QuoteLifecycle::Sent | QuoteLifecycle::AcceptedBaseline
        ) {
            let mut statement = transaction.prepare(
                "SELECT quote_id FROM quotes WHERE quote_number = ?1 AND quote_id <> ?2
                 AND lifecycle IN ('SENT', 'ACCEPTED_BASELINE')",
            )?;
            let superseded = statement
                .query_map(params![quote_number, quote_id], |row| {
                    row.get::<_, String>(0)
                })?
                .collect::<Result<Vec<_>, _>>()?;
            drop(statement);
            transaction.execute(
                "UPDATE quotes SET lifecycle = 'SUPERSEDED', updated_at = ?1
                 WHERE quote_number = ?2 AND quote_id <> ?3
                   AND lifecycle IN ('SENT', 'ACCEPTED_BASELINE')",
                params![now, quote_number, quote_id],
            )?;
            for superseded_id in superseded {
                insert_audit(
                    &transaction,
                    "QUOTE",
                    &superseded_id,
                    "QUOTE_REVISION_SUPERSEDED",
                    Some("SUPERSEDED"),
                    now,
                )?;
            }
        }
        transaction.execute(
            "UPDATE quotes SET lifecycle = ?1, updated_at = ?2 WHERE quote_id = ?3",
            params![lifecycle.as_str(), now, quote_id],
        )?;
        insert_audit(
            &transaction,
            "QUOTE",
            quote_id,
            "QUOTE_LIFECYCLE_CHANGED",
            Some(lifecycle.as_str()),
            now,
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn register_order(&self, order: &CustomerOrder, source_kind: &str) -> StorageResult<bool> {
        validate_hash(&order.document.source.sha256)?;
        validate_source_kind(source_kind)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let intake_id = new_id("intake");
        let inserted = transaction.execute(
            "INSERT OR IGNORE INTO document_intake (
                intake_id, document_kind, source_kind, sha256, source_path, filename,
                document_format, received_at, parse_status, parse_error
             ) VALUES (?1, 'CUSTOMER_ORDER', ?2, ?3, ?4, ?5, ?6, ?7, 'COMPLETED', NULL)",
            params![
                intake_id,
                source_kind,
                order.document.source.sha256,
                order.document.source.source_path,
                order.document.source.document.filename,
                order.document.source.document.format.as_str(),
                order.document.source.parsed_at,
            ],
        )?;
        if inserted == 0 {
            return Ok(false);
        }
        let now = order.document.source.parsed_at;
        transaction.execute(
            "INSERT INTO customer_orders (
                order_id, intake_id, order_number, customer, quote_reference, quote_revision,
                order_date, currency, subtotal, tax, grand_total, payment_term_days,
                delivery_term, delivery_date, freight, reconciliation_status, difference_count,
                received_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                       'INCOMPLETE', 0, ?16, ?16)",
            params![
                order.order_id,
                intake_id,
                field_value(&order.order_number),
                field_value(&order.document.customer),
                field_value(&order.quote_reference),
                field_value(&order.quote_revision),
                date_field_value(&order.document.document_date),
                field_value(&order.document.currency),
                number_field_value(&order.document.terms.subtotal),
                number_field_value(&order.document.terms.tax),
                number_field_value(&order.document.terms.grand_total),
                u32_field_value(&order.document.terms.payment_term_days),
                field_value(&order.document.terms.delivery_term),
                date_field_value(&order.document.terms.delivery_date),
                number_field_value(&order.document.terms.freight),
                now,
            ],
        )?;
        insert_order_lines(&transaction, &order.order_id, &order.document.line_items)?;
        insert_audit(
            &transaction,
            "CUSTOMER_ORDER",
            &order.order_id,
            "ORDER_RECEIVED",
            Some("INCOMPLETE"),
            now,
        )?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn register_incomplete_intake(
        &self,
        record: &IncompleteIntakeRecord,
    ) -> StorageResult<Option<String>> {
        let document_kind = record.document_kind.as_str();
        let source_kind = record.source_kind.as_str();
        let sha256 = record.sha256.as_str();
        let source_path = record.source_path.as_str();
        let filename = record.filename.as_str();
        let document_format = record.document_format.as_str();
        let reason = record.reason.as_str();
        if !matches!(document_kind, "QUOTE" | "CUSTOMER_ORDER") {
            return Err(StorageError::Validation(
                "invalid commercial document kind".to_owned(),
            ));
        }
        validate_source_kind(source_kind)?;
        validate_hash(sha256)?;
        if !matches!(document_format, "PDF" | "XLSX") {
            return Err(StorageError::Validation(
                "invalid document format".to_owned(),
            ));
        }
        let now = now_millis();
        let intake_id = new_id("intake");
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let inserted = transaction.execute(
            "INSERT OR IGNORE INTO document_intake (
                intake_id, document_kind, source_kind, sha256, source_path, filename,
                document_format, received_at, parse_status, parse_error
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'INCOMPLETE', ?9)",
            params![
                intake_id,
                document_kind,
                source_kind,
                sha256,
                source_path,
                filename,
                document_format,
                now,
                reason,
            ],
        )?;
        if inserted == 0 {
            return Ok(None);
        }
        let entity_id = if document_kind == "CUSTOMER_ORDER" {
            let order_id = new_id("order");
            transaction.execute(
                "INSERT INTO customer_orders (
                    order_id, intake_id, reconciliation_status, difference_count,
                    received_at, updated_at
                 ) VALUES (?1, ?2, 'INCOMPLETE', 0, ?3, ?3)",
                params![order_id, intake_id, now],
            )?;
            transaction.execute(
                "INSERT INTO reconciliation_runs (
                    run_id, order_id, quote_id, status, incomplete_reasons, completed_at
                 ) VALUES (?1, ?2, NULL, 'INCOMPLETE', ?3, ?4)",
                params![new_id("reconciliation"), order_id, reason, now],
            )?;
            insert_audit(
                &transaction,
                "CUSTOMER_ORDER",
                &order_id,
                "ORDER_RECEIVED",
                Some("INCOMPLETE"),
                now,
            )?;
            insert_audit(
                &transaction,
                "CUSTOMER_ORDER",
                &order_id,
                "RECONCILIATION_COMPLETED",
                Some("INCOMPLETE"),
                now,
            )?;
            order_id
        } else {
            insert_audit(
                &transaction,
                "DOCUMENT_INTAKE",
                &intake_id,
                "QUOTE_INTAKE_INCOMPLETE",
                Some("INCOMPLETE"),
                now,
            )?;
            intake_id.clone()
        };
        transaction.commit()?;
        Ok(Some(entity_id))
    }

    pub fn persist_association(
        &self,
        order_id: &str,
        association: &QuoteAssociation,
        manual: bool,
    ) -> StorageResult<()> {
        let now = now_millis();
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO quote_order_associations (
                order_id, quote_id, association_kind, manual, associated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(order_id) DO UPDATE SET quote_id = excluded.quote_id,
                association_kind = excluded.association_kind, manual = excluded.manual,
                associated_at = excluded.associated_at",
            params![
                order_id,
                association.quote_id,
                association.kind.as_str(),
                manual,
                now,
            ],
        )?;
        let event_type = if association.quote_id.is_some() {
            "QUOTE_ASSOCIATED"
        } else {
            "QUOTE_ASSOCIATION_UNRESOLVED"
        };
        insert_audit(
            &transaction,
            "CUSTOMER_ORDER",
            order_id,
            event_type,
            Some(association.kind.as_str()),
            now,
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn persist_reconciliation(
        &self,
        result: &ReconciliationResult,
        policy_profile_id: Option<&str>,
        policy_version: Option<u32>,
    ) -> StorageResult<String> {
        let run_id = new_id("reconciliation");
        let now = now_millis();
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO reconciliation_runs (
                run_id, order_id, quote_id, status, incomplete_reasons,
                policy_profile_id, policy_version, completed_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                run_id,
                result.order_id,
                result.quote_id,
                result.status.as_str(),
                result.incomplete_reasons.join("\n"),
                policy_profile_id,
                policy_version,
                now,
            ],
        )?;
        for (position, difference) in result.differences.iter().enumerate() {
            transaction.execute(
                "INSERT INTO reconciliation_differences (
                    run_id, position, difference_code, title, severity, subject,
                    quote_value, order_value
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    run_id,
                    position,
                    difference.id.code(),
                    difference.id.title(),
                    difference.severity.as_str(),
                    difference.subject,
                    difference.quote_value,
                    difference.order_value,
                ],
            )?;
        }
        transaction.execute(
            "UPDATE customer_orders SET reconciliation_status = ?1, difference_count = ?2,
                updated_at = ?3 WHERE order_id = ?4",
            params![
                result.status.as_str(),
                result.differences.len(),
                now,
                result.order_id,
            ],
        )?;
        insert_audit(
            &transaction,
            "CUSTOMER_ORDER",
            &result.order_id,
            "RECONCILIATION_COMPLETED",
            Some(result.status.as_str()),
            now,
        )?;
        transaction.commit()?;
        Ok(run_id)
    }

    pub fn persist_unmatched_reconciliation(
        &self,
        order_id: &str,
        reason: &str,
        policy_profile_id: Option<&str>,
        policy_version: Option<u32>,
    ) -> StorageResult<String> {
        let run_id = new_id("reconciliation");
        let now = now_millis();
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO reconciliation_runs (
                run_id, order_id, quote_id, status, incomplete_reasons,
                policy_profile_id, policy_version, completed_at
             ) VALUES (?1, ?2, NULL, 'INCOMPLETE', ?3, ?4, ?5, ?6)",
            params![
                run_id,
                order_id,
                reason,
                policy_profile_id,
                policy_version,
                now,
            ],
        )?;
        transaction.execute(
            "UPDATE customer_orders SET reconciliation_status = 'INCOMPLETE',
                difference_count = 0, updated_at = ?1 WHERE order_id = ?2",
            params![now, order_id],
        )?;
        insert_audit(
            &transaction,
            "CUSTOMER_ORDER",
            order_id,
            "RECONCILIATION_COMPLETED",
            Some("INCOMPLETE"),
            now,
        )?;
        transaction.commit()?;
        Ok(run_id)
    }

    pub fn list_quotes(&self) -> StorageResult<Vec<QuoteRecord>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT q.quote_id, q.quote_number, q.revision, q.customer, q.document_date,
                    q.currency, q.grand_total, q.lifecycle, i.filename, i.source_path,
                    i.document_format, q.created_at
             FROM quotes q JOIN document_intake i ON i.intake_id = q.intake_id
             ORDER BY q.quote_number, q.revision DESC",
        )?;
        let rows = statement.query_map([], |row| {
            let revision: String = row.get(2)?;
            let source_path: String = row.get(9)?;
            Ok(QuoteRecord {
                quote_id: row.get(0)?,
                quote_number: row.get(1)?,
                revision: (!revision.is_empty()).then_some(revision),
                customer: row.get(3)?,
                document_date: row.get(4)?,
                currency: row.get(5)?,
                grand_total: row.get(6)?,
                lifecycle: row.get(7)?,
                filename: row.get(8)?,
                source_available: Path::new(&source_path).is_file(),
                source_path,
                format: row.get(10)?,
                created_at: row.get(11)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn load_quotes(&self) -> StorageResult<Vec<Quote>> {
        self.list_quotes()?
            .iter()
            .map(|record| self.load_quote(&record.quote_id))
            .collect()
    }

    pub fn load_quote(&self, quote_id: &str) -> StorageResult<Quote> {
        let connection = self.connection()?;
        let row = connection
            .query_row(
                "SELECT q.quote_number, q.revision, q.customer, q.document_date, q.currency,
                        q.subtotal, q.tax, q.grand_total, q.payment_term_days, q.delivery_term,
                        q.delivery_date, q.freight, q.lifecycle, i.filename, i.source_path,
                        i.sha256, i.document_format, i.received_at
                 FROM quotes q JOIN document_intake i ON i.intake_id = q.intake_id
                 WHERE q.quote_id = ?1",
                [quote_id],
                |row| {
                    Ok(CommercialRow {
                        number: row.get(0)?,
                        revision: row.get(1)?,
                        customer: row.get(2)?,
                        document_date: row.get(3)?,
                        currency: row.get(4)?,
                        subtotal: row.get(5)?,
                        tax: row.get(6)?,
                        grand_total: row.get(7)?,
                        payment_term_days: row.get(8)?,
                        delivery_term: row.get(9)?,
                        delivery_date: row.get(10)?,
                        freight: row.get(11)?,
                        lifecycle: row.get(12)?,
                        filename: row.get(13)?,
                        source_path: row.get(14)?,
                        sha256: row.get(15)?,
                        format: row.get(16)?,
                        received_at: row.get(17)?,
                    })
                },
            )
            .optional()?
            .ok_or(StorageError::NotFound)?;
        let mut proposal = proposal_from_row(&row)?;
        proposal.quote_number = Some(metadata_field(row.number));
        proposal.quote_revision = (!row.revision.is_empty()).then(|| metadata_field(row.revision));
        proposal.line_items = load_lines(&connection, "quote_lines", "quote_id", quote_id)?;
        Quote::from_normalized(
            &proposal,
            quote_id,
            parse_lifecycle(&row.lifecycle)?,
            row.source_path,
            row.sha256,
            row.received_at,
        )
        .map_err(|reason| StorageError::Validation(reason.to_owned()))
    }

    pub fn load_order(&self, order_id: &str) -> StorageResult<CustomerOrder> {
        let connection = self.connection()?;
        let row = connection
            .query_row(
                "SELECT COALESCE(o.quote_reference, ''), COALESCE(o.quote_revision, ''),
                        o.customer, o.order_date, o.currency, o.subtotal, o.tax, o.grand_total,
                        o.payment_term_days, o.delivery_term, o.delivery_date, o.freight,
                        '', i.filename, i.source_path, i.sha256, i.document_format, i.received_at,
                        o.order_number
                 FROM customer_orders o JOIN document_intake i ON i.intake_id = o.intake_id
                 WHERE o.order_id = ?1",
                [order_id],
                |row| {
                    Ok((
                        CommercialRow {
                            number: row.get(0)?,
                            revision: row.get(1)?,
                            customer: row.get(2)?,
                            document_date: row.get(3)?,
                            currency: row.get(4)?,
                            subtotal: row.get(5)?,
                            tax: row.get(6)?,
                            grand_total: row.get(7)?,
                            payment_term_days: row.get(8)?,
                            delivery_term: row.get(9)?,
                            delivery_date: row.get(10)?,
                            freight: row.get(11)?,
                            lifecycle: row.get(12)?,
                            filename: row.get(13)?,
                            source_path: row.get(14)?,
                            sha256: row.get(15)?,
                            format: row.get(16)?,
                            received_at: row.get(17)?,
                        },
                        row.get::<_, Option<String>>(18)?,
                    ))
                },
            )
            .optional()?
            .ok_or(StorageError::NotFound)?;
        let (row, order_number) = row;
        let mut proposal = proposal_from_row(&row)?;
        proposal.quote_number = (!row.number.is_empty()).then(|| metadata_field(row.number));
        proposal.quote_revision = (!row.revision.is_empty()).then(|| metadata_field(row.revision));
        proposal.order_number = order_number.map(metadata_field);
        proposal.line_items = load_lines(&connection, "order_lines", "order_id", order_id)?;
        Ok(CustomerOrder::from_normalized(
            &proposal,
            order_id,
            row.source_path,
            row.sha256,
            row.received_at,
        ))
    }

    pub fn pending_orders_for_quote(
        &self,
        quote_number: &str,
    ) -> StorageResult<Vec<CustomerOrder>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT o.order_id
             FROM customer_orders o
             LEFT JOIN quote_order_associations a ON a.order_id = o.order_id
             WHERE UPPER(TRIM(o.quote_reference)) = UPPER(TRIM(?1))
               AND (
                    a.order_id IS NULL
                    OR a.quote_id IS NULL
                    OR a.association_kind IN (
                        'MISSING_REFERENCE', 'UNKNOWN_QUOTE', 'UNKNOWN_REVISION', 'AMBIGUOUS'
                    )
               )
             ORDER BY o.received_at, o.order_id",
        )?;
        let order_ids = statement
            .query_map([quote_number], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        drop(connection);
        order_ids
            .iter()
            .map(|order_id| self.load_order(order_id))
            .collect()
    }

    pub fn record_automatic_backfill(&self, order_id: &str, status: &str) -> StorageResult<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        insert_audit(
            &transaction,
            "CUSTOMER_ORDER",
            order_id,
            "ORDER_AUTO_RECONCILED_AFTER_QUOTE_REGISTRATION",
            Some(status),
            now_millis(),
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn reconciliation_run_count(&self, order_id: &str) -> StorageResult<u64> {
        self.connection()?
            .query_row(
                "SELECT COUNT(*) FROM reconciliation_runs WHERE order_id = ?1",
                [order_id],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    pub fn association_count(&self, order_id: &str) -> StorageResult<u64> {
        self.connection()?
            .query_row(
                "SELECT COUNT(*) FROM quote_order_associations WHERE order_id = ?1",
                [order_id],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    pub fn order_inbox(&self) -> StorageResult<Vec<OrderSummary>> {
        self.query_orders("")
    }

    pub fn exceptions(&self) -> StorageResult<Vec<OrderSummary>> {
        self.query_orders("WHERE o.reconciliation_status <> 'MATCHED' AND o.reviewed_at IS NULL")
    }

    fn query_orders(&self, condition: &str) -> StorageResult<Vec<OrderSummary>> {
        let connection = self.connection()?;
        let sql = format!(
            "SELECT o.order_id, o.received_at, o.order_number, o.customer, a.quote_id,
                    COALESCE(q.quote_number, o.quote_reference),
                    COALESCE(NULLIF(q.revision, ''), o.quote_revision),
                    o.reconciliation_status,
                    CASE WHEN EXISTS (
                        SELECT 1 FROM reconciliation_differences causal
                        WHERE causal.run_id = (
                            SELECT latest.run_id FROM reconciliation_runs latest
                            WHERE latest.order_id = o.order_id
                            ORDER BY latest.completed_at DESC LIMIT 1
                        ) AND causal.difference_code IN (
                            'CP-REC-005', 'CP-REC-006', 'CP-REC-007',
                            'CP-REC-008', 'CP-REC-009', 'CP-REC-010'
                        )
                    ) THEN o.difference_count - (
                        SELECT COUNT(*) FROM reconciliation_differences derived
                        WHERE derived.run_id = (
                            SELECT latest.run_id FROM reconciliation_runs latest
                            WHERE latest.order_id = o.order_id
                            ORDER BY latest.completed_at DESC LIMIT 1
                        ) AND derived.difference_code IN ('CP-REC-011', 'CP-REC-012')
                    ) ELSE o.difference_count END,
                    a.association_kind, o.reviewed_at, i.source_path
             FROM customer_orders o
             JOIN document_intake i ON i.intake_id = o.intake_id
             LEFT JOIN quote_order_associations a ON a.order_id = o.order_id
             LEFT JOIN quotes q ON q.quote_id = a.quote_id
             {condition} ORDER BY o.received_at DESC"
        );
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map([], order_summary_from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn order_detail(&self, order_id: &str) -> StorageResult<OrderDetail> {
        let summary = self
            .order_inbox()?
            .into_iter()
            .find(|order| order.order_id == order_id)
            .ok_or(StorageError::NotFound)?;
        let connection = self.connection()?;
        let (filename, format, reviewed_status): (String, String, Option<String>) = connection
            .query_row(
                "SELECT i.filename, i.document_format, o.reviewed_status
                 FROM customer_orders o JOIN document_intake i ON i.intake_id = o.intake_id
                 WHERE o.order_id = ?1",
                [order_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
        let latest: Option<(String, String)> = connection
            .query_row(
                "SELECT run_id, incomplete_reasons FROM reconciliation_runs
                 WHERE order_id = ?1 ORDER BY completed_at DESC LIMIT 1",
                [order_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let (differences, incomplete_reasons) = if let Some((run_id, reasons)) = latest {
            let mut statement = connection.prepare(
                "SELECT difference_code, title, severity, subject, quote_value, order_value
                 FROM reconciliation_differences WHERE run_id = ?1 ORDER BY position",
            )?;
            let differences = statement
                .query_map([run_id], |row| {
                    Ok(DifferenceRecord {
                        code: row.get(0)?,
                        title: row.get(1)?,
                        severity: row.get(2)?,
                        subject: row.get(3)?,
                        quote_value: row.get(4)?,
                        order_value: row.get(5)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            (differences, split_rule_ids(&reasons))
        } else {
            (Vec::new(), Vec::new())
        };
        Ok(OrderDetail {
            summary,
            filename,
            format,
            reviewed_status,
            differences,
            incomplete_reasons,
        })
    }

    pub fn mark_order_reviewed(&self, order_id: &str) -> StorageResult<i64> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let status: String = transaction
            .query_row(
                "SELECT reconciliation_status FROM customer_orders WHERE order_id = ?1",
                [order_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(StorageError::NotFound)?;
        let now = now_millis();
        transaction.execute(
            "UPDATE customer_orders SET reviewed_at = ?1, reviewed_status = ?2,
                updated_at = ?1 WHERE order_id = ?3",
            params![now, status, order_id],
        )?;
        insert_audit(
            &transaction,
            "CUSTOMER_ORDER",
            order_id,
            "EXCEPTION_REVIEWED",
            Some(&status),
            now,
        )?;
        transaction.commit()?;
        Ok(now)
    }

    pub fn folder_intake_settings(&self) -> StorageResult<FolderIntakeSettings> {
        let connection = self.connection()?;
        let get = |key: &str| -> StorageResult<Option<String>> {
            connection
                .query_row(
                    "SELECT value FROM app_settings WHERE key = ?1",
                    [key],
                    |row| row.get(0),
                )
                .optional()
                .map_err(Into::into)
        };
        Ok(FolderIntakeSettings {
            quote_folder: get("quote_intake_folder")?.filter(|value| !value.is_empty()),
            order_folder: get("order_intake_folder")?.filter(|value| !value.is_empty()),
        })
    }

    pub fn set_folder_intake_settings(
        &self,
        settings: &FolderIntakeSettings,
    ) -> StorageResult<FolderIntakeSettings> {
        for path in [&settings.quote_folder, &settings.order_folder]
            .into_iter()
            .flatten()
        {
            let path = Path::new(path);
            if !path.is_dir() {
                return Err(StorageError::Validation(
                    "an intake folder does not exist or is not a directory".to_owned(),
                ));
            }
        }
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        set_setting(
            &transaction,
            "quote_intake_folder",
            settings.quote_folder.as_deref().unwrap_or_default(),
        )?;
        set_setting(
            &transaction,
            "order_intake_folder",
            settings.order_folder.as_deref().unwrap_or_default(),
        )?;
        transaction.commit()?;
        self.folder_intake_settings()
    }

    pub fn audit_events(&self, entity_id: &str) -> StorageResult<Vec<AuditEvent>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT event_id, entity_type, entity_id, event_type, status_snapshot, created_at
             FROM audit_events WHERE entity_id = ?1 ORDER BY created_at, event_id",
        )?;
        let rows = statement.query_map([entity_id], |row| {
            Ok(AuditEvent {
                event_id: row.get(0)?,
                entity_type: row.get(1)?,
                entity_id: row.get(2)?,
                event_type: row.get(3)?,
                status_snapshot: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

struct CommercialRow {
    number: String,
    revision: String,
    customer: Option<String>,
    document_date: Option<String>,
    currency: Option<String>,
    subtotal: Option<f64>,
    tax: Option<f64>,
    grand_total: Option<f64>,
    payment_term_days: Option<u32>,
    delivery_term: Option<String>,
    delivery_date: Option<String>,
    freight: Option<f64>,
    lifecycle: String,
    filename: String,
    source_path: String,
    sha256: String,
    format: String,
    received_at: i64,
}

fn field_value(field: &Option<ProposalField<String>>) -> Option<&str> {
    field.as_ref().map(|field| field.value.as_str())
}

fn number_field_value(field: &Option<ProposalField<f64>>) -> Option<f64> {
    field.as_ref().map(|field| field.value)
}

fn u32_field_value(field: &Option<ProposalField<u32>>) -> Option<u32> {
    field.as_ref().map(|field| field.value)
}

fn date_field_value(field: &Option<ProposalField<Date>>) -> Option<String> {
    field.as_ref().map(|field| field.value.to_string())
}

fn insert_quote_lines(
    transaction: &Transaction<'_>,
    quote_id: &str,
    lines: &[LineItem],
) -> StorageResult<()> {
    for (position, line) in lines.iter().enumerate() {
        transaction.execute(
            "INSERT INTO quote_lines (
                quote_id, position, sku, description, quantity, uom, unit_price,
                discount_percent, line_total
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                quote_id,
                position,
                field_value(&line.sku),
                field_value(&line.description),
                number_field_value(&line.quantity),
                field_value(&line.uom),
                number_field_value(&line.unit_price),
                number_field_value(&line.discount_percent),
                number_field_value(&line.line_total),
            ],
        )?;
    }
    Ok(())
}

fn insert_order_lines(
    transaction: &Transaction<'_>,
    order_id: &str,
    lines: &[LineItem],
) -> StorageResult<()> {
    for (position, line) in lines.iter().enumerate() {
        transaction.execute(
            "INSERT INTO order_lines (
                order_id, position, sku, description, quantity, uom, unit_price,
                discount_percent, line_total
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                order_id,
                position,
                field_value(&line.sku),
                field_value(&line.description),
                number_field_value(&line.quantity),
                field_value(&line.uom),
                number_field_value(&line.unit_price),
                number_field_value(&line.discount_percent),
                number_field_value(&line.line_total),
            ],
        )?;
    }
    Ok(())
}

fn insert_audit(
    transaction: &Transaction<'_>,
    entity_type: &str,
    entity_id: &str,
    event_type: &str,
    status_snapshot: Option<&str>,
    created_at: i64,
) -> StorageResult<()> {
    transaction.execute(
        "INSERT INTO audit_events (
            event_id, entity_type, entity_id, event_type, status_snapshot, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            new_id("audit"),
            entity_type,
            entity_id,
            event_type,
            status_snapshot,
            created_at,
        ],
    )?;
    Ok(())
}

fn metadata_field<T>(value: T) -> ProposalField<T> {
    ProposalField::new(value, SourceEvidence::metadata(None))
}

fn proposal_from_row(row: &CommercialRow) -> StorageResult<NormalizedProposal> {
    let format = match row.format.as_str() {
        "PDF" => DocumentFormat::Pdf,
        "XLSX" => DocumentFormat::Xlsx,
        value => {
            return Err(StorageError::Validation(format!(
                "unsupported stored document format {value}"
            )));
        }
    };
    let mut proposal = NormalizedProposal::new(format, row.filename.clone());
    proposal.customer = row.customer.clone().map(metadata_field);
    proposal.quote_date = parse_optional_date(row.document_date.as_deref())?.map(metadata_field);
    proposal.order_date = proposal.quote_date.clone();
    proposal.currency = row.currency.clone().map(metadata_field);
    proposal.subtotal = row.subtotal.map(metadata_field);
    proposal.tax = row.tax.map(metadata_field);
    proposal.grand_total = row.grand_total.map(metadata_field);
    proposal.payment_term_days = row.payment_term_days.map(metadata_field);
    proposal.delivery_term = row.delivery_term.clone().map(metadata_field);
    proposal.delivery_date = parse_optional_date(row.delivery_date.as_deref())?.map(metadata_field);
    proposal.freight = row.freight.map(metadata_field);
    Ok(proposal)
}

fn parse_optional_date(value: Option<&str>) -> StorageResult<Option<Date>> {
    value
        .map(Date::parse_supported)
        .transpose()
        .map_err(|error| StorageError::Validation(error.to_string()))
}

fn parse_lifecycle(value: &str) -> StorageResult<QuoteLifecycle> {
    match value {
        "DRAFT" => Ok(QuoteLifecycle::Draft),
        "SENT" => Ok(QuoteLifecycle::Sent),
        "ACCEPTED_BASELINE" => Ok(QuoteLifecycle::AcceptedBaseline),
        "SUPERSEDED" => Ok(QuoteLifecycle::Superseded),
        _ => Err(StorageError::Validation(
            "invalid stored quote lifecycle".to_owned(),
        )),
    }
}

fn load_lines(
    connection: &Connection,
    table: &str,
    id_column: &str,
    id: &str,
) -> StorageResult<Vec<LineItem>> {
    let sql = format!(
        "SELECT sku, description, quantity, uom, unit_price, discount_percent, line_total
         FROM {table} WHERE {id_column} = ?1 ORDER BY position"
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map([id], |row| {
        Ok(LineItem {
            sku: row.get::<_, Option<String>>(0)?.map(metadata_field),
            description: row.get::<_, Option<String>>(1)?.map(metadata_field),
            quantity: row.get::<_, Option<f64>>(2)?.map(metadata_field),
            uom: row.get::<_, Option<String>>(3)?.map(metadata_field),
            unit_price: row.get::<_, Option<f64>>(4)?.map(metadata_field),
            discount_percent: row.get::<_, Option<f64>>(5)?.map(metadata_field),
            line_total: row.get::<_, Option<f64>>(6)?.map(metadata_field),
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn order_summary_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<OrderSummary> {
    let revision: Option<String> = row.get(6)?;
    let source_path: String = row.get(11)?;
    Ok(OrderSummary {
        order_id: row.get(0)?,
        received_at: row.get(1)?,
        order_number: row.get(2)?,
        customer: row.get(3)?,
        quote_id: row.get(4)?,
        quote_number: row.get(5)?,
        quote_revision: revision.filter(|revision| !revision.is_empty()),
        status: row.get(7)?,
        difference_count: row.get(8)?,
        association_kind: row.get(9)?,
        reviewed_at: row.get(10)?,
        source_available: Path::new(&source_path).is_file(),
        source_path,
    })
}

fn validate_source_kind(value: &str) -> StorageResult<()> {
    if matches!(value, "MANUAL" | "WATCHED_FOLDER" | "OUTLOOK" | "ERP_API") {
        Ok(())
    } else {
        Err(StorageError::Validation(
            "invalid document intake source".to_owned(),
        ))
    }
}

pub fn sha256_file(path: impl AsRef<Path>) -> StorageResult<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub fn new_id(prefix: &str) -> String {
    let sequence = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    format!(
        "{prefix}-{}-{}-{sequence}",
        now_millis(),
        std::process::id()
    )
}

pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

fn migrate(connection: &mut Connection) -> StorageResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at INTEGER NOT NULL
        );",
    )?;
    let current: i64 = connection.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;
    if current < 1 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(MIGRATION_1)?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (1, ?1)",
            [now_millis()],
        )?;
        transaction.commit()?;
    }
    if current < 2 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(MIGRATION_2)?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (2, ?1)",
            [now_millis()],
        )?;
        transaction.commit()?;
    }
    let actual: i64 = connection.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;
    if actual != LATEST_MIGRATION {
        return Err(StorageError::Validation(format!(
            "unsupported database schema version {actual}"
        )));
    }
    Ok(())
}

const MIGRATION_1: &str = r#"
CREATE TABLE companies (
    company_id TEXT PRIMARY KEY,
    company_name TEXT NOT NULL,
    default_currency TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE policy_profiles (
    policy_id TEXT PRIMARY KEY,
    company_id TEXT NOT NULL REFERENCES companies(company_id),
    name TEXT NOT NULL,
    current_version INTEGER NOT NULL,
    active INTEGER NOT NULL DEFAULT 0,
    archived INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE UNIQUE INDEX one_active_policy_per_company
    ON policy_profiles(company_id) WHERE active = 1;
CREATE TABLE policy_versions (
    policy_id TEXT NOT NULL REFERENCES policy_profiles(policy_id),
    version INTEGER NOT NULL,
    maximum_discount_percent REAL NOT NULL,
    maximum_payment_term_days INTEGER NOT NULL,
    minimum_margin_percent REAL,
    maximum_quote_validity_days INTEGER NOT NULL,
    arithmetic_enabled INTEGER NOT NULL,
    discount_enabled INTEGER NOT NULL,
    payment_terms_enabled INTEGER NOT NULL,
    validity_enabled INTEGER NOT NULL,
    customer_reference_enabled INTEGER NOT NULL,
    document_hygiene_enabled INTEGER NOT NULL,
    discount_severity TEXT NOT NULL CHECK(discount_severity IN ('REVIEW', 'CRITICAL')),
    payment_term_severity TEXT NOT NULL CHECK(payment_term_severity IN ('REVIEW', 'CRITICAL')),
    created_at INTEGER NOT NULL,
    PRIMARY KEY(policy_id, version)
);
CREATE TABLE template_profiles (
    template_id TEXT PRIMARY KEY,
    company_id TEXT NOT NULL REFERENCES companies(company_id),
    name TEXT NOT NULL,
    is_default INTEGER NOT NULL DEFAULT 0,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE UNIQUE INDEX one_default_template_per_company
    ON template_profiles(company_id) WHERE is_default = 1;
CREATE TABLE field_aliases (
    template_id TEXT NOT NULL REFERENCES template_profiles(template_id) ON DELETE CASCADE,
    field_key TEXT NOT NULL,
    alias TEXT NOT NULL,
    position INTEGER NOT NULL,
    PRIMARY KEY(template_id, field_key, alias)
);
CREATE TABLE app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE scan_history (
    scan_id TEXT PRIMARY KEY,
    timestamp INTEGER NOT NULL,
    filename TEXT NOT NULL,
    sha256 TEXT NOT NULL CHECK(length(sha256) = 64),
    document_format TEXT NOT NULL CHECK(document_format IN ('XLSX', 'PDF')),
    company_id TEXT NOT NULL REFERENCES companies(company_id),
    policy_profile_id TEXT NOT NULL REFERENCES policy_profiles(policy_id),
    policy_version INTEGER NOT NULL,
    template_profile_id TEXT NOT NULL REFERENCES template_profiles(template_id),
    verdict TEXT NOT NULL CHECK(verdict IN ('PASS', 'REVIEW', 'CRITICAL', 'INCOMPLETE')),
    finding_count INTEGER NOT NULL,
    rule_ids TEXT NOT NULL,
    checks_completed INTEGER NOT NULL,
    checks_not_applicable INTEGER NOT NULL,
    checks_incomplete INTEGER NOT NULL,
    duration_ms INTEGER NOT NULL,
    FOREIGN KEY(policy_profile_id, policy_version) REFERENCES policy_versions(policy_id, version)
);
CREATE INDEX scan_history_timestamp ON scan_history(timestamp DESC);
CREATE INDEX scan_history_filters ON scan_history(verdict, document_format, timestamp);
"#;

const MIGRATION_2: &str = r#"
CREATE TABLE document_intake (
    intake_id TEXT PRIMARY KEY,
    document_kind TEXT NOT NULL CHECK(document_kind IN ('QUOTE', 'CUSTOMER_ORDER')),
    source_kind TEXT NOT NULL CHECK(source_kind IN ('MANUAL', 'WATCHED_FOLDER', 'OUTLOOK', 'ERP_API')),
    sha256 TEXT NOT NULL UNIQUE CHECK(length(sha256) = 64),
    source_path TEXT NOT NULL,
    filename TEXT NOT NULL,
    document_format TEXT NOT NULL CHECK(document_format IN ('XLSX', 'PDF')),
    received_at INTEGER NOT NULL,
    parse_status TEXT NOT NULL CHECK(parse_status IN ('COMPLETED', 'INCOMPLETE')),
    parse_error TEXT
);
CREATE INDEX document_intake_received_at ON document_intake(received_at DESC);

CREATE TABLE quotes (
    quote_id TEXT PRIMARY KEY,
    intake_id TEXT NOT NULL UNIQUE REFERENCES document_intake(intake_id),
    quote_number TEXT NOT NULL,
    revision TEXT NOT NULL DEFAULT '',
    customer TEXT,
    document_date TEXT,
    currency TEXT,
    subtotal REAL,
    tax REAL,
    grand_total REAL,
    payment_term_days INTEGER,
    delivery_term TEXT,
    delivery_date TEXT,
    freight REAL,
    lifecycle TEXT NOT NULL CHECK(lifecycle IN ('DRAFT', 'SENT', 'ACCEPTED_BASELINE', 'SUPERSEDED')),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(quote_number, revision)
);
CREATE INDEX quotes_identity ON quotes(quote_number, revision, lifecycle);

CREATE TABLE quote_lines (
    quote_id TEXT NOT NULL REFERENCES quotes(quote_id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    sku TEXT,
    description TEXT,
    quantity REAL,
    uom TEXT,
    unit_price REAL,
    discount_percent REAL,
    line_total REAL,
    PRIMARY KEY(quote_id, position)
);

CREATE TABLE customer_orders (
    order_id TEXT PRIMARY KEY,
    intake_id TEXT NOT NULL UNIQUE REFERENCES document_intake(intake_id),
    order_number TEXT,
    customer TEXT,
    quote_reference TEXT,
    quote_revision TEXT,
    order_date TEXT,
    currency TEXT,
    subtotal REAL,
    tax REAL,
    grand_total REAL,
    payment_term_days INTEGER,
    delivery_term TEXT,
    delivery_date TEXT,
    freight REAL,
    reconciliation_status TEXT NOT NULL DEFAULT 'INCOMPLETE' CHECK(reconciliation_status IN ('MATCHED', 'REVIEW', 'CRITICAL', 'INCOMPLETE')),
    difference_count INTEGER NOT NULL DEFAULT 0,
    reviewed_at INTEGER,
    reviewed_status TEXT CHECK(reviewed_status IS NULL OR reviewed_status IN ('MATCHED', 'REVIEW', 'CRITICAL', 'INCOMPLETE')),
    received_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX customer_orders_inbox ON customer_orders(received_at DESC);
CREATE INDEX customer_orders_exceptions ON customer_orders(reconciliation_status, reviewed_at, received_at DESC);

CREATE TABLE order_lines (
    order_id TEXT NOT NULL REFERENCES customer_orders(order_id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    sku TEXT,
    description TEXT,
    quantity REAL,
    uom TEXT,
    unit_price REAL,
    discount_percent REAL,
    line_total REAL,
    PRIMARY KEY(order_id, position)
);

CREATE TABLE quote_order_associations (
    order_id TEXT PRIMARY KEY REFERENCES customer_orders(order_id) ON DELETE CASCADE,
    quote_id TEXT REFERENCES quotes(quote_id),
    association_kind TEXT NOT NULL,
    manual INTEGER NOT NULL DEFAULT 0,
    associated_at INTEGER NOT NULL
);

CREATE TABLE reconciliation_runs (
    run_id TEXT PRIMARY KEY,
    order_id TEXT NOT NULL REFERENCES customer_orders(order_id) ON DELETE CASCADE,
    quote_id TEXT REFERENCES quotes(quote_id),
    status TEXT NOT NULL CHECK(status IN ('MATCHED', 'REVIEW', 'CRITICAL', 'INCOMPLETE')),
    incomplete_reasons TEXT NOT NULL DEFAULT '',
    policy_profile_id TEXT REFERENCES policy_profiles(policy_id),
    policy_version INTEGER,
    completed_at INTEGER NOT NULL,
    FOREIGN KEY(policy_profile_id, policy_version) REFERENCES policy_versions(policy_id, version)
);
CREATE INDEX reconciliation_runs_order ON reconciliation_runs(order_id, completed_at DESC);

CREATE TABLE reconciliation_differences (
    run_id TEXT NOT NULL REFERENCES reconciliation_runs(run_id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    difference_code TEXT NOT NULL,
    title TEXT NOT NULL,
    severity TEXT NOT NULL CHECK(severity IN ('REVIEW', 'CRITICAL')),
    subject TEXT NOT NULL,
    quote_value TEXT,
    order_value TEXT,
    PRIMARY KEY(run_id, position)
);
CREATE INDEX reconciliation_differences_code ON reconciliation_differences(difference_code);

CREATE TABLE audit_events (
    event_id TEXT PRIMARY KEY,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    status_snapshot TEXT,
    created_at INTEGER NOT NULL
);
CREATE INDEX audit_events_entity ON audit_events(entity_type, entity_id, created_at DESC);
"#;

fn insert_company(
    transaction: &Transaction<'_>,
    draft: &CompanyDraft,
) -> StorageResult<CompanyProfile> {
    let timestamp = now_millis();
    let profile = CompanyProfile {
        company_id: new_id("company"),
        company_name: draft.company_name.trim().to_owned(),
        default_currency: draft.default_currency.trim().to_uppercase(),
        created_at: timestamp,
        updated_at: timestamp,
    };
    transaction.execute(
        "INSERT INTO companies (company_id, company_name, default_currency, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![profile.company_id, profile.company_name, profile.default_currency, timestamp, timestamp],
    )?;
    Ok(profile)
}

fn insert_policy(
    transaction: &Transaction<'_>,
    company_id: &str,
    draft: &PolicyDraft,
    active: bool,
) -> StorageResult<PolicyProfile> {
    let timestamp = now_millis();
    let policy_id = new_id("policy");
    transaction.execute(
        "INSERT INTO policy_profiles (policy_id, company_id, name, current_version, active, archived, created_at, updated_at)
         VALUES (?1, ?2, ?3, 1, ?4, 0, ?5, ?5)",
        params![policy_id, company_id, draft.name.trim(), active, timestamp],
    )?;
    insert_policy_version(transaction, &policy_id, 1, draft, timestamp)?;
    Ok(PolicyProfile {
        id: policy_id,
        company_id: company_id.to_owned(),
        name: draft.name.trim().to_owned(),
        version: 1,
        created_at: timestamp,
        updated_at: timestamp,
        active,
        archived: false,
        maximum_discount_percent: draft.maximum_discount_percent,
        maximum_payment_term_days: draft.maximum_payment_term_days,
        minimum_margin_percent: draft.minimum_margin_percent,
        maximum_quote_validity_days: draft.maximum_quote_validity_days,
        controls: draft.controls,
        discount_severity: normalize_severity(&draft.discount_severity)?,
        payment_term_severity: normalize_severity(&draft.payment_term_severity)?,
    })
}

fn insert_policy_version(
    transaction: &Transaction<'_>,
    policy_id: &str,
    version: u32,
    draft: &PolicyDraft,
    timestamp: i64,
) -> StorageResult<()> {
    transaction.execute(
        "INSERT INTO policy_versions (
            policy_id, version, maximum_discount_percent, maximum_payment_term_days,
            minimum_margin_percent, maximum_quote_validity_days, arithmetic_enabled,
            discount_enabled, payment_terms_enabled, validity_enabled, customer_reference_enabled,
            document_hygiene_enabled, discount_severity, payment_term_severity, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            policy_id,
            version,
            draft.maximum_discount_percent,
            draft.maximum_payment_term_days,
            draft.minimum_margin_percent,
            draft.maximum_quote_validity_days,
            draft.controls.arithmetic,
            draft.controls.discount,
            draft.controls.payment_terms,
            draft.controls.validity,
            draft.controls.customer_reference,
            draft.controls.document_hygiene,
            normalize_severity(&draft.discount_severity)?,
            normalize_severity(&draft.payment_term_severity)?,
            timestamp,
        ],
    )?;
    Ok(())
}

fn insert_template(
    transaction: &Transaction<'_>,
    company_id: &str,
    draft: &TemplateDraft,
    is_default: bool,
) -> StorageResult<ProposalTemplateProfile> {
    validate_template(draft)?;
    let timestamp = now_millis();
    let template_id = new_id("template");
    transaction.execute(
        "INSERT INTO template_profiles (template_id, company_id, name, is_default, enabled, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5)",
        params![template_id, company_id, draft.name.trim(), is_default, timestamp],
    )?;
    insert_aliases(transaction, &template_id, &draft.aliases)?;
    Ok(ProposalTemplateProfile {
        id: template_id,
        company_id: company_id.to_owned(),
        name: draft.name.trim().to_owned(),
        is_default,
        enabled: true,
        created_at: timestamp,
        updated_at: timestamp,
        aliases: draft.aliases.clone(),
    })
}

fn insert_aliases(
    transaction: &Transaction<'_>,
    template_id: &str,
    aliases: &AliasSet,
) -> StorageResult<()> {
    let fields = FieldAliases::from(aliases.clone());
    for (field_key, values) in fields.fields() {
        for (position, alias) in values.iter().enumerate() {
            transaction.execute(
                "INSERT INTO field_aliases (template_id, field_key, alias, position) VALUES (?1, ?2, ?3, ?4)",
                params![template_id, field_key, alias.trim(), position],
            )?;
        }
    }
    Ok(())
}

fn template_with_aliases(
    connection: &Connection,
    basic: (String, String, String, bool, bool, i64, i64),
) -> StorageResult<ProposalTemplateProfile> {
    let mut aliases = FieldAliases {
        customer: Vec::new(),
        quote_number: Vec::new(),
        payment_term: Vec::new(),
        discount: Vec::new(),
        subtotal: Vec::new(),
        tax: Vec::new(),
        grand_total: Vec::new(),
        valid_until: Vec::new(),
    };
    let mut statement = connection.prepare(
        "SELECT field_key, alias FROM field_aliases WHERE template_id = ?1 ORDER BY field_key, position",
    )?;
    let rows = statement.query_map([&basic.0], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (key, alias) = row?;
        if let Some(values) = aliases.field_mut(&key) {
            values.push(alias);
        }
    }
    Ok(ProposalTemplateProfile {
        id: basic.0,
        company_id: basic.1,
        name: basic.2,
        is_default: basic.3,
        enabled: basic.4,
        created_at: basic.5,
        updated_at: basic.6,
        aliases: aliases.into(),
    })
}

fn set_setting(transaction: &Transaction<'_>, key: &str, value: &str) -> StorageResult<()> {
    transaction.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

fn company_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CompanyProfile> {
    Ok(CompanyProfile {
        company_id: row.get(0)?,
        company_name: row.get(1)?,
        default_currency: row.get(2)?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
    })
}

fn policy_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PolicyProfile> {
    Ok(PolicyProfile {
        id: row.get(0)?,
        company_id: row.get(1)?,
        name: row.get(2)?,
        version: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        active: row.get(6)?,
        archived: row.get(7)?,
        maximum_discount_percent: row.get(8)?,
        maximum_payment_term_days: row.get(9)?,
        minimum_margin_percent: row.get(10)?,
        maximum_quote_validity_days: row.get(11)?,
        controls: RuleControlSettings {
            arithmetic: row.get(12)?,
            discount: row.get(13)?,
            payment_terms: row.get(14)?,
            validity: row.get(15)?,
            customer_reference: row.get(16)?,
            document_hygiene: row.get(17)?,
        },
        discount_severity: row.get(18)?,
        payment_term_severity: row.get(19)?,
    })
}

fn validate_company(draft: &CompanyDraft) -> StorageResult<()> {
    validate_name("company name", &draft.company_name)?;
    let currency = draft.default_currency.trim();
    if currency.len() != 3
        || !currency
            .chars()
            .all(|character| character.is_ascii_alphabetic())
    {
        return Err(StorageError::Validation(
            "default currency must be a three-letter code".to_owned(),
        ));
    }
    Ok(())
}

fn validate_policy(draft: &PolicyDraft) -> StorageResult<()> {
    validate_name("policy name", &draft.name)?;
    validate_percentage("maximum discount", draft.maximum_discount_percent)?;
    if let Some(margin) = draft.minimum_margin_percent {
        validate_percentage("minimum margin", margin)?;
    }
    if draft.maximum_payment_term_days == 0 || draft.maximum_quote_validity_days == 0 {
        return Err(StorageError::Validation(
            "payment term and quote validity must be greater than zero".to_owned(),
        ));
    }
    normalize_severity(&draft.discount_severity)?;
    normalize_severity(&draft.payment_term_severity)?;
    Ok(())
}

fn validate_template(draft: &TemplateDraft) -> StorageResult<()> {
    validate_name("template name", &draft.name)?;
    let aliases = FieldAliases::from(draft.aliases.clone());
    for (field, values) in aliases.fields() {
        if values.is_empty() || values.iter().any(|value| value.trim().is_empty()) {
            return Err(StorageError::Validation(format!(
                "{field} must contain at least one non-empty alias"
            )));
        }
        let mut normalized = values
            .iter()
            .map(|value| {
                value
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_lowercase()
            })
            .collect::<Vec<_>>();
        normalized.sort();
        normalized.dedup();
        if normalized.len() != values.len() {
            return Err(StorageError::Validation(format!(
                "{field} contains duplicate aliases"
            )));
        }
    }
    Ok(())
}

fn validate_name(field: &str, value: &str) -> StorageResult<()> {
    if value.trim().is_empty() || value.trim().chars().count() > 120 {
        return Err(StorageError::Validation(format!(
            "{field} must contain 1 to 120 characters"
        )));
    }
    Ok(())
}

fn validate_percentage(field: &str, value: f64) -> StorageResult<()> {
    if !value.is_finite() || !(0.0..=100.0).contains(&value) {
        return Err(StorageError::Validation(format!(
            "{field} must be between 0 and 100"
        )));
    }
    Ok(())
}

fn parse_severity(value: &str) -> StorageResult<Severity> {
    match normalize_severity(value)?.as_str() {
        "REVIEW" => Ok(Severity::Review),
        "CRITICAL" => Ok(Severity::Critical),
        _ => unreachable!("normalized severity is constrained"),
    }
}

fn normalize_severity(value: &str) -> StorageResult<String> {
    let normalized = value.trim().to_uppercase();
    if matches!(normalized.as_str(), "REVIEW" | "CRITICAL") {
        Ok(normalized)
    } else {
        Err(StorageError::Validation(
            "configurable severity must be REVIEW or CRITICAL".to_owned(),
        ))
    }
}

fn validate_hash(value: &str) -> StorageResult<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(StorageError::Validation(
            "SHA-256 must contain 64 hexadecimal characters".to_owned(),
        ));
    }
    Ok(())
}

fn split_rule_ids(value: &str) -> Vec<String> {
    value
        .lines()
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn scalar_count(connection: &Connection, sql: &str) -> StorageResult<u64> {
    connection
        .query_row(sql, [], |row| row.get(0))
        .map_err(Into::into)
}

fn grouped_counts(connection: &Connection, sql: &str) -> StorageResult<Vec<CountByKey>> {
    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map([], |row| {
        Ok(CountByKey {
            key: row.get(0)?,
            count: row.get(1)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use commercial_preflight_reconciliation::{
        AssociationKind, QuoteAssociation, ReconciliationPolicy, reconcile,
    };

    struct TestDatabase {
        path: PathBuf,
        store: LocalStore,
    }

    impl TestDatabase {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "commercial-preflight-test-{}.sqlite3",
                new_id("db")
            ));
            let store = LocalStore::open(&path).expect("temporary database opens");
            Self { path, store }
        }

        fn setup(&self) -> SetupState {
            self.store
                .complete_onboarding(&sample_onboarding())
                .expect("onboarding completes")
        }
    }

    impl Drop for TestDatabase {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    fn sample_policy() -> PolicyDraft {
        PolicyDraft {
            name: "Standard Sales Policy".to_owned(),
            maximum_discount_percent: 10.0,
            maximum_payment_term_days: 60,
            minimum_margin_percent: Some(15.0),
            maximum_quote_validity_days: 30,
            controls: RuleControlSettings::default(),
            discount_severity: "REVIEW".to_owned(),
            payment_term_severity: "CRITICAL".to_owned(),
        }
    }

    fn sample_onboarding() -> OnboardingRequest {
        OnboardingRequest {
            company: CompanyDraft {
                company_name: "Example Technology".to_owned(),
                default_currency: "try".to_owned(),
            },
            policy: sample_policy(),
            template_name: "Example Standard Proposal".to_owned(),
        }
    }

    fn scan(
        state: &SetupState,
        id: &str,
        verdict: &str,
        format: &str,
        timestamp: i64,
    ) -> NewScanRecord {
        NewScanRecord {
            scan_id: id.to_owned(),
            timestamp,
            filename: format!("{id}.{format}"),
            sha256: "a".repeat(64),
            document_format: format.to_owned(),
            company_id: state.company.as_ref().expect("company").company_id.clone(),
            policy_profile_id: state.policy.as_ref().expect("policy").id.clone(),
            policy_version: state.policy.as_ref().expect("policy").version,
            template_profile_id: state.template.as_ref().expect("template").id.clone(),
            verdict: verdict.to_owned(),
            finding_count: usize::from(verdict != "PASS"),
            rule_ids: (verdict != "PASS")
                .then(|| "CP-POL-001".to_owned())
                .into_iter()
                .collect(),
            checks_completed: 5,
            checks_not_applicable: 1,
            checks_incomplete: usize::from(verdict == "INCOMPLETE"),
            duration_ms: 12,
        }
    }

    fn test_field<T>(value: T) -> ProposalField<T> {
        ProposalField::new(value, SourceEvidence::metadata(None))
    }

    fn test_proposal(number: &str, revision: &str, hash_seed: char) -> NormalizedProposal {
        let mut proposal = NormalizedProposal::new(DocumentFormat::Pdf, "synthetic.pdf");
        proposal.customer = Some(test_field("Example Electrical".to_owned()));
        proposal.quote_number = Some(test_field(number.to_owned()));
        proposal.quote_revision = Some(test_field(revision.to_owned()));
        proposal.order_number = Some(test_field("PO-1".to_owned()));
        proposal.currency = Some(test_field("EUR".to_owned()));
        proposal.payment_term_days = Some(test_field(30));
        proposal.delivery_term = Some(test_field("3 weeks".to_owned()));
        proposal.subtotal = Some(test_field(100.0));
        proposal.tax = Some(test_field(0.0));
        proposal.grand_total = Some(test_field(100.0));
        proposal.line_items.push(LineItem {
            sku: Some(test_field("SKU-1".to_owned())),
            description: Some(test_field("Synthetic line".to_owned())),
            quantity: Some(test_field(1.0)),
            uom: Some(test_field("EA".to_owned())),
            unit_price: Some(test_field(100.0)),
            discount_percent: Some(test_field(0.0)),
            line_total: Some(test_field(100.0)),
        });
        proposal
            .source
            .metadata
            .push(commercial_preflight_core::SourceMetadata {
                name: "hash-seed".to_owned(),
                value: hash_seed.to_string(),
                evidence: SourceEvidence::metadata(None),
            });
        proposal
    }

    fn test_quote(revision: &str, lifecycle: QuoteLifecycle, hash_seed: char) -> Quote {
        Quote::from_normalized(
            &test_proposal("Q-1", revision, hash_seed),
            format!("quote-{revision}"),
            lifecycle,
            "missing-quote.pdf",
            hash_seed.to_string().repeat(64),
            now_millis(),
        )
        .expect("quote")
    }

    fn test_order(id: &str, hash_seed: char) -> CustomerOrder {
        CustomerOrder::from_normalized(
            &test_proposal("Q-1", "3", hash_seed),
            id,
            "missing-order.pdf",
            hash_seed.to_string().repeat(64),
            now_millis(),
        )
    }

    #[test]
    fn fresh_database_requires_onboarding() {
        let database = TestDatabase::new();
        assert!(
            database
                .store
                .setup_state()
                .expect("state")
                .onboarding_required
        );
    }

    #[test]
    fn company_create_read_and_update_are_persistent() {
        let database = TestDatabase::new();
        let state = database.setup();
        let company = state.company.expect("company created");
        let updated = database
            .store
            .update_company(
                &company.company_id,
                &CompanyDraft {
                    company_name: "Example Technology A.Ş.".to_owned(),
                    default_currency: "EUR".to_owned(),
                },
            )
            .expect("company updates");
        assert_eq!(updated.company_name, "Example Technology A.Ş.");
        assert_eq!(
            database.store.active_company().expect("company reads"),
            Some(updated)
        );
    }

    #[test]
    fn policy_validation_rejects_unsafe_severity_and_invalid_limits() {
        let database = TestDatabase::new();
        let state = database.setup();
        let company = state.company.expect("company");
        let mut invalid = sample_policy();
        invalid.maximum_discount_percent = 101.0;
        invalid.discount_severity = "PASS".to_owned();
        assert!(
            database
                .store
                .create_policy(&company.company_id, &invalid)
                .is_err()
        );
    }

    #[test]
    fn policy_update_creates_an_immutable_new_version() {
        let database = TestDatabase::new();
        let state = database.setup();
        let policy = state.policy.expect("policy");
        let mut revision = sample_policy();
        revision.maximum_discount_percent = 8.0;
        let updated = database
            .store
            .update_policy(&policy.id, &revision)
            .expect("revision saves");
        assert_eq!(updated.version, 2);
        let connection = database.store.connection().expect("connection");
        let versions: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM policy_versions WHERE policy_id = ?1",
                [&policy.id],
                |row| row.get(0),
            )
            .expect("versions count");
        assert_eq!(versions, 2);
    }

    #[test]
    fn policy_crud_and_active_selection_work() {
        let database = TestDatabase::new();
        let state = database.setup();
        let company = state.company.expect("company");
        let second = database
            .store
            .create_policy(
                &company.company_id,
                &PolicyDraft {
                    name: "Export Policy".to_owned(),
                    ..sample_policy()
                },
            )
            .expect("policy creates");
        database
            .store
            .set_active_policy(&company.company_id, &second.id)
            .expect("active changes");
        let active = database
            .store
            .active_policy(&company.company_id)
            .expect("active reads")
            .expect("active");
        assert_eq!(active.id, second.id);
        let original = database
            .store
            .list_policies(&company.company_id)
            .expect("policies")
            .into_iter()
            .find(|item| item.id != second.id)
            .expect("original");
        database
            .store
            .archive_policy(&original.id)
            .expect("inactive policy archives");
    }

    #[test]
    fn template_aliases_round_trip_and_default_changes() {
        let database = TestDatabase::new();
        let state = database.setup();
        let company = state.company.expect("company");
        let mut aliases = AliasSet::default();
        aliases.customer.push("Alıcı".to_owned());
        let created = database
            .store
            .create_template(
                &company.company_id,
                &TemplateDraft {
                    name: "Export Template".to_owned(),
                    aliases: aliases.clone(),
                },
            )
            .expect("template creates");
        aliases.discount.push("İndirim Oranı".to_owned());
        let updated = database
            .store
            .update_template(
                &created.id,
                &TemplateDraft {
                    name: "Export Proposal".to_owned(),
                    aliases: aliases.clone(),
                },
            )
            .expect("template updates");
        assert_eq!(updated.name, "Export Proposal");
        database
            .store
            .set_default_template(&company.company_id, &created.id)
            .expect("default changes");
        let loaded = database
            .store
            .default_template(&company.company_id)
            .expect("template reads")
            .expect("default");
        assert_eq!(loaded.aliases.customer, aliases.customer);
        assert_eq!(loaded.aliases.discount, aliases.discount);
        let previous_default = database
            .store
            .list_templates(&company.company_id)
            .expect("templates list")
            .into_iter()
            .find(|template| template.id != created.id)
            .expect("previous default exists");
        database
            .store
            .disable_template(&previous_default.id)
            .expect("non-default template disables safely");
    }

    #[test]
    fn default_template_cannot_be_disabled() {
        let database = TestDatabase::new();
        let state = database.setup();
        assert!(
            database
                .store
                .disable_template(&state.template.expect("template").id)
                .is_err()
        );
    }

    #[test]
    fn migrations_are_idempotent() {
        let database = TestDatabase::new();
        assert_eq!(database.store.migration_version().expect("version"), 2);
        let reopened = LocalStore::open(&database.path).expect("database reopens");
        assert_eq!(reopened.migration_version().expect("version"), 2);
    }

    #[test]
    fn existing_v1_database_migrates_to_v2_without_recreation() {
        let path =
            std::env::temp_dir().join(format!("commercial-preflight-v1-{}.sqlite3", new_id("db")));
        let connection = Connection::open(&path).expect("v1 database");
        connection
            .execute_batch(
                "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at INTEGER NOT NULL);",
            )
            .expect("migration table");
        connection.execute_batch(MIGRATION_1).expect("v1 schema");
        connection
            .execute("INSERT INTO schema_migrations VALUES (1, 1)", [])
            .expect("v1 marker");
        connection
            .execute(
                "INSERT INTO app_settings (key, value) VALUES ('preserved', 'yes')",
                [],
            )
            .expect("v1 data");
        drop(connection);
        let store = LocalStore::open(&path).expect("v2 migration");
        assert_eq!(store.migration_version().expect("version"), 2);
        let preserved: String = store
            .connection()
            .expect("connection")
            .query_row(
                "SELECT value FROM app_settings WHERE key = 'preserved'",
                [],
                |row| row.get(0),
            )
            .expect("v1 data preserved");
        assert_eq!(preserved, "yes");
        drop(store);
        std::fs::remove_file(path).expect("temporary database removes");
    }

    #[test]
    fn quote_revisions_persist_and_new_baseline_supersedes_prior_revision() {
        let database = TestDatabase::new();
        database.setup();
        database
            .store
            .register_quote(&test_quote("2", QuoteLifecycle::Sent, 'b'), "MANUAL")
            .expect("rev 2 registers");
        database
            .store
            .register_quote(
                &test_quote("3", QuoteLifecycle::AcceptedBaseline, 'c'),
                "MANUAL",
            )
            .expect("rev 3 registers");
        let quotes = database.store.list_quotes().expect("quotes list");
        assert_eq!(quotes.len(), 2);
        assert_eq!(
            quotes
                .iter()
                .find(|quote| quote.revision.as_deref() == Some("2"))
                .expect("rev 2")
                .lifecycle,
            "SUPERSEDED"
        );
        assert_eq!(
            database.store.load_quotes().expect("domain quotes").len(),
            2
        );
    }

    #[test]
    fn matched_order_creates_no_exception_and_review_does_not_change_result() {
        let database = TestDatabase::new();
        let setup = database.setup();
        let quote = test_quote("3", QuoteLifecycle::Sent, 'd');
        let order = test_order("order-matched", 'e');
        database
            .store
            .register_quote(&quote, "MANUAL")
            .expect("quote");
        database
            .store
            .register_order(&order, "MANUAL")
            .expect("order");
        database
            .store
            .persist_association(
                &order.order_id,
                &QuoteAssociation {
                    kind: AssociationKind::ExactRevision,
                    quote_id: Some(quote.quote_id.clone()),
                    candidate_quote_ids: Vec::new(),
                },
                false,
            )
            .expect("association");
        let result = reconcile(&quote, &order, &ReconciliationPolicy::default());
        database
            .store
            .persist_reconciliation(
                &result,
                setup.policy.as_ref().map(|policy| policy.id.as_str()),
                setup.policy.as_ref().map(|policy| policy.version),
            )
            .expect("result");
        assert!(database.store.exceptions().expect("exceptions").is_empty());
        let before = database
            .store
            .order_detail(&order.order_id)
            .expect("detail");
        assert_eq!(before.summary.status, "MATCHED");
        database
            .store
            .mark_order_reviewed(&order.order_id)
            .expect("review");
        let after = database
            .store
            .order_detail(&order.order_id)
            .expect("detail");
        assert_eq!(after.summary.status, "MATCHED");
        assert_eq!(after.reviewed_status.as_deref(), Some("MATCHED"));
    }

    #[test]
    fn changed_order_is_an_exception_until_reviewed_and_audit_is_retained() {
        let database = TestDatabase::new();
        database.setup();
        let quote = test_quote("3", QuoteLifecycle::Sent, 'f');
        let mut order = test_order("order-changed", '1');
        order.document.line_items[0].unit_price = Some(test_field(110.0));
        database
            .store
            .register_quote(&quote, "MANUAL")
            .expect("quote");
        database
            .store
            .register_order(&order, "MANUAL")
            .expect("order");
        let association = QuoteAssociation {
            kind: AssociationKind::ExactRevision,
            quote_id: Some(quote.quote_id.clone()),
            candidate_quote_ids: Vec::new(),
        };
        database
            .store
            .persist_association(&order.order_id, &association, false)
            .expect("association");
        let result = reconcile(&quote, &order, &ReconciliationPolicy::default());
        database
            .store
            .persist_reconciliation(&result, None, None)
            .expect("result");
        assert_eq!(database.store.exceptions().expect("exceptions").len(), 1);
        database
            .store
            .mark_order_reviewed(&order.order_id)
            .expect("review");
        assert!(database.store.exceptions().expect("exceptions").is_empty());
        let events = database
            .store
            .audit_events(&order.order_id)
            .expect("events");
        assert!(
            events
                .iter()
                .any(|event| event.event_type == "ORDER_RECEIVED")
        );
        assert!(
            events
                .iter()
                .any(|event| event.event_type == "EXCEPTION_REVIEWED")
        );
    }

    #[test]
    fn intake_hash_deduplication_and_deleted_source_are_safe() {
        let database = TestDatabase::new();
        database.setup();
        let source = std::env::temp_dir().join(format!(
            "commercial-preflight-source-{}.pdf",
            new_id("file")
        ));
        std::fs::write(&source, b"synthetic source").expect("source writes");
        let mut quote = test_quote("3", QuoteLifecycle::Sent, '2');
        quote.document.source.source_path = source.to_string_lossy().into_owned();
        assert!(
            database
                .store
                .register_quote(&quote, "WATCHED_FOLDER")
                .expect("first import")
        );
        assert!(
            !database
                .store
                .register_quote(&quote, "WATCHED_FOLDER")
                .expect("dedupe")
        );
        assert!(database.store.list_quotes().expect("quotes")[0].source_available);
        std::fs::remove_file(&source).expect("source deletes");
        assert!(!database.store.list_quotes().expect("quotes")[0].source_available);
    }

    #[test]
    fn history_persists_and_filters_by_verdict_format_and_date() {
        let database = TestDatabase::new();
        let state = database.setup();
        database
            .store
            .insert_scan(&scan(&state, "one", "PASS", "XLSX", 1000))
            .expect("scan one");
        database
            .store
            .insert_scan(&scan(&state, "two", "CRITICAL", "PDF", 2000))
            .expect("scan two");
        let filtered = database
            .store
            .history(&HistoryFilter {
                verdict: Some("CRITICAL".to_owned()),
                document_format: Some("PDF".to_owned()),
                from_timestamp: Some(1500),
                to_timestamp: Some(2500),
            })
            .expect("history filters");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].scan_id, "two");
    }

    #[test]
    fn clearing_history_preserves_configuration() {
        let database = TestDatabase::new();
        let state = database.setup();
        database
            .store
            .insert_scan(&scan(&state, "one", "PASS", "XLSX", 1000))
            .expect("scan");
        assert_eq!(database.store.clear_history().expect("clear"), 1);
        let after = database.store.setup_state().expect("configuration remains");
        assert!(!after.onboarding_required);
        assert!(
            database
                .store
                .history(&HistoryFilter::default())
                .expect("history")
                .is_empty()
        );
    }

    #[test]
    fn aggregates_cover_verdicts_rules_formats_weeks_and_months() {
        let database = TestDatabase::new();
        let state = database.setup();
        database
            .store
            .insert_scan(&scan(&state, "one", "PASS", "XLSX", 1_762_000_000_000))
            .expect("scan one");
        database
            .store
            .insert_scan(&scan(&state, "two", "CRITICAL", "PDF", 1_762_100_000_000))
            .expect("scan two");
        let aggregates = database.store.aggregates().expect("aggregates");
        assert_eq!(aggregates.total_proposals, 2);
        assert_eq!(aggregates.pass_count, 1);
        assert_eq!(aggregates.critical_count, 1);
        assert_eq!(aggregates.findings_by_rule[0].count, 1);
        assert_eq!(aggregates.scans_by_format.len(), 2);
        assert!(!aggregates.weekly_counts.is_empty());
        assert!(!aggregates.monthly_counts.is_empty());
    }

    #[test]
    fn sha256_file_hashes_the_document_without_storing_it() {
        let path =
            std::env::temp_dir().join(format!("commercial-preflight-hash-{}.txt", new_id("file")));
        std::fs::write(&path, b"proposal bytes").expect("fixture writes");
        let hash = sha256_file(&path).expect("hash computes");
        assert_eq!(hash.len(), 64);
        std::fs::remove_file(path).expect("fixture removes");
    }

    #[test]
    fn history_schema_has_no_proposal_body_or_evidence_columns() {
        let database = TestDatabase::new();
        let connection = database.store.connection().expect("connection");
        let mut statement = connection
            .prepare("PRAGMA table_info(scan_history)")
            .expect("schema query");
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))
            .expect("columns")
            .collect::<Result<Vec<_>, _>>()
            .expect("column values");
        for forbidden in [
            "content",
            "text",
            "evidence",
            "line_items",
            "financial_values",
        ] {
            assert!(
                !columns.iter().any(|column| column.contains(forbidden)),
                "forbidden column {forbidden}"
            );
        }
    }

    #[test]
    fn historical_scan_keeps_the_policy_version_used() {
        let database = TestDatabase::new();
        let state = database.setup();
        database
            .store
            .insert_scan(&scan(&state, "v1", "REVIEW", "XLSX", 1000))
            .expect("v1 scan");
        let policy = state.policy.as_ref().expect("policy");
        database
            .store
            .update_policy(
                &policy.id,
                &PolicyDraft {
                    maximum_discount_percent: 5.0,
                    ..sample_policy()
                },
            )
            .expect("v2 policy");
        let history = database
            .store
            .history(&HistoryFilter::default())
            .expect("history");
        assert_eq!(history[0].policy_version, 1);
    }
}
