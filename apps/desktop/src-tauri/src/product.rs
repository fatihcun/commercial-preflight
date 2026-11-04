use std::path::Path;
use std::time::Instant;

use commercial_preflight_core::{CustomerOrder, Date, Quote, QuoteLifecycle};
use commercial_preflight_engine::{parse_commercial_file, scan_file_with_aliases};
use commercial_preflight_intake::{IntakeCandidate, IntakeDocumentKind, manual_candidate};
use commercial_preflight_reconciliation::{
    AssociationKind, QuoteAssociation, ReconciliationPolicy, match_quote, old_revision_result,
    reconcile,
};
use commercial_preflight_storage::{
    HistoryFilter, IncompleteIntakeRecord, LocalStore, NewScanRecord, OrderDetail, OrderSummary,
    QuoteRecord, ScanHistoryEntry, SetupState, new_id, now_millis, sha256_file,
};
use serde::Serialize;

use crate::domain::{ENFORCEMENT_MODE, ScanRequest, ScanResponse, ScanResultView};

pub fn scan_proposal(store: &LocalStore, request: ScanRequest) -> ScanResponse {
    let path = Path::new(request.path.trim());
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Proposal".to_owned());
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !extension.eq_ignore_ascii_case("xlsx") && !extension.eq_ignore_ascii_case("pdf") {
        return ScanResponse::Unsupported {
            file_name,
            title: "Unsupported file",
            message: "Commercial Preflight currently supports XLSX and PDF proposals.",
        };
    }

    let setup = match store.setup_state() {
        Ok(setup) if !setup.onboarding_required => setup,
        _ => {
            return incomplete(
                file_name,
                "Company setup is required before a proposal can be checked.",
            );
        }
    };
    let company = setup.company.expect("complete setup includes a company");
    let policy_profile = setup.policy.expect("complete setup includes a policy");
    let template = setup.template.expect("complete setup includes a template");
    let policy = match policy_profile.to_policy(&company) {
        Ok(policy) => policy,
        Err(_) => {
            return incomplete(
                file_name,
                "The active commercial policy could not be loaded.",
            );
        }
    };
    let as_of = match request.as_of {
        Some(value) => match Date::parse_supported(&value) {
            Ok(date) => date,
            Err(_) => return incomplete(file_name, "The requested check date was not valid."),
        },
        None => match Date::today_utc() {
            Ok(date) => date,
            Err(_) => {
                return incomplete(
                    file_name,
                    "The current date could not be determined safely.",
                );
            }
        },
    };
    let sha256 = match sha256_file(path) {
        Ok(hash) => hash,
        Err(_) => {
            return incomplete(
                file_name,
                "The proposal could not be opened. It may have moved or you may not have access.",
            );
        }
    };

    let started = Instant::now();
    let aliases = template.aliases.clone().into();
    let result = scan_file_with_aliases(path, &policy, &aliases, as_of);
    let duration_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let scan_id = new_id("scan");
    let record = NewScanRecord {
        scan_id: scan_id.clone(),
        timestamp: now_millis(),
        filename: result.file_name.clone(),
        sha256,
        document_format: extension.to_uppercase(),
        company_id: company.company_id,
        policy_profile_id: policy_profile.id.clone(),
        policy_version: policy_profile.version,
        template_profile_id: template.id,
        verdict: result.verdict.as_str().to_owned(),
        finding_count: result.findings.len(),
        rule_ids: result
            .findings
            .iter()
            .map(|finding| finding.rule_id.to_string())
            .collect(),
        checks_completed: result.checks_run,
        checks_not_applicable: result.checks_not_applicable,
        checks_incomplete: result.checks_incomplete,
        duration_ms,
    };
    if store.insert_scan(&record).is_err() {
        return incomplete(
            result.file_name,
            "The check completed, but its local history record could not be saved.",
        );
    }
    let mut view = ScanResultView::from(result);
    view.scan_id = Some(scan_id);
    view.policy_name = Some(policy_profile.name);
    view.policy_version = Some(policy_profile.version);
    view.template_name = Some(template.name);
    ScanResponse::Completed {
        result: Box::new(view),
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub processed: bool,
    pub duplicate: bool,
    pub entity_id: Option<String>,
    pub status: String,
    pub message: String,
}

pub fn import_quote(
    store: &LocalStore,
    path: &str,
    lifecycle: QuoteLifecycle,
) -> Result<ImportResult, String> {
    let candidate = manual_candidate(path, IntakeDocumentKind::Quote)
        .map_err(|_| "The quote could not be opened safely.".to_owned())?;
    process_intake_candidate(store, &candidate, Some(lifecycle))
}

pub fn import_order(store: &LocalStore, path: &str) -> Result<ImportResult, String> {
    let candidate = manual_candidate(path, IntakeDocumentKind::CustomerOrder)
        .map_err(|_| "The customer order could not be opened safely.".to_owned())?;
    process_intake_candidate(store, &candidate, None)
}

pub fn process_intake_candidate(
    store: &LocalStore,
    candidate: &IntakeCandidate,
    quote_lifecycle: Option<QuoteLifecycle>,
) -> Result<ImportResult, String> {
    if store
        .has_document_hash(&candidate.sha256)
        .map_err(safe_error)?
    {
        return Ok(ImportResult {
            processed: false,
            duplicate: true,
            entity_id: None,
            status: "DUPLICATE".to_owned(),
            message: "Identical document already processed.".to_owned(),
        });
    }
    let setup = store.setup_state().map_err(safe_error)?;
    if setup.onboarding_required {
        return Err("Company setup is required before documents can be imported.".to_owned());
    }
    let template = setup
        .template
        .as_ref()
        .ok_or_else(|| "A proposal template is required.".to_owned())?;
    let aliases = template.aliases.clone().into();
    let source_path = candidate.path.to_string_lossy().into_owned();
    let source_kind = candidate.source_kind.as_str();
    let normalized = match parse_commercial_file(&candidate.path, &aliases) {
        Ok(normalized) => normalized,
        Err(error) => {
            let reason = error.incomplete_reason();
            let filename = candidate
                .path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Commercial document".to_owned());
            let format = candidate
                .path
                .extension()
                .map(|extension| extension.to_string_lossy().to_uppercase())
                .unwrap_or_default();
            let kind = match candidate.document_kind {
                IntakeDocumentKind::Quote => "QUOTE",
                IntakeDocumentKind::CustomerOrder => "CUSTOMER_ORDER",
            };
            let entity_id = store
                .register_incomplete_intake(&IncompleteIntakeRecord {
                    document_kind: kind.to_owned(),
                    source_kind: source_kind.to_owned(),
                    sha256: candidate.sha256.clone(),
                    source_path,
                    filename,
                    document_format: format,
                    reason: reason.clone(),
                })
                .map_err(safe_error)?;
            return Ok(ImportResult {
                processed: entity_id.is_some(),
                duplicate: entity_id.is_none(),
                entity_id,
                status: "INCOMPLETE".to_owned(),
                message: reason,
            });
        }
    };
    match candidate.document_kind {
        IntakeDocumentKind::Quote => {
            let quote_id = new_id("quote");
            let quote = Quote::from_normalized(
                &normalized,
                quote_id.clone(),
                quote_lifecycle.unwrap_or(QuoteLifecycle::Sent),
                source_path,
                candidate.sha256.clone(),
                now_millis(),
            )
            .map_err(str::to_owned)?;
            if !store
                .register_quote(&quote, source_kind)
                .map_err(safe_error)?
            {
                return duplicate_result();
            }
            let backfilled =
                backfill_pending_orders_for_quote(store, &quote.identity.quote_number, &setup)?;
            let message = if backfilled == 0 {
                "Quote registered.".to_owned()
            } else {
                format!(
                    "Quote registered. {backfilled} pending {} automatically reconciled.",
                    if backfilled == 1 { "order" } else { "orders" }
                )
            };
            Ok(ImportResult {
                processed: true,
                duplicate: false,
                entity_id: Some(quote_id),
                status: quote.lifecycle.as_str().to_owned(),
                message,
            })
        }
        IntakeDocumentKind::CustomerOrder => {
            let order_id = new_id("order");
            let order = CustomerOrder::from_normalized(
                &normalized,
                order_id.clone(),
                source_path,
                candidate.sha256.clone(),
                now_millis(),
            );
            if !store
                .register_order(&order, source_kind)
                .map_err(safe_error)?
            {
                return duplicate_result();
            }
            reconcile_order(store, &order, &setup)?;
            let status = store
                .order_inbox()
                .map_err(safe_error)?
                .into_iter()
                .find(|entry| entry.order_id == order_id)
                .map_or_else(|| "INCOMPLETE".to_owned(), |entry| entry.status);
            Ok(ImportResult {
                processed: true,
                duplicate: false,
                entity_id: Some(order_id),
                status,
                message: "Customer order processed.".to_owned(),
            })
        }
    }
}

fn duplicate_result() -> Result<ImportResult, String> {
    Ok(ImportResult {
        processed: false,
        duplicate: true,
        entity_id: None,
        status: "DUPLICATE".to_owned(),
        message: "Identical document already processed.".to_owned(),
    })
}

fn reconcile_order(
    store: &LocalStore,
    order: &CustomerOrder,
    setup: &SetupState,
) -> Result<(), String> {
    let quotes = store.load_quotes().map_err(safe_error)?;
    let association = match_quote(order, &quotes);
    store
        .persist_association(&order.order_id, &association, false)
        .map_err(safe_error)?;
    let policy_profile = setup.policy.as_ref();
    let policy_id = policy_profile.map(|policy| policy.id.as_str());
    let policy_version = policy_profile.map(|policy| policy.version);
    match association.kind {
        AssociationKind::ExactRevision | AssociationKind::UnambiguousActive => {
            let quote_id = association
                .quote_id
                .as_deref()
                .ok_or_else(|| "The quote association was incomplete.".to_owned())?;
            let quote = quotes
                .iter()
                .find(|quote| quote.quote_id == quote_id)
                .ok_or_else(|| "The associated quote is unavailable.".to_owned())?;
            let result = reconcile(quote, order, &ReconciliationPolicy::default());
            store
                .persist_reconciliation(&result, policy_id, policy_version)
                .map_err(safe_error)?;
        }
        AssociationKind::OldRevision => {
            let quote_id = association
                .quote_id
                .as_deref()
                .ok_or_else(|| "The old quote revision is unavailable.".to_owned())?;
            let old_quote = quotes
                .iter()
                .find(|quote| quote.quote_id == quote_id)
                .ok_or_else(|| "The old quote revision is unavailable.".to_owned())?;
            let active_revision = quotes
                .iter()
                .find(|quote| {
                    quote.identity.quote_number == old_quote.identity.quote_number
                        && quote.lifecycle != QuoteLifecycle::Superseded
                })
                .and_then(|quote| quote.identity.revision.as_ref())
                .map(|revision| revision.as_str());
            let result = old_revision_result(
                old_quote,
                order,
                active_revision,
                &ReconciliationPolicy::default(),
            );
            store
                .persist_reconciliation(&result, policy_id, policy_version)
                .map_err(safe_error)?;
        }
        AssociationKind::UnknownRevision => {
            let reference = order
                .quote_reference
                .as_ref()
                .map(|field| field.value.trim().to_uppercase())
                .unwrap_or_default();
            let active = quotes.iter().find(|quote| {
                quote.identity.quote_number.trim().to_uppercase() == reference
                    && matches!(
                        quote.lifecycle,
                        QuoteLifecycle::Sent | QuoteLifecycle::AcceptedBaseline
                    )
            });
            if let Some(active) = active {
                let active_revision = active
                    .identity
                    .revision
                    .as_ref()
                    .map(|revision| revision.as_str());
                let result = old_revision_result(
                    active,
                    order,
                    active_revision,
                    &ReconciliationPolicy::default(),
                );
                store
                    .persist_reconciliation(&result, policy_id, policy_version)
                    .map_err(safe_error)?;
            } else {
                store
                    .persist_unmatched_reconciliation(
                        &order.order_id,
                        association_reason(association.kind),
                        policy_id,
                        policy_version,
                    )
                    .map_err(safe_error)?;
            }
        }
        _ => {
            store
                .persist_unmatched_reconciliation(
                    &order.order_id,
                    association_reason(association.kind),
                    policy_id,
                    policy_version,
                )
                .map_err(safe_error)?;
        }
    }
    Ok(())
}

fn deterministic_backfill_candidate<'a>(
    order: &CustomerOrder,
    quotes: &'a [Quote],
) -> Option<(QuoteAssociation, &'a Quote)> {
    let association = match_quote(order, quotes);
    if !matches!(
        association.kind,
        AssociationKind::ExactRevision | AssociationKind::UnambiguousActive
    ) {
        return None;
    }
    let quote = quotes
        .iter()
        .find(|quote| Some(quote.quote_id.as_str()) == association.quote_id.as_deref())?;
    Some((association, quote))
}

