import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { applyTranslations, getLocale, getLocaleTag, initializeLocale, setLocale, t, type Locale, type MessageKey } from "./i18n";
import { onboardingAction, submitOnboardingStep } from "./onboarding";
import "./styles.css";

type Verdict = "PASS" | "REVIEW" | "CRITICAL" | "INCOMPLETE";
type PageName = "order-inbox" | "exceptions" | "quotes" | "order-review" | "quick-check" | "checking" | "outcome" | "findings" | "history" | "policies" | "templates" | "settings";
type ControlKey = "arithmetic" | "discount" | "paymentTerms" | "validity" | "customerReference" | "documentHygiene";
type AliasKey = "customer" | "quoteNumber" | "paymentTerm" | "discount" | "subtotal" | "tax" | "grandTotal" | "validUntil";

interface CompanyProfile { companyId: string; companyName: string; defaultCurrency: string; createdAt: number; updatedAt: number; }
interface RuleControls { arithmetic: boolean; discount: boolean; paymentTerms: boolean; validity: boolean; customerReference: boolean; documentHygiene: boolean; }
interface PolicyDraft { name: string; maximumDiscountPercent: number; maximumPaymentTermDays: number; minimumMarginPercent: number | null; maximumQuoteValidityDays: number; controls: RuleControls; discountSeverity: "REVIEW" | "CRITICAL"; paymentTermSeverity: "REVIEW" | "CRITICAL"; }
interface PolicyProfile extends PolicyDraft { id: string; companyId: string; version: number; createdAt: number; updatedAt: number; active: boolean; archived: boolean; }
interface AliasSet { customer: string[]; quoteNumber: string[]; paymentTerm: string[]; discount: string[]; subtotal: string[]; tax: string[]; grandTotal: string[]; validUntil: string[]; }
interface TemplateDraft { name: string; aliases: AliasSet; }
interface TemplateProfile extends TemplateDraft { id: string; companyId: string; isDefault: boolean; enabled: boolean; createdAt: number; updatedAt: number; }
interface SetupState { onboardingRequired: boolean; company: CompanyProfile | null; policy: PolicyProfile | null; template: TemplateProfile | null; databasePath: string; schemaVersion: number; }
interface Finding { ruleId: string; severity: "INFO" | "REVIEW" | "CRITICAL"; title: string; message: string; evidence: string; actualValue?: string | null; expectedValue?: string | null; sheet?: string | null; cell?: string | null; page?: number | null; }
interface ScanResult { schemaVersion: string; fileName: string; verdict: Verdict; findings: Finding[]; checksRun: number; checksNotApplicable: number; checksIncomplete: number; asOfDate: string; incompleteReason?: string | null; enforcementMode: "SHADOW"; scanId?: string | null; policyName?: string | null; policyVersion?: number | null; templateName?: string | null; }
type ScanResponse = { kind: "COMPLETED"; result: ScanResult } | { kind: "UNSUPPORTED"; fileName: string; title: string; message: string };
interface HistoryEntry { scanId: string; timestamp: number; filename: string; sha256: string; documentFormat: string; companyId: string; policyProfileId: string; policyName: string; policyVersion: number; templateProfileId: string; templateName: string; verdict: Verdict; findingCount: number; ruleIds: string[]; checksCompleted: number; checksNotApplicable: number; checksIncomplete: number; durationMs: number; }
interface CountByKey { key: string; count: number; }
interface Aggregates { totalProposals: number; passCount: number; reviewCount: number; criticalCount: number; incompleteCount: number; findingsByRule: CountByKey[]; scansByFormat: CountByKey[]; weeklyCounts: CountByKey[]; monthlyCounts: CountByKey[]; }
interface AutostartStatus { enabled: boolean; available: boolean; isError: boolean; message?: string | null; }
interface ConfigurationDefaults { policy: PolicyDraft; aliases: AliasSet; templateName: string; }
interface ImportResult { processed: boolean; duplicate: boolean; entityId: string | null; status: string; message: string; }
interface OrderSummary { orderId: string; receivedAt: number; orderNumber: string | null; customer: string | null; quoteId: string | null; quoteNumber: string | null; quoteRevision: string | null; status: "MATCHED" | "REVIEW" | "CRITICAL" | "INCOMPLETE"; differenceCount: number; associationKind: string | null; reviewedAt: number | null; sourcePath: string; sourceAvailable: boolean; }
interface DifferenceRecord { code: string; title: string; severity: "REVIEW" | "CRITICAL"; subject: string; quoteValue: string | null; orderValue: string | null; }
interface OrderDetail { summary: OrderSummary; filename: string; format: string; reviewedStatus: string | null; differences: DifferenceRecord[]; incompleteReasons: string[]; }
interface QuoteRecord { quoteId: string; quoteNumber: string; revision: string | null; customer: string | null; documentDate: string | null; currency: string | null; grandTotal: number | null; lifecycle: "DRAFT" | "SENT" | "ACCEPTED_BASELINE" | "SUPERSEDED"; filename: string; sourcePath: string; sourceAvailable: boolean; format: string; createdAt: number; }
interface FolderIntakeSettings { quoteFolder: string | null; orderFolder: string | null; }

const controlLabelKeys: Record<ControlKey, MessageKey> = {
  arithmetic: "control.arithmetic",
  discount: "control.discount",
  paymentTerms: "control.paymentTerms",
  validity: "control.validity",
  customerReference: "control.customerReference",
  documentHygiene: "control.documentHygiene",
};
const controlKeys = Object.keys(controlLabelKeys) as ControlKey[];
const onboardingControlKeys: ControlKey[] = ["arithmetic", "customerReference", "documentHygiene", "validity"];
const aliasLabelKeys: Record<AliasKey, MessageKey> = {
  customer: "alias.customer",
  quoteNumber: "alias.quoteNumber",
  paymentTerm: "alias.paymentTerm",
  discount: "alias.discount",
  subtotal: "alias.subtotal",
  tax: "alias.tax",
  grandTotal: "alias.grandTotal",
  validUntil: "alias.validUntil",
};
const aliasKeys = Object.keys(aliasLabelKeys) as AliasKey[];
const currencies = ["TRY", "EUR", "USD", "GBP"] as const;
const pageTitleKeys: Record<PageName, MessageKey> = {
  "order-inbox": "nav.orderInbox", exceptions: "nav.exceptions", quotes: "nav.quotes", "order-review": "nav.orderInbox",
  "quick-check": "nav.proposalCheck", checking: "nav.proposalCheck", outcome: "nav.proposalCheck", findings: "proposal.findings",
  history: "nav.history", policies: "nav.policies", templates: "nav.templates", settings: "nav.settings",
};
let bootstrap: SetupState | null = null;
let configurationDefaults: ConfigurationDefaults | null = null;
let currentResult: ScanResult | null = null;
let policies: PolicyProfile[] = [];
let templates: TemplateProfile[] = [];
let editingPolicyId: string | null = null;
let editingTemplateId: string | null = null;
let onboardingStep = 1;
let scanning = false;
let quoteRecords: QuoteRecord[] = [];
let currentOrder: OrderDetail | null = null;
let currentPage: PageName = "order-inbox";