fn backfill_pending_orders_for_quote(
    store: &LocalStore,
    quote_number: &str,
    setup: &SetupState,
) -> Result<usize, String> {
    let quotes = store.load_quotes().map_err(safe_error)?;
    let pending = store
        .pending_orders_for_quote(quote_number)
        .map_err(safe_error)?;
    let policy_profile = setup.policy.as_ref();
    let policy_id = policy_profile.map(|policy| policy.id.as_str());
    let policy_version = policy_profile.map(|policy| policy.version);
    let mut reconciled = 0;
    for order in pending {
        let Some((association, quote)) = deterministic_backfill_candidate(&order, &quotes) else {
            continue;
        };
        store
            .persist_association(&order.order_id, &association, false)
            .map_err(safe_error)?;
        let result = reconcile(quote, &order, &ReconciliationPolicy::default());
        store
            .persist_reconciliation(&result, policy_id, policy_version)
            .map_err(safe_error)?;
        store
            .record_automatic_backfill(&order.order_id, result.status.as_str())
            .map_err(safe_error)?;
        reconciled += 1;
    }
    Ok(reconciled)
}

pub fn set_quote_lifecycle(
    store: &LocalStore,
    quote_id: &str,
    lifecycle: QuoteLifecycle,
) -> Result<Vec<QuoteRecord>, String> {
    store
        .set_quote_lifecycle(quote_id, lifecycle)
        .map_err(safe_error)?;
    let quote = store.load_quote(quote_id).map_err(safe_error)?;
    let setup = store.setup_state().map_err(safe_error)?;
    backfill_pending_orders_for_quote(store, &quote.identity.quote_number, &setup)?;
    quotes(store)
}

fn association_reason(kind: AssociationKind) -> &'static str {
    match kind {
        AssociationKind::MissingReference => "Quote reference was not identified",
        AssociationKind::UnknownQuote => "Referenced quote is not registered",
        AssociationKind::UnknownRevision => "Referenced quote revision is not registered",
        AssociationKind::Ambiguous => "Quote association requires manual selection",
        _ => "Quote association could not be completed",
    }
}

pub fn manually_associate(
    store: &LocalStore,
    order_id: &str,
    quote_id: &str,
) -> Result<OrderDetail, String> {
    let order = store.load_order(order_id).map_err(safe_error)?;
    let quote = store.load_quote(quote_id).map_err(safe_error)?;
    let association = QuoteAssociation {
        kind: AssociationKind::Manual,
        quote_id: Some(quote_id.to_owned()),
        candidate_quote_ids: Vec::new(),
    };
    store
        .persist_association(order_id, &association, true)
        .map_err(safe_error)?;
    let result = reconcile(&quote, &order, &ReconciliationPolicy::default());
    let setup = store.setup_state().map_err(safe_error)?;
    store
        .persist_reconciliation(
            &result,
            setup.policy.as_ref().map(|policy| policy.id.as_str()),
            setup.policy.as_ref().map(|policy| policy.version),
        )
        .map_err(safe_error)?;
    store.order_detail(order_id).map_err(safe_error)
}