initializeLocale(window.localStorage, navigator.language);
applyTranslations(document);

function element<T extends HTMLElement>(id: string): T {
  const value = document.getElementById(id);
  if (!value) throw new Error(`Missing element: ${id}`);
  return value as T;
}

function escapeHtml(value: unknown): string {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

function inputValue(id: string): string { return element<HTMLInputElement>(id).value.trim(); }
function numberValue(id: string): number { return Number(element<HTMLInputElement>(id).value); }
function optionalNumber(id: string): number | null { const value = inputValue(id); return value === "" ? null : Number(value); }
function setText(id: string, value: unknown): void { element(id).textContent = String(value ?? ""); }
function formatDate(timestamp: number): string { return new Intl.DateTimeFormat(getLocaleTag(), { dateStyle: "medium", timeStyle: "short" }).format(new Date(timestamp)); }
function formatMoney(value: number | null, currency: string | null): string { return value === null ? "—" : `${currency ?? ""} ${new Intl.NumberFormat(getLocaleTag(), { maximumFractionDigits: 2 }).format(value)}`.trim(); }
function statusClass(verdict: string): string { return `status-${verdict.toLowerCase()}`; }
function statusLabel(status: string): string {
  const key = `status.${status}` as MessageKey;
  return ["PASS", "MATCHED", "REVIEW", "CRITICAL", "INCOMPLETE", "INFO", "DRAFT", "SENT", "ACCEPTED_BASELINE", "SUPERSEDED"].includes(status) ? t(key) : status.replaceAll("_", " ");
}

function renderCurrencyOptions(): void {
  for (const id of ["onboarding-currency", "settings-currency"]) {
    const select = element<HTMLSelectElement>(id);
    const selected = select.value || "EUR";
    select.innerHTML = currencies.map((currency) => `<option value="${currency}">${currency}</option>`).join("");
    select.value = currencies.includes(selected as typeof currencies[number]) ? selected : "EUR";
  }
}

function showPage(page: PageName): void {
  currentPage = page;
  document.querySelectorAll<HTMLElement>("main > .page").forEach((item) => { item.hidden = item.id !== `${page}-page`; });
  document.querySelectorAll<HTMLButtonElement>(".nav-item").forEach((button) => { button.classList.toggle("is-active", button.dataset.page === page || (page === "checking" || page === "outcome" || page === "findings") && button.dataset.page === "quick-check" || page === "order-review" && button.dataset.page === "order-inbox"); });
  setText("top-page-title", t(pageTitleKeys[page]));
  if (page === "history") void loadHistory();
  if (page === "order-inbox") void loadOrderInbox();
  if (page === "exceptions") void loadExceptions();
  if (page === "quotes") void loadQuotes();
  if (page === "policies") void loadPolicies();
  if (page === "templates") void loadTemplates();
  if (page === "settings") { void refreshAutostart(); void refreshFolderSettings(); }
}

function renderControls(containerId: string, prefix: string, controls: RuleControls): void {
  const container = element(containerId);
  container.innerHTML = controlKeys.map((key) => `<label class="control-option"><input id="${prefix}-${key}" type="checkbox" ${controls[key] ? "checked" : ""} /><span>${escapeHtml(t(controlLabelKeys[key]))}</span></label>`).join("");
}

function renderOnboardingControls(controls: RuleControls): void {
  element("onboarding-controls").innerHTML = [
    ...onboardingControlKeys.map((key) => `<label class="control-option"><input id="onboarding-${key}" type="checkbox" ${controls[key] ? "checked" : ""} /><span>${escapeHtml(t(controlLabelKeys[key]))}</span></label>`),
    ...(["discount", "paymentTerms"] as ControlKey[]).map((key) => `<input id="onboarding-${key}" type="checkbox" ${controls[key] ? "checked" : ""} hidden />`),
  ].join("");
}

function readControls(prefix: string): RuleControls {
  return Object.fromEntries(controlKeys.map((key) => [key, element<HTMLInputElement>(`${prefix}-${key}`).checked])) as unknown as RuleControls;
}

function renderBootstrap(): void {
  if (!bootstrap) return;
  const company = bootstrap.company;
  const policy = bootstrap.policy;
  const template = bootstrap.template;
  setText("top-company", company?.companyName ?? t("top.setupRequired"));
  setText("top-policy", policy ? `${policy.name} v${policy.version}` : t("top.noPolicy"));
  setText("quick-company", company?.companyName ?? "—");
  setText("quick-policy", policy ? `${policy.name} v${policy.version}` : "—");
  setText("quick-template", template?.name ?? "—");
  setText("database-location", bootstrap.databasePath);
  if (company) {
    element<HTMLInputElement>("settings-company-name").value = company.companyName;
    element<HTMLSelectElement>("settings-currency").value = company.defaultCurrency;
  }
}

async function refreshBootstrap(): Promise<void> {
  bootstrap = await invoke<SetupState>("get_bootstrap");
  renderBootstrap();
  if (bootstrap.onboardingRequired) {
    const dialog = element<HTMLDialogElement>("onboarding-dialog");
    if (!dialog.open) dialog.showModal();
  }
}

function showOnboardingStep(step: number): void {
  onboardingStep = step;
  document.querySelectorAll<HTMLElement>(".onboarding-step").forEach((section) => { section.hidden = Number(section.dataset.step) !== step; });
  document.querySelectorAll<HTMLElement>("[data-progress-step]").forEach((item) => {
    const itemStep = Number(item.dataset.progressStep);
    item.classList.toggle("is-active", itemStep === step);
    item.classList.toggle("is-complete", itemStep < step);
    item.setAttribute("aria-current", itemStep === step ? "step" : "false");
    const marker = item.querySelector("span");
    if (marker) marker.textContent = itemStep < step ? "✓" : String(itemStep);
  });
  element<HTMLButtonElement>("onboarding-back").hidden = step === 1;
  setText("onboarding-next", onboardingAction(step) === "finish" ? t("button.finishSetup") : t("button.next"));
  setText("onboarding-error", "");
}

function onboardingReview(): void {
  element("onboarding-review").innerHTML = [
    [t("onboarding.review.company"), inputValue("onboarding-company")],
    [t("onboarding.review.currency"), inputValue("onboarding-currency")],
    [t("onboarding.review.policy"), configurationDefaults?.policy.name ?? "—"],
  ].map(([term, description]) => `<div><dt>${escapeHtml(term)}</dt><dd>${escapeHtml(description)}</dd></div>`).join("");
}

function onboardingPolicy(): PolicyDraft {
  if (!configurationDefaults) throw new Error(t("validation.defaultsUnavailable"));
  return {
    name: configurationDefaults.policy.name,
    maximumDiscountPercent: numberValue("onboarding-discount"),
    maximumPaymentTermDays: numberValue("onboarding-payment"),
    minimumMarginPercent: optionalNumber("onboarding-margin"),
    maximumQuoteValidityDays: numberValue("onboarding-validity"),
    controls: readControls("onboarding"),
    discountSeverity: configurationDefaults.policy.discountSeverity,
    paymentTermSeverity: configurationDefaults.policy.paymentTermSeverity,
  };
}

async function submitOnboarding(event: SubmitEvent): Promise<void> {
  event.preventDefault();
  const form = element<HTMLFormElement>("onboarding-form");
  if (!form.checkValidity()) { form.reportValidity(); return; }
  if (onboardingAction(onboardingStep) === "next") {
    if (onboardingStep === 2) onboardingReview();
    const submission = await submitOnboardingStep(onboardingStep, async () => undefined);
    showOnboardingStep(submission.step);
    return;
  }
  const button = element<HTMLButtonElement>("onboarding-next");
  button.disabled = true;
  try {
    if (!configurationDefaults) throw new Error(t("validation.defaultsUnavailable"));
    const defaults = configurationDefaults;
    await submitOnboardingStep(onboardingStep, async () => {
      bootstrap = await invoke<SetupState>("complete_onboarding", { request: { company: { companyName: inputValue("onboarding-company"), defaultCurrency: inputValue("onboarding-currency") }, policy: onboardingPolicy(), templateName: defaults.templateName } });
    });
    element<HTMLDialogElement>("onboarding-dialog").close();
    renderBootstrap();
    showPage("order-inbox");
    await refreshLastCheck();
  } catch (error) {
    setText("onboarding-error", String(error));
  } finally { button.disabled = false; }
}

function verdictNote(result: ScanResult): string {
  if (result.verdict === "PASS") return t("proposal.passed");
  if (result.verdict === "INCOMPLETE") return result.incompleteReason ?? t("proposal.incomplete");
  return t(result.findings.length === 1 ? "proposal.requires.one" : "proposal.requires.other", { count: result.findings.length });
}

function renderOutcome(result: ScanResult): void {
  currentResult = result;
  setText("outcome-file", result.fileName);
  const verdict = element("outcome-verdict");
  verdict.textContent = statusLabel(result.verdict);
  verdict.className = statusClass(result.verdict);
  setText("outcome-finding-count", t(result.findings.length === 1 ? "proposal.finding.one" : "proposal.finding.other", { count: result.findings.length }));
  setText("outcome-checks", result.checksRun);
  setText("outcome-na", result.checksNotApplicable);
  setText("outcome-policy", result.policyName ? `${result.policyName} v${result.policyVersion ?? ""}` : "—");
  setText("outcome-template", result.templateName ?? "—");
  setText("outcome-note", verdictNote(result));
  element<HTMLButtonElement>("view-findings-button").hidden = result.findings.length === 0;
  showPage("outcome");
}

function findingLocation(finding: Finding): string {
  if (finding.page) return t("proposal.page", { page: finding.page });
  if (finding.sheet && finding.cell) return `${finding.sheet} · ${finding.cell}`;
  return finding.sheet ?? t("proposal.document");
}

function renderFindings(): void {
  if (!currentResult) return;
  setText("findings-file", currentResult.fileName);
  setText("findings-summary", t(currentResult.findings.length === 1 ? "proposal.requires.one" : "proposal.requires.other", { count: currentResult.findings.length }));
  element("findings-list").innerHTML = currentResult.findings.map((finding) => `<article class="finding-item"><div class="finding-number"></div><div><h2>${escapeHtml(finding.title)}</h2><p class="finding-message">${escapeHtml(finding.message)}</p><div class="finding-facts"><div><span>${escapeHtml(t("proposal.actual"))}</span><strong>${escapeHtml(finding.actualValue ?? "—")}</strong></div><div><span>${escapeHtml(t("proposal.expected"))}</span><strong>${escapeHtml(finding.expectedValue ?? "—")}</strong></div><div><span>${escapeHtml(t("proposal.location"))}</span><strong>${escapeHtml(findingLocation(finding))}</strong></div><div class="evidence-block"><span>${escapeHtml(t("proposal.evidence"))}</span><strong>${escapeHtml(finding.evidence)}</strong></div></div><details class="technical-details"><summary>${escapeHtml(t("proposal.technicalDetails"))}</summary><p>${escapeHtml(finding.ruleId)} · ${escapeHtml(t("proposal.checked", { date: currentResult?.asOfDate || "—" }))}</p></details></div><span class="finding-severity ${statusClass(finding.severity)}">${escapeHtml(statusLabel(finding.severity))}</span></article>`).join("");
  showPage("findings");
}

function fileNameFromPath(path: string): string { return path.split(/[\\/]/).filter(Boolean).at(-1) ?? t("proposal.proposal"); }

async function scanPath(path: string): Promise<void> {
  if (scanning) return;
  scanning = true;
  element("drop-zone").classList.remove("is-dragging");
  setText("checking-file", t("proposal.checkingFile", { file: fileNameFromPath(path) }));
  showPage("checking");
  try {
    const response = await invoke<ScanResponse>("scan_proposal", { request: { path, policyId: null, asOf: null } });
    if (response.kind === "UNSUPPORTED") {
      renderOutcome({ schemaVersion: "0.1", fileName: response.fileName, verdict: "INCOMPLETE", findings: [], checksRun: 0, checksNotApplicable: 0, checksIncomplete: 1, asOfDate: "", incompleteReason: response.message, enforcementMode: "SHADOW" });
    } else renderOutcome(response.result);
    await refreshLastCheck();
  } catch {
    renderOutcome({ schemaVersion: "0.1", fileName: fileNameFromPath(path), verdict: "INCOMPLETE", findings: [], checksRun: 0, checksNotApplicable: 0, checksIncomplete: 1, asOfDate: "", incompleteReason: t("proposal.failed"), enforcementMode: "SHADOW" });
  } finally { scanning = false; }
}

async function chooseFile(): Promise<void> {
  const selection = await open({ multiple: false, directory: false, title: t("dialog.selectProposal"), filters: [{ name: t("dialog.commercialProposals"), extensions: ["xlsx", "pdf"] }] });
  if (typeof selection === "string") await scanPath(selection);
}

function orderResult(order: OrderSummary): string {
  if (order.status === "MATCHED") return t("review.matched");
  if (order.differenceCount > 0) return t(order.differenceCount === 1 ? "review.difference.one" : "review.difference.other", { count: order.differenceCount });
  if (order.associationKind === "UNKNOWN_QUOTE") return t("review.quoteUnknown");
  if (order.associationKind === "UNKNOWN_REVISION") return t("review.revisionUnknown");
  if (order.associationKind === "AMBIGUOUS") return t("review.needsAssociation");
  return t("review.comparisonIncomplete");
}

function quoteReference(order: OrderSummary): string {
  if (!order.quoteNumber) return "—";
  return `${order.quoteNumber}${order.quoteRevision ? ` Rev ${order.quoteRevision}` : ""}`;
}

async function loadOrderInbox(): Promise<void> {
  const orders = await invoke<OrderSummary[]>("get_order_inbox").catch(() => []);
  element("order-inbox-body").innerHTML = orders.map((order) => `<tr class="operational-row" data-order-id="${escapeHtml(order.orderId)}"><td>${escapeHtml(formatDate(order.receivedAt))}</td><td><strong>${escapeHtml(order.orderNumber ?? t("review.poUnknown"))}</strong><span>${escapeHtml(order.customer ?? t("review.customerUnknown"))}</span></td><td>${escapeHtml(quoteReference(order))}</td><td>${escapeHtml(orderResult(order))}</td><td><span class="status-text ${statusClass(order.status)}">${escapeHtml(statusLabel(order.status))}</span>${order.reviewedAt ? `<small class="reviewed-label">${escapeHtml(t("review.reviewed"))}</small>` : ""}</td></tr>`).join("");
  element("order-inbox-empty").hidden = orders.length !== 0;
}

async function loadExceptions(): Promise<void> {
  const orders = await invoke<OrderSummary[]>("get_exceptions").catch(() => []);
  element("exceptions-body").innerHTML = orders.map((order) => `<tr class="operational-row" data-order-id="${escapeHtml(order.orderId)}"><td>${escapeHtml(formatDate(order.receivedAt))}</td><td>${escapeHtml(order.customer ?? t("review.customerUnknown"))}</td><td>${escapeHtml(order.orderNumber ?? "—")}</td><td>${escapeHtml(quoteReference(order))}</td><td>${escapeHtml(orderResult(order))}</td><td><span class="status-text ${statusClass(order.status)}">${escapeHtml(statusLabel(order.status))}</span></td></tr>`).join("");
  element("exceptions-empty").hidden = orders.length !== 0;
}

function renderQuotes(filterQuoteNumber?: string): void {
  const records = filterQuoteNumber ? quoteRecords.filter((quote) => quote.quoteNumber === filterQuoteNumber) : quoteRecords;
  element("quotes-body").innerHTML = records.map((quote) => {
    const nextAction = quote.lifecycle === "DRAFT" ? `<button class="text-action" data-quote-lifecycle="SENT" data-quote-id="${escapeHtml(quote.quoteId)}">${escapeHtml(t("button.markSent"))}</button>` : quote.lifecycle === "SENT" ? `<button class="text-action" data-quote-lifecycle="ACCEPTED_BASELINE" data-quote-id="${escapeHtml(quote.quoteId)}">${escapeHtml(t("button.markBaseline"))}</button>` : "";
    return `<tr><td><strong>${escapeHtml(quote.quoteNumber)}</strong></td><td>${escapeHtml(quote.revision ?? "—")}</td><td>${escapeHtml(quote.customer ?? "—")}</td><td>${escapeHtml(quote.documentDate ?? "—")}</td><td>${escapeHtml(quote.currency ?? "—")}</td><td>${escapeHtml(formatMoney(quote.grandTotal, quote.currency))}</td><td><span class="status-text status-${escapeHtml(quote.lifecycle.toLowerCase().replace("accepted_baseline", "matched"))}">${escapeHtml(statusLabel(quote.lifecycle))}</span></td><td>${quote.sourceAvailable ? escapeHtml(quote.filename) : escapeHtml(t("review.sourceUnavailable"))}</td><td class="row-actions">${nextAction}<button class="text-action" data-quote-revisions="${escapeHtml(quote.quoteNumber)}">${escapeHtml(t("button.viewRevisions"))}</button></td></tr>`;
  }).join("");
  element("quotes-empty").hidden = records.length !== 0;
}

async function loadQuotes(): Promise<void> {
  quoteRecords = await invoke<QuoteRecord[]>("get_quotes").catch(() => []);
  renderQuotes();
}

async function chooseOrder(): Promise<void> {
  const selection = await open({ multiple: false, directory: false, title: t("dialog.selectOrder"), filters: [{ name: t("dialog.customerOrders"), extensions: ["xlsx", "pdf"] }] });
  if (typeof selection !== "string") return;
  const result = await invoke<ImportResult>("import_customer_order", { path: selection }).catch((error) => ({ processed: false, duplicate: false, entityId: null, status: "INCOMPLETE", message: String(error) }));
  await Promise.all([loadOrderInbox(), loadExceptions()]);
  if (result.entityId) await openOrderReview(result.entityId);
}

async function chooseQuote(): Promise<void> {
  const selection = await open({ multiple: false, directory: false, title: t("dialog.selectQuote"), filters: [{ name: t("dialog.commercialQuotes"), extensions: ["xlsx", "pdf"] }] });
  if (typeof selection !== "string") return;
  const result = await invoke<ImportResult>("import_quote", { path: selection, lifecycle: "DRAFT" }).catch((error) => ({ processed: false, duplicate: false, entityId: null, status: "INCOMPLETE", message: String(error) }));
  setText("quote-import-message", result.message);
  await Promise.all([loadQuotes(), loadOrderInbox(), loadExceptions()]);
}

async function openOrderReview(orderId: string): Promise<void> {
  currentOrder = await invoke<OrderDetail>("get_order_detail", { orderId });
  if (quoteRecords.length === 0) quoteRecords = await invoke<QuoteRecord[]>("get_quotes").catch(() => []);
  const summary = currentOrder.summary;
  setText("review-order-number", summary.orderNumber ?? t("review.poUnknown"));
  setText("review-customer", summary.customer ?? t("review.customerUnknown"));
  setText("review-status", statusLabel(summary.status));
  element("review-status").className = statusClass(summary.status);
  setText("review-difference-count", summary.differenceCount === 0 ? t("review.noDifferences") : t(summary.differenceCount === 1 ? "review.difference.one" : "review.difference.other", { count: summary.differenceCount }));
  setText("review-summary-customer", summary.customer ?? "—");
  setText("review-summary-po", summary.orderNumber ?? "—");
  setText("review-summary-quote", quoteReference(summary));
  const hasCausalLineDifference = currentOrder.differences.some((difference) => ["CP-REC-005", "CP-REC-006", "CP-REC-007", "CP-REC-008", "CP-REC-009", "CP-REC-010"].includes(difference.code));
  const presentationDifferences = currentOrder.differences.filter((difference) => !hasCausalLineDifference || !["CP-REC-011", "CP-REC-012"].includes(difference.code));
  const rows = presentationDifferences.map((difference) => `<tr class="comparison-changed"><td><strong>${escapeHtml(difference.subject)}</strong><span>${escapeHtml(difference.code)} · ${escapeHtml(difference.title)}</span></td><td>${escapeHtml(difference.quoteValue ?? "—")}</td><td>${escapeHtml(difference.orderValue ?? "—")}</td><td><span class="status-text ${statusClass(difference.severity)}">${escapeHtml(t("review.changed"))}</span></td></tr>`);
  rows.push(...currentOrder.incompleteReasons.map((reason) => `<tr class="comparison-incomplete"><td><strong>${escapeHtml(t("review.safeComparison"))}</strong></td><td>—</td><td>${escapeHtml(reason)}</td><td><span class="status-text status-incomplete">${escapeHtml(t("review.requiresReview"))}</span></td></tr>`));
  element("comparison-body").innerHTML = rows.join("");
  element("comparison-empty").hidden = rows.length !== 0;
  const canAssociate = ["AMBIGUOUS", "MISSING_REFERENCE"].includes(summary.associationKind ?? "") && quoteRecords.length > 0;
  element("association-panel").hidden = !canAssociate;
  element<HTMLSelectElement>("association-quote").innerHTML = quoteRecords.filter((quote) => quote.lifecycle !== "SUPERSEDED").map((quote) => `<option value="${escapeHtml(quote.quoteId)}">${escapeHtml(quote.quoteNumber)} Rev ${escapeHtml(quote.revision ?? "—")} · ${escapeHtml(quote.customer ?? t("review.customerUnknown"))}</option>`).join("");
  element<HTMLButtonElement>("mark-reviewed-button").disabled = summary.reviewedAt !== null || summary.status === "MATCHED";
  element<HTMLButtonElement>("open-order-button").disabled = !summary.sourceAvailable;
  const matchedQuote = quoteRecords.find((quote) => quote.quoteId === summary.quoteId);
  element<HTMLButtonElement>("open-quote-button").disabled = !matchedQuote?.sourceAvailable;
  setText("review-message", summary.reviewedAt ? t("review.reviewedAt", { date: formatDate(summary.reviewedAt) }) : "");
  showPage("order-review");
}

async function refreshFolderSettings(): Promise<void> {
  const settings = await invoke<FolderIntakeSettings>("get_folder_intake_settings").catch(() => ({ quoteFolder: null, orderFolder: null }));
  element<HTMLInputElement>("quote-intake-folder").value = settings.quoteFolder ?? "";
  element<HTMLInputElement>("order-intake-folder").value = settings.orderFolder ?? "";
}

async function chooseIntakeFolder(inputId: string): Promise<void> {
  const selection = await open({ multiple: false, directory: true, title: t("dialog.selectFolder") });
  if (typeof selection === "string") element<HTMLInputElement>(inputId).value = selection;
}

async function saveIntakeFolders(): Promise<void> {
  const settings = { quoteFolder: inputValue("quote-intake-folder") || null, orderFolder: inputValue("order-intake-folder") || null };
  try {
    await invoke<FolderIntakeSettings>("set_folder_intake_settings", { settings });
    setText("intake-folder-message", t("settings.foldersSaved"));
  } catch (error) { setText("intake-folder-message", String(error)); }
}

async function pollFolderIntake(): Promise<void> {
  const results = await invoke<ImportResult[]>("poll_folder_intake").catch(() => []);
  if (results.some((result) => result.processed)) await Promise.all([loadOrderInbox(), loadExceptions(), loadQuotes()]);
}

async function refreshLastCheck(): Promise<void> {
  const history = await invoke<HistoryEntry[]>("get_recent_history").catch(() => []);
  const last = history[0];
  element("last-check-empty").hidden = Boolean(last);
  element("last-check-data").hidden = !last;
  const verdict = element("last-verdict");
  verdict.hidden = !last;
  if (!last) return;
  setText("last-file", last.filename);
  setText("last-meta", `${t(last.findingCount === 1 ? "proposal.finding.one" : "proposal.finding.other", { count: last.findingCount })} · ${formatDate(last.timestamp)}`);
  verdict.className = `status-text ${statusClass(last.verdict)}`;
  verdict.textContent = statusLabel(last.verdict);
}

function dateBoundary(value: string, end: boolean): number | null {
  if (!value) return null;
  const date = new Date(`${value}T00:00:00`);
  return date.getTime() + (end ? 86_399_999 : 0);
}

async function loadHistory(): Promise<void> {
  const filter = {
    verdict: inputValue("history-verdict") || null,
    documentFormat: inputValue("history-format") || null,
    fromTimestamp: dateBoundary(inputValue("history-from"), false),
    toTimestamp: dateBoundary(inputValue("history-to"), true),
  };
  const [history, aggregates] = await Promise.all([
    invoke<HistoryEntry[]>("get_history", { filter }).catch(() => []),
    invoke<Aggregates>("get_management_aggregates").catch(() => ({ totalProposals: 0, passCount: 0, reviewCount: 0, criticalCount: 0, incompleteCount: 0, findingsByRule: [], scansByFormat: [], weeklyCounts: [], monthlyCounts: [] })),
  ]);
  element("history-body").innerHTML = history.map((entry) => `<tr><td>${escapeHtml(formatDate(entry.timestamp))}</td><td class="filename-cell" title="${escapeHtml(entry.filename)}">${escapeHtml(entry.filename)}</td><td>${escapeHtml(entry.documentFormat)}</td><td><span class="status-text ${statusClass(entry.verdict)}">${escapeHtml(statusLabel(entry.verdict))}</span></td><td>${entry.findingCount}</td><td>${escapeHtml(entry.policyName)} v${entry.policyVersion}</td></tr>`).join("");
  element("history-empty").hidden = history.length !== 0;
  setText("aggregate-total", aggregates.totalProposals);
  setText("aggregate-pass", aggregates.passCount);
  setText("aggregate-review", aggregates.reviewCount);
  setText("aggregate-critical", aggregates.criticalCount);
  setText("aggregate-incomplete", aggregates.incompleteCount);
  const maximum = Math.max(1, ...aggregates.findingsByRule.map((item) => item.count));
  element("rule-aggregates").innerHTML = aggregates.findingsByRule.length ? aggregates.findingsByRule.map((item) => `<div class="aggregate-rule"><strong>${escapeHtml(item.key)}</strong><div class="aggregate-rule-bar"><span style="width:${Math.round(item.count / maximum * 100)}%"></span></div><span>${item.count}</span></div>`).join("") : `<p class="muted">${escapeHtml(t("empty.findings"))}</p>`;
}

function policyDraftFromForm(): PolicyDraft {
  return {
    name: inputValue("policy-name-input"),
    maximumDiscountPercent: numberValue("policy-discount-input"),
    maximumPaymentTermDays: numberValue("policy-payment-input"),
    minimumMarginPercent: optionalNumber("policy-margin-input"),
    maximumQuoteValidityDays: numberValue("policy-validity-input"),
    controls: readControls("policy"),
    discountSeverity: element<HTMLSelectElement>("policy-discount-severity").value as "REVIEW" | "CRITICAL",
    paymentTermSeverity: element<HTMLSelectElement>("policy-payment-severity").value as "REVIEW" | "CRITICAL",
  };
}

function openPolicyEditor(policy?: PolicyProfile): void {
  editingPolicyId = policy?.id ?? null;
  const source = policy ?? configurationDefaults?.policy;
  if (!source) return;
  setText("policy-dialog-title", policy ? t("policy.editNamed", { name: policy.name }) : t("policy.new"));
  element<HTMLInputElement>("policy-name-input").value = source.name;
  element<HTMLInputElement>("policy-discount-input").value = String(source.maximumDiscountPercent);
  element<HTMLInputElement>("policy-payment-input").value = String(source.maximumPaymentTermDays);
  element<HTMLInputElement>("policy-margin-input").value = source.minimumMarginPercent === null ? "" : String(source.minimumMarginPercent);
  element<HTMLInputElement>("policy-validity-input").value = String(source.maximumQuoteValidityDays);
  element<HTMLSelectElement>("policy-discount-severity").value = source.discountSeverity;
  element<HTMLSelectElement>("policy-payment-severity").value = source.paymentTermSeverity;
  renderControls("policy-controls", "policy", source.controls);
  setText("policy-error", "");
  element<HTMLDialogElement>("policy-dialog").showModal();
}

async function savePolicy(event: SubmitEvent): Promise<void> {
  event.preventDefault();
  if (!bootstrap?.company) return;
  try {
    if (editingPolicyId) await invoke("update_policy", { policyId: editingPolicyId, draft: policyDraftFromForm() });
    else await invoke("create_policy", { companyId: bootstrap.company.companyId, draft: policyDraftFromForm() });
    element<HTMLDialogElement>("policy-dialog").close();
    await refreshBootstrap();
    await loadPolicies();
  } catch (error) { setText("policy-error", String(error)); }
}

async function loadPolicies(): Promise<void> {
  if (!bootstrap?.company) return;
  policies = await invoke<PolicyProfile[]>("list_policies", { companyId: bootstrap.company.companyId }).catch(() => []);
  element("policy-list").innerHTML = policies.map((policy) => `<section class="profile-section"><div class="profile-header"><div class="profile-title"><h2>${escapeHtml(policy.name)}</h2><span>v${policy.version}</span>${policy.active ? `<span class="active-label">${escapeHtml(t("policy.active"))}</span>` : ""}${policy.archived ? `<span>${escapeHtml(t("policy.archived"))}</span>` : ""}</div><div class="profile-actions"><button class="text-action" data-policy-edit="${escapeHtml(policy.id)}">${escapeHtml(t("button.edit"))}</button>${!policy.active && !policy.archived ? `<button class="text-action" data-policy-use="${escapeHtml(policy.id)}">${escapeHtml(t("button.setActive"))}</button><button class="text-action" data-policy-archive="${escapeHtml(policy.id)}">${escapeHtml(t("button.archive"))}</button>` : ""}</div></div><div class="profile-rows"><div class="profile-row"><span>${escapeHtml(t("policy.maximumDiscount"))}</span><strong>${policy.maximumDiscountPercent}%</strong></div><div class="profile-row"><span>${escapeHtml(t("policy.maximumPayment"))}</span><strong>${escapeHtml(t("policy.days", { count: policy.maximumPaymentTermDays }))}</strong></div><div class="profile-row"><span>${escapeHtml(t("policy.minimumMargin"))}</span><strong>${policy.minimumMarginPercent === null ? escapeHtml(t("policy.notSet")) : `${policy.minimumMarginPercent}%`}</strong></div><div class="profile-row"><span>${escapeHtml(t("policy.quoteValidity"))}</span><strong>${escapeHtml(t("policy.days", { count: policy.maximumQuoteValidityDays }))}</strong></div><div class="profile-row"><span>${escapeHtml(t("policy.controls"))}</span><strong>${controlKeys.filter((key) => policy.controls[key]).map((key) => escapeHtml(t(controlLabelKeys[key]))).join(" · ")}</strong></div><div class="profile-row"><span>${escapeHtml(t("policy.violationBehavior"))}</span><strong>${escapeHtml(t("policy.violationSummary", { discount: statusLabel(policy.discountSeverity), payment: statusLabel(policy.paymentTermSeverity) }))}</strong></div></div></section>`).join("");
}

function renderAliasEditor(aliases: AliasSet): void {
  element("alias-editor").innerHTML = aliasKeys.map((key) => `<label><span>${escapeHtml(t(aliasLabelKeys[key]))}</span><textarea id="alias-${key}" required>${escapeHtml(aliases[key].join("\n"))}</textarea></label>`).join("");
}

function templateDraftFromForm(): TemplateDraft {
  return { name: inputValue("template-name-input"), aliases: Object.fromEntries(aliasKeys.map((key) => [key, element<HTMLTextAreaElement>(`alias-${key}`).value.split(/\r?\n/).map((value) => value.trim()).filter(Boolean)])) as unknown as AliasSet };
}

function openTemplateEditor(template?: TemplateProfile): void {
  editingTemplateId = template?.id ?? null;
  setText("template-dialog-title", template ? t("template.editNamed", { name: template.name }) : t("template.new"));
  element<HTMLInputElement>("template-name-input").value = template?.name ?? t("template.defaultName");
  const aliases = template?.aliases ?? configurationDefaults?.aliases;
  if (!aliases) return;
  renderAliasEditor(structuredClone(aliases));
  setText("template-error", "");
  element<HTMLDialogElement>("template-dialog").showModal();
}

async function saveTemplate(event: SubmitEvent): Promise<void> {
  event.preventDefault();
  if (!bootstrap?.company) return;
  try {
    if (editingTemplateId) await invoke("update_template", { templateId: editingTemplateId, draft: templateDraftFromForm() });
    else await invoke("create_template", { companyId: bootstrap.company.companyId, draft: templateDraftFromForm() });
    element<HTMLDialogElement>("template-dialog").close();
    await refreshBootstrap();
    await loadTemplates();
  } catch (error) { setText("template-error", String(error)); }
}

async function loadTemplates(): Promise<void> {
  if (!bootstrap?.company) return;
  templates = await invoke<TemplateProfile[]>("list_templates", { companyId: bootstrap.company.companyId }).catch(() => []);
  element("template-list").innerHTML = templates.map((template) => `<section class="profile-section"><div class="profile-header"><div class="profile-title"><h2>${escapeHtml(template.name)}</h2>${template.isDefault ? `<span class="active-label">${escapeHtml(t("template.default"))}</span>` : ""}${!template.enabled ? `<span>${escapeHtml(t("template.disabled"))}</span>` : ""}</div><div class="profile-actions"><button class="text-action" data-template-edit="${escapeHtml(template.id)}">${escapeHtml(t("button.edit"))}</button>${template.enabled && !template.isDefault ? `<button class="text-action" data-template-default="${escapeHtml(template.id)}">${escapeHtml(t("button.setDefault"))}</button><button class="text-action" data-template-disable="${escapeHtml(template.id)}">${escapeHtml(t("button.disable"))}</button>` : ""}</div></div><p class="recognized-fields"><strong>${escapeHtml(t("template.recognizedFields"))}</strong><br />${aliasKeys.map((key) => escapeHtml(t(aliasLabelKeys[key]))).join(" · ")}</p><div class="profile-row"><span>${escapeHtml(t("template.configuredAliases"))}</span><strong>${aliasKeys.reduce((total, key) => total + template.aliases[key].length, 0)}</strong></div></section>`).join("");
}

async function updateCompany(event: SubmitEvent): Promise<void> {
  event.preventDefault();
  if (!bootstrap?.company) return;
  try {
    bootstrap = await invoke<SetupState>("update_company", { companyId: bootstrap.company.companyId, draft: { companyName: inputValue("settings-company-name"), defaultCurrency: inputValue("settings-currency") } });
    renderBootstrap();
    setText("company-message", t("settings.companySaved"));
  } catch (error) { setText("company-message", String(error)); }
}

async function refreshAutostart(): Promise<void> {
  const toggle = element<HTMLInputElement>("autostart-toggle");
  const status = await invoke<AutostartStatus>("get_autostart_status").catch(() => ({ enabled: false, available: false, isError: true, message: t("settings.windowsUnavailable") }));
  toggle.checked = status.enabled; toggle.disabled = !status.available; setText("autostart-message", status.message ?? (status.enabled ? t("settings.enabled") : t("settings.disabled")));
}

async function setAutostart(): Promise<void> {
  const toggle = element<HTMLInputElement>("autostart-toggle");
  toggle.disabled = true;
  const status = await invoke<AutostartStatus>("set_autostart", { enabled: toggle.checked }).catch(() => ({ enabled: !toggle.checked, available: false, isError: true, message: t("settings.windowsChangeFailed") }));
  toggle.checked = status.enabled; toggle.disabled = !status.available; setText("autostart-message", status.message ?? (status.enabled ? t("settings.enabled") : t("settings.disabled")));
}

function syncLanguageSelectors(): void {
  document.querySelectorAll<HTMLSelectElement>(".language-select").forEach((select) => { select.value = getLocale(); });
}

async function changeLanguage(locale: Locale): Promise<void> {
  const onboardingControls = configurationDefaults && element("onboarding-controls").childElementCount > 0 ? readControls("onboarding") : configurationDefaults?.policy.controls;
  const policyDialog = element<HTMLDialogElement>("policy-dialog");
  const policyControls = policyDialog.open ? readControls("policy") : null;
  const templateDialog = element<HTMLDialogElement>("template-dialog");
  const templateAliases = templateDialog.open ? templateDraftFromForm().aliases : null;
  setLocale(locale, window.localStorage);
  applyTranslations(document);
  syncLanguageSelectors();
  renderCurrencyOptions();
  if (onboardingControls) renderOnboardingControls(onboardingControls);
  if (policyControls) renderControls("policy-controls", "policy", policyControls);
  if (templateAliases) renderAliasEditor(templateAliases);
  renderBootstrap();
  showOnboardingStep(onboardingStep);
  if (onboardingStep === 3) onboardingReview();
  if (currentPage === "findings" && currentResult) renderFindings();
  else if (currentPage === "outcome" && currentResult) renderOutcome(currentResult);
  else if (currentPage === "order-review" && currentOrder) await openOrderReview(currentOrder.summary.orderId);
  else showPage(currentPage);
  await refreshLastCheck();
}

document.querySelectorAll<HTMLButtonElement>(".nav-item").forEach((button) => button.addEventListener("click", () => showPage(button.dataset.page as PageName)));
document.querySelectorAll<HTMLButtonElement>(".settings-tabs button").forEach((button) => button.addEventListener("click", () => { document.querySelectorAll<HTMLButtonElement>(".settings-tabs button").forEach((item) => item.classList.toggle("is-active", item === button)); document.querySelectorAll<HTMLElement>(".settings-content > section").forEach((section) => { section.hidden = section.id !== `settings-${button.dataset.settings}`; }); }));
document.querySelectorAll<HTMLButtonElement>("[data-close]").forEach((button) => button.addEventListener("click", () => element<HTMLDialogElement>(button.dataset.close ?? "").close()));
document.querySelectorAll<HTMLSelectElement>(".language-select").forEach((select) => select.addEventListener("change", () => void changeLanguage(select.value as Locale)));
document.querySelectorAll<HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement>("[required]").forEach((field) => {
  field.addEventListener("invalid", () => {
    if (field.validity.valueMissing) field.setCustomValidity(t("validation.required"));
    else if (field.validity.rangeUnderflow || field.validity.rangeOverflow) field.setCustomValidity(t("validation.range"));
  });
  field.addEventListener("input", () => field.setCustomValidity(""));
});

element<HTMLFormElement>("onboarding-form").addEventListener("submit", (event) => void submitOnboarding(event as SubmitEvent));
element("onboarding-back").addEventListener("click", () => showOnboardingStep(Math.max(1, onboardingStep - 1)));
element<HTMLFormElement>("policy-form").addEventListener("submit", (event) => void savePolicy(event as SubmitEvent));
element<HTMLFormElement>("template-form").addEventListener("submit", (event) => void saveTemplate(event as SubmitEvent));
element<HTMLFormElement>("company-form").addEventListener("submit", (event) => void updateCompany(event as SubmitEvent));
element<HTMLFormElement>("history-filter-form").addEventListener("submit", (event) => { event.preventDefault(); void loadHistory(); });
element("add-order-button").addEventListener("click", () => void chooseOrder());
element("add-quote-button").addEventListener("click", () => void chooseQuote());
element("order-review-back").addEventListener("click", () => showPage("order-inbox"));
element("choose-quote-folder").addEventListener("click", () => void chooseIntakeFolder("quote-intake-folder"));
element("choose-order-folder").addEventListener("click", () => void chooseIntakeFolder("order-intake-folder"));
element("save-intake-folders").addEventListener("click", () => void saveIntakeFolders());
element("mark-reviewed-button").addEventListener("click", () => {
  if (!currentOrder) return;
  void invoke<OrderDetail>("mark_order_reviewed", { orderId: currentOrder.summary.orderId }).then((detail) => { currentOrder = detail; return openOrderReview(detail.summary.orderId); }).then(loadExceptions);
});
element("associate-quote-button").addEventListener("click", () => {
  if (!currentOrder) return;
  const quoteId = element<HTMLSelectElement>("association-quote").value;
  if (quoteId) void invoke<OrderDetail>("associate_quote", { orderId: currentOrder.summary.orderId, quoteId }).then((detail) => openOrderReview(detail.summary.orderId)).then(() => Promise.all([loadOrderInbox(), loadExceptions()])).catch((error) => setText("review-message", String(error)));
});
element("open-order-button").addEventListener("click", () => { if (currentOrder?.summary.sourceAvailable) void invoke("open_source", { path: currentOrder.summary.sourcePath }).catch((error) => setText("review-message", String(error))); });
element("open-quote-button").addEventListener("click", () => {
  const quote = quoteRecords.find((item) => item.quoteId === currentOrder?.summary.quoteId);
  if (quote?.sourceAvailable) void invoke("open_source", { path: quote.sourcePath }).catch((error) => setText("review-message", String(error)));
});
for (const id of ["order-inbox-body", "exceptions-body"]) element(id).addEventListener("click", (event) => {
  const row = (event.target as HTMLElement).closest<HTMLElement>("[data-order-id]");
  if (row?.dataset.orderId) void openOrderReview(row.dataset.orderId);
});
element("quotes-body").addEventListener("click", (event) => {
  const button = (event.target as HTMLElement).closest<HTMLButtonElement>("button");
  if (!button) return;
  if (button.dataset.quoteRevisions) { renderQuotes(button.dataset.quoteRevisions); setText("quote-import-message", t("review.showingRevisions", { quote: button.dataset.quoteRevisions })); }
  if (button.dataset.quoteId && button.dataset.quoteLifecycle) void invoke<QuoteRecord[]>("set_quote_lifecycle", { quoteId: button.dataset.quoteId, lifecycle: button.dataset.quoteLifecycle }).then(async (records) => { quoteRecords = records; renderQuotes(); await Promise.all([loadOrderInbox(), loadExceptions()]); });
});
element("choose-file-button").addEventListener("click", () => void chooseFile());
element("drop-zone").addEventListener("keydown", (event) => { if (event instanceof KeyboardEvent && (event.key === "Enter" || event.key === " ")) { event.preventDefault(); void chooseFile(); } });
element("view-findings-button").addEventListener("click", renderFindings);
element("check-another-button").addEventListener("click", () => showPage("quick-check"));
element("findings-check-another").addEventListener("click", () => showPage("quick-check"));
element("outcome-back").addEventListener("click", () => showPage("quick-check"));
element("findings-back").addEventListener("click", () => showPage("outcome"));
element("new-policy-button").addEventListener("click", () => openPolicyEditor());
element("new-template-button").addEventListener("click", () => openTemplateEditor());
element<HTMLInputElement>("autostart-toggle").addEventListener("change", () => void setAutostart());
element("clear-history-button").addEventListener("click", () => element<HTMLDialogElement>("clear-history-dialog").showModal());
element("confirm-clear-history").addEventListener("click", () => void invoke<number>("clear_history").then(async () => { await refreshLastCheck(); if (!element("history-page").hidden) await loadHistory(); }));

element("policy-list").addEventListener("click", (event) => {
  const button = (event.target as HTMLElement).closest<HTMLButtonElement>("button"); if (!button || !bootstrap?.company) return;
  const edit = button.dataset.policyEdit; const use = button.dataset.policyUse; const archive = button.dataset.policyArchive;
  if (edit) openPolicyEditor(policies.find((policy) => policy.id === edit));
  if (use) void invoke<SetupState>("set_active_policy", { companyId: bootstrap.company.companyId, policyId: use }).then(async (state) => { bootstrap = state; renderBootstrap(); await loadPolicies(); });
  if (archive) void invoke("archive_policy", { policyId: archive }).then(loadPolicies);
});

element("template-list").addEventListener("click", (event) => {
  const button = (event.target as HTMLElement).closest<HTMLButtonElement>("button"); if (!button || !bootstrap?.company) return;
  const edit = button.dataset.templateEdit; const makeDefault = button.dataset.templateDefault; const disable = button.dataset.templateDisable;
  if (edit) openTemplateEditor(templates.find((template) => template.id === edit));
  if (makeDefault) void invoke<SetupState>("set_default_template", { companyId: bootstrap.company.companyId, templateId: makeDefault }).then(async (state) => { bootstrap = state; renderBootstrap(); await loadTemplates(); });
  if (disable) void invoke("disable_template", { templateId: disable }).then(loadTemplates);
});

await getCurrentWebview().onDragDropEvent((event) => {
  const zone = element("drop-zone");
  if (event.payload.type === "over") zone.classList.add("is-dragging");
  else if (event.payload.type === "drop") { zone.classList.remove("is-dragging"); const path = event.payload.paths[0]; if (path && event.payload.paths.length === 1) void scanPath(path); }
  else zone.classList.remove("is-dragging");
});
await listen("quick-check", () => { showPage("quick-check"); void chooseFile(); });

configurationDefaults = await invoke<ConfigurationDefaults>("get_configuration_defaults");
renderCurrencyOptions();
syncLanguageSelectors();
element<HTMLInputElement>("onboarding-discount").value = String(configurationDefaults.policy.maximumDiscountPercent);
element<HTMLInputElement>("onboarding-payment").value = String(configurationDefaults.policy.maximumPaymentTermDays);
element<HTMLInputElement>("onboarding-margin").value = configurationDefaults.policy.minimumMarginPercent === null ? "" : String(configurationDefaults.policy.minimumMarginPercent);
element<HTMLInputElement>("onboarding-validity").value = String(configurationDefaults.policy.maximumQuoteValidityDays);
renderOnboardingControls(configurationDefaults.policy.controls);
showOnboardingStep(1);
showPage("order-inbox");
await refreshBootstrap();
await refreshLastCheck();
await Promise.all([loadOrderInbox(), loadQuotes()]);
window.setInterval(() => void pollFolderIntake(), 5000);