pub fn quotes(store: &LocalStore) -> Result<Vec<QuoteRecord>, String> {
    store.list_quotes().map_err(safe_error)
}

pub fn orders(store: &LocalStore) -> Result<Vec<OrderSummary>, String> {
    store.order_inbox().map_err(safe_error)
}

pub fn exceptions(store: &LocalStore) -> Result<Vec<OrderSummary>, String> {
    store.exceptions().map_err(safe_error)
}

fn incomplete(file_name: String, reason: &str) -> ScanResponse {
    ScanResponse::Completed {
        result: Box::new(ScanResultView {
            schema_version: "0.1".to_owned(),
            file_name,
            verdict: "INCOMPLETE".to_owned(),
            findings: Vec::new(),
            checks_run: 0,
            checks_not_applicable: 0,
            checks_incomplete: 1,
            as_of_date: String::new(),
            incomplete_reason: Some(reason.to_owned()),
            enforcement_mode: ENFORCEMENT_MODE,
            scan_id: None,
            policy_name: None,
            policy_version: None,
            template_name: None,
        }),
    }
}

pub fn recent_history(store: &LocalStore) -> Result<Vec<ScanHistoryEntry>, String> {
    store.history(&HistoryFilter::default()).map_err(safe_error)
}

pub fn setup_state(store: &LocalStore) -> Result<SetupState, String> {
    store.setup_state().map_err(safe_error)
}

pub fn safe_error(error: commercial_preflight_storage::StorageError) -> String {
    match error {
        commercial_preflight_storage::StorageError::Validation(message) => message,
        commercial_preflight_storage::StorageError::NotFound => {
            "The requested local configuration was not found.".to_owned()
        }
        _ => "The local Commercial Preflight database operation could not be completed.".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use commercial_preflight_core::FieldAliases;
    use commercial_preflight_storage::{
        CompanyDraft, OnboardingRequest, PolicyDraft, RuleControlSettings,
    };

    use super::*;

    fn repository_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
    }

    fn fixture(name: &str) -> String {
        repository_root()
            .join("tests/fixtures")
            .join(name)
            .to_string_lossy()
            .into_owned()
    }

    fn configured_store() -> (PathBuf, LocalStore) {
        let database_path = std::env::temp_dir().join(format!(
            "commercial-preflight-product-test-{}.sqlite3",
            new_id("db")
        ));
        let store = LocalStore::open(&database_path).expect("temporary database opens");
        store
            .complete_onboarding(&OnboardingRequest {
                company: CompanyDraft {
                    company_name: "Example Technology".to_owned(),
                    default_currency: "EUR".to_owned(),
                },
                policy: PolicyDraft {
                    name: "Standard Sales Policy".to_owned(),
                    maximum_discount_percent: 10.0,
                    maximum_payment_term_days: 60,
                    minimum_margin_percent: Some(15.0),
                    maximum_quote_validity_days: 30,
                    controls: RuleControlSettings::default(),
                    discount_severity: "REVIEW".to_owned(),
                    payment_term_severity: "CRITICAL".to_owned(),
                },
                template_name: "Example Standard Proposal".to_owned(),
            })
            .expect("setup completes");
        (database_path, store)
    }

    #[test]
    fn configured_scan_uses_profiles_and_persists_privacy_safe_history() {
        let (database_path, store) = configured_store();
        let response = scan_proposal(
            &store,
            ScanRequest {
                path: repository_root()
                    .join("tests/fixtures/Project_Orion_Quote_v0.1.xlsx")
                    .to_string_lossy()
                    .into_owned(),
                policy_id: None,
                as_of: Some("2026-10-30".to_owned()),
            },
        );
        let result = match response {
            ScanResponse::Completed { result } => *result,
            ScanResponse::Unsupported { .. } => panic!("XLSX is supported"),
        };
        assert_eq!(result.verdict, "CRITICAL");
        assert_eq!(result.findings.len(), 6);
        assert_eq!(result.policy_name.as_deref(), Some("Standard Sales Policy"));
        assert_eq!(result.policy_version, Some(1));

        let history = store
            .history(&HistoryFilter::default())
            .expect("history reads");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].sha256.len(), 64);
        assert_eq!(history[0].policy_version, 1);
        assert_eq!(history[0].rule_ids.len(), 6);
        drop(store);
        std::fs::remove_file(database_path).expect("temporary database removes");
    }

    #[test]
    fn synthetic_quote_to_order_acceptance_cases_are_persisted() {
        let (database_path, store) = configured_store();
        let quote = import_quote(
            &store,
            &fixture("SprintE_Quote_Q-2026-184_Rev3.pdf"),
            QuoteLifecycle::AcceptedBaseline,
        )
        .expect("quote imports");
        assert_eq!(quote.status, "ACCEPTED_BASELINE");

        let exact = import_order(&store, &fixture("SprintE_PO-45590_exact.pdf"))
            .expect("exact order imports");
        assert_eq!(exact.status, "MATCHED");
        let exact_detail = store
            .order_detail(exact.entity_id.as_deref().expect("order id"))
            .expect("exact detail");
        assert!(exact_detail.differences.is_empty());

        let changed = import_order(&store, &fixture("SprintE_PO-45591_changed.pdf"))
            .expect("changed order imports");
        assert_eq!(changed.status, "REVIEW");
        let changed_detail = store
            .order_detail(changed.entity_id.as_deref().expect("order id"))
            .expect("changed detail");
        assert_eq!(changed_detail.summary.difference_count, 4);
        let codes = changed_detail
            .differences
            .iter()
            .map(|difference| difference.code.as_str())
            .collect::<Vec<_>>();
        for expected in ["CP-REC-007", "CP-REC-014", "CP-REC-015", "CP-REC-016"] {
            assert!(codes.contains(&expected), "missing {expected}");
        }

        let old = import_order(&store, &fixture("SprintE_PO-45592_old-revision.pdf"))
            .expect("old revision order imports");
        assert_eq!(old.status, "CRITICAL");
        assert_eq!(
            store
                .order_detail(old.entity_id.as_deref().expect("order id"))
                .expect("old detail")
                .differences[0]
                .code,
            "CP-REC-003"
        );

        let unknown = import_order(&store, &fixture("SprintE_PO-45593_unknown-quote.pdf"))
            .expect("unknown quote order imports");
        assert_eq!(unknown.status, "INCOMPLETE");
        let unknown_detail = store
            .order_detail(unknown.entity_id.as_deref().expect("order id"))
            .expect("unknown detail");
        assert_eq!(
            unknown_detail.incomplete_reasons,
            vec!["Referenced quote is not registered"]
        );

        let duplicate = import_order(&store, &fixture("SprintE_PO-45590_exact.pdf"))
            .expect("duplicate is handled");
        assert!(duplicate.duplicate);
        assert!(!duplicate.processed);
        assert_eq!(store.order_inbox().expect("inbox").len(), 4);
        assert_eq!(store.exceptions().expect("exceptions").len(), 3);

        drop(store);
        std::fs::remove_file(database_path).expect("temporary database removes");
    }

    #[test]
    fn quote_registration_backfills_exact_and_changed_pending_orders_idempotently() {
        let (database_path, store) = configured_store();
        let exact = import_order(&store, &fixture("SprintE_PO-45590_exact.pdf"))
            .expect("order arrives first");
        let changed = import_order(&store, &fixture("SprintE_PO-45591_changed.pdf"))
            .expect("changed order arrives first");
        assert_eq!(exact.status, "INCOMPLETE");
        assert_eq!(changed.status, "INCOMPLETE");
        let exact_id = exact.entity_id.expect("exact order id");
        let changed_id = changed.entity_id.expect("changed order id");
        assert_eq!(store.reconciliation_run_count(&exact_id).expect("runs"), 1);
        assert_eq!(
            store.reconciliation_run_count(&changed_id).expect("runs"),
            1
        );

        let quote = import_quote(
            &store,
            &fixture("SprintE_Quote_Q-2026-184_Rev3.pdf"),
            QuoteLifecycle::AcceptedBaseline,
        )
        .expect("quote arrives later");
        assert!(quote.message.contains("2 pending orders"));
        assert_eq!(
            store
                .order_detail(&exact_id)
                .expect("exact detail")
                .summary
                .status,
            "MATCHED"
        );
        assert_eq!(
            store
                .order_detail(&changed_id)
                .expect("changed detail")
                .summary
                .status,
            "REVIEW"
        );
        assert_eq!(store.reconciliation_run_count(&exact_id).expect("runs"), 2);
        assert_eq!(
            store.reconciliation_run_count(&changed_id).expect("runs"),
            2
        );
        assert_eq!(store.association_count(&exact_id).expect("association"), 1);
        assert_eq!(
            store.association_count(&changed_id).expect("association"),
            1
        );
        for order_id in [&exact_id, &changed_id] {
            let events = store.audit_events(order_id).expect("audit events");
            assert_eq!(
                events
                    .iter()
                    .filter(|event| {
                        event.event_type == "ORDER_AUTO_RECONCILED_AFTER_QUOTE_REGISTRATION"
                    })
                    .count(),
                1
            );
        }

        let duplicate = import_quote(
            &store,
            &fixture("SprintE_Quote_Q-2026-184_Rev3.pdf"),
            QuoteLifecycle::AcceptedBaseline,
        )
        .expect("duplicate quote is safe");
        assert!(duplicate.duplicate);
        assert_eq!(store.reconciliation_run_count(&exact_id).expect("runs"), 2);
        assert_eq!(
            store.reconciliation_run_count(&changed_id).expect("runs"),
            2
        );
        assert_eq!(store.association_count(&exact_id).expect("association"), 1);
        assert_eq!(store.list_quotes().expect("quotes").len(), 1);

        drop(store);
        std::fs::remove_file(database_path).expect("temporary database removes");
    }

    #[test]
    fn automatic_backfill_leaves_ambiguous_and_old_revision_orders_unresolved() {
        let aliases = FieldAliases::default();
        let mut revision_two =
            parse_commercial_file(fixture("SprintE_Quote_Q-2026-184_Rev3.pdf"), &aliases)
                .expect("quote parses");
        revision_two
            .quote_revision
            .as_mut()
            .expect("revision")
            .value = "2".to_owned();
        let revision_two_active = Quote::from_normalized(
            &revision_two,
            "quote-2-active",
            QuoteLifecycle::Sent,
            "quote-2.pdf",
            "2".repeat(64),
            1,
        )
        .expect("quote 2");
        let revision_two_old = Quote::from_normalized(
            &revision_two,
            "quote-2-old",
            QuoteLifecycle::Superseded,
            "quote-2.pdf",
            "3".repeat(64),
            1,
        )
        .expect("old quote 2");
        let revision_three_normalized =
            parse_commercial_file(fixture("SprintE_Quote_Q-2026-184_Rev3.pdf"), &aliases)
                .expect("quote parses");
        let revision_three = Quote::from_normalized(
            &revision_three_normalized,
            "quote-3",
            QuoteLifecycle::Sent,
            "quote-3.pdf",
            "4".repeat(64),
            1,
        )
        .expect("quote 3");
        let mut order_normalized =
            parse_commercial_file(fixture("SprintE_PO-45590_exact.pdf"), &aliases)
                .expect("order parses");
        order_normalized.quote_revision = None;
        let ambiguous_order = CustomerOrder::from_normalized(
            &order_normalized,
            "ambiguous-order",
            "order.pdf",
            "5".repeat(64),
            1,
        );
        assert!(
            deterministic_backfill_candidate(
                &ambiguous_order,
                &[revision_two_active, revision_three.clone()]
            )
            .is_none()
        );

        order_normalized.quote_revision = Some(commercial_preflight_core::ProposalField::new(
            "2".to_owned(),
            commercial_preflight_core::SourceEvidence::metadata(None),
        ));
        let old_order = CustomerOrder::from_normalized(
            &order_normalized,
            "old-order",
            "old-order.pdf",
            "6".repeat(64),
            1,
        );
        assert!(
            deterministic_backfill_candidate(&old_order, &[revision_two_old, revision_three])
                .is_none()
        );
    }

    #[test]
    fn unparseable_order_is_persisted_once_as_incomplete() {
        let (database_path, store) = configured_store();
        let path = fixture("Project_Orion_Quote_scanned.pdf");
        let first = import_order(&store, &path).expect("incomplete order records");
        assert!(first.processed);
        assert_eq!(first.status, "INCOMPLETE");
        let order_id = first.entity_id.expect("order identity");
        let detail = store.order_detail(&order_id).expect("incomplete detail");
        assert!(!detail.incomplete_reasons.is_empty());
        assert_eq!(store.exceptions().expect("exception query").len(), 1);
        let second = import_order(&store, &path).expect("duplicate is safe");
        assert!(second.duplicate);
        assert_eq!(store.order_inbox().expect("inbox").len(), 1);
        drop(store);
        std::fs::remove_file(database_path).expect("temporary database removes");
    }
}
