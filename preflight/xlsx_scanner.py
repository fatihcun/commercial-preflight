from __future__ import annotations

import re
from datetime import date, datetime
from pathlib import Path
from typing import Iterable

from openpyxl import load_workbook
from openpyxl.cell.cell import Cell
from openpyxl.worksheet.worksheet import Worksheet

from .models import Finding, ScanResult, Severity, Verdict
from .policy import Policy


RULE_DISCOUNT = "CP-POL-001"
RULE_PAYMENT = "CP-POL-002"
RULE_HIDDEN_COMMERCIAL = "CP-HYG-001"
RULE_MATH = "CP-MATH-001"
RULE_PRIOR_CUSTOMER = "CP-META-001"
RULE_EXPIRED = "CP-DATE-001"


def _norm(value: object) -> str:
    if value is None:
        return ""
    return " ".join(str(value).strip().casefold().split())


def _iter_cells(ws: Worksheet) -> Iterable[Cell]:
    for row in ws.iter_rows():
        for cell in row:
            if cell.value is not None:
                yield cell


def _find_label_cell(ws: Worksheet, labels: set[str]) -> Cell | None:
    wanted = {_norm(x) for x in labels}
    for cell in _iter_cells(ws):
        if _norm(cell.value) in wanted:
            return cell
    return None


def _right_value(ws: Worksheet, cell: Cell, offset: int = 1) -> object:
    return ws.cell(row=cell.row, column=cell.column + offset).value


def _to_date(value: object) -> date | None:
    if isinstance(value, datetime):
        return value.date()
    if isinstance(value, date):
        return value
    if isinstance(value, str):
        value = value.strip()
        for fmt in ("%Y-%m-%d", "%d.%m.%Y", "%d/%m/%Y"):
            try:
                return datetime.strptime(value, fmt).date()
            except ValueError:
                pass
    return None


def _extract_days(value: object) -> int | None:
    if isinstance(value, (int, float)):
        return int(value)
    if isinstance(value, str):
        match = re.search(r"\b(\d{1,4})\b", value)
        if match:
            return int(match.group(1))
    return None


def _extract_prior_customer(metadata_text: str) -> str | None:
    patterns = (
        r"\bprior\s+(.+?)\s+proposal\b",
        r"\bprevious\s+customer\s*[:\-]\s*([^;|,]+)",
        r"\bprior\s+customer\s*[:\-]\s*([^;|,]+)",
    )
    for pattern in patterns:
        match = re.search(pattern, metadata_text, flags=re.IGNORECASE)
        if match:
            return " ".join(match.group(1).strip().split())
    return None


def _sheet_text(ws: Worksheet) -> str:
    values: list[str] = [ws.title]
    for cell in _iter_cells(ws):
        values.append(str(cell.value))
    return "\n".join(values).casefold()


def _find_column(headers: list[object], candidates: set[str]) -> int | None:
    candidate_norm = {_norm(x) for x in candidates}
    for index, header in enumerate(headers, start=1):
        if _norm(header) in candidate_norm:
            return index
    return None


def _find_line_item_table(ws: Worksheet) -> tuple[int, dict[str, int]] | None:
    for row_idx in range(1, ws.max_row + 1):
        headers = [ws.cell(row_idx, c).value for c in range(1, ws.max_column + 1)]
        description = _find_column(headers, {"description", "item description"})
        qty = _find_column(headers, {"qty", "quantity"})
        unit_price = _find_column(headers, {"unit price (€)", "unit price", "price"})
        discount = _find_column(headers, {"discount %", "discount", "disc %"})
        if description and qty and unit_price and discount:
            return row_idx, {
                "description": description,
                "qty": qty,
                "unit_price": unit_price,
                "discount": discount,
            }
    return None


def _line_items(ws: Worksheet) -> list[dict[str, object]]:
    table = _find_line_item_table(ws)
    if not table:
        return []
    header_row, columns = table
    items: list[dict[str, object]] = []
    for row_idx in range(header_row + 1, ws.max_row + 1):
        description = ws.cell(row_idx, columns["description"]).value
        qty = ws.cell(row_idx, columns["qty"]).value
        unit_price = ws.cell(row_idx, columns["unit_price"]).value
        discount = ws.cell(row_idx, columns["discount"]).value
        if description is None and qty is None and unit_price is None:
            if items:
                break
            continue
        if not isinstance(qty, (int, float)) or not isinstance(unit_price, (int, float)):
            if items:
                break
            continue
        discount_value = float(discount or 0)
        discount_fraction = discount_value if abs(discount_value) <= 1.0 else discount_value / 100.0
        items.append({
            "row": row_idx,
            "description": str(description or f"Row {row_idx}"),
            "qty": float(qty),
            "unit_price": float(unit_price),
            "discount_fraction": discount_fraction,
        })
    return items


def _find_value_below_or_right_of_label(ws: Worksheet, labels: set[str]) -> tuple[Cell, object] | None:
    label = _find_label_cell(ws, labels)
    if not label:
        return None
    right = _right_value(ws, label)
    if right is not None:
        return label, right
    below = ws.cell(label.row + 1, label.column).value
    return label, below


def _currency_from_workbook(ws: Worksheet, policy: Policy) -> str:
    found = _find_value_below_or_right_of_label(ws, {"currency"})
    if found and found[1]:
        return str(found[1]).strip().upper()
    return policy.default_currency


def scan_xlsx(path: str | Path, policy: Policy, as_of: date) -> ScanResult:
    path = Path(path)
    wb = load_workbook(path, data_only=False, read_only=False)
    findings: list[Finding] = []

    visible_sheets = [ws for ws in wb.worksheets if ws.sheet_state == "visible"]
    offer_ws = next((ws for ws in visible_sheets if _find_label_cell(ws, {"customer"})), None)
    if offer_ws is None:
        return ScanResult(
            file_name=path.name,
            verdict=Verdict.INCOMPLETE,
            findings=(),
            checks_run=6,
            as_of_date=as_of.isoformat(),
        )

    customer_cell = _find_label_cell(offer_ws, {"customer"})
    customer = str(_right_value(offer_ws, customer_cell) or "").strip() if customer_cell else ""
    currency = _currency_from_workbook(offer_ws, policy)

    for item in _line_items(offer_ws):
        discount_pct = item["discount_fraction"] * 100.0
        if discount_pct > policy.maximum_sales_discount_pct + 1e-9:
            findings.append(Finding(
                rule_id=RULE_DISCOUNT,
                severity=policy.severity(RULE_DISCOUNT),
                title="Discount exceeds company policy",
                evidence=(
                    f"{discount_pct:.1f}% discount on {item['description']}; "
                    f"policy maximum {policy.maximum_sales_discount_pct:.1f}%"
                ),
                sheet=offer_ws.title,
                cell=f"F{item['row']}",
            ))
            break

    payment_label = _find_label_cell(offer_ws, {"payment term", "payment terms"})
    if payment_label:
        payment_raw = _right_value(offer_ws, payment_label)
        payment_days = _extract_days(payment_raw)
        if payment_days is not None and payment_days > policy.maximum_payment_term_days:
            findings.append(Finding(
                rule_id=RULE_PAYMENT,
                severity=policy.severity(RULE_PAYMENT),
                title="Payment term exceeds company policy",
                evidence=f"{payment_days} days; policy maximum {policy.maximum_payment_term_days} days",
                sheet=offer_ws.title,
                cell=offer_ws.cell(payment_label.row, payment_label.column + 1).coordinate,
            ))

    for ws in wb.worksheets:
        if ws.sheet_state == "visible":
            continue
        text = _sheet_text(ws)
        matched_terms = [term for term in policy.forbidden_outgoing_terms if term in text]
        if matched_terms:
            findings.append(Finding(
                rule_id=RULE_HIDDEN_COMMERCIAL,
                severity=policy.severity(RULE_HIDDEN_COMMERCIAL),
                title="Hidden worksheet contains commercial-sensitive content",
                evidence=f"Hidden sheet: {ws.title}",
                sheet=ws.title,
            ))
            break

    items = _line_items(offer_ws)
    subtotal = sum(
        item["qty"] * item["unit_price"] * (1.0 - item["discount_fraction"])
        for item in items
    )
    vat_label = _find_label_cell(offer_ws, {"vat %", "tax %", "kdv %"})
    vat_fraction = 0.0
    if vat_label:
        vat_raw = _right_value(offer_ws, vat_label)
        if isinstance(vat_raw, (int, float)):
            vat_fraction = float(vat_raw)
            if abs(vat_fraction) > 1.0:
                vat_fraction /= 100.0
    expected_total = subtotal * (1.0 + vat_fraction)
    declared_label = _find_label_cell(offer_ws, {"declared grand total", "grand total", "total"})
    if declared_label:
        declared_raw = _right_value(offer_ws, declared_label)
        if isinstance(declared_raw, (int, float)):
            declared_total = float(declared_raw)
            delta = expected_total - declared_total
            if abs(delta) >= 0.01:
                direction = "below" if delta > 0 else "above"
                findings.append(Finding(
                    rule_id=RULE_MATH,
                    severity=policy.severity(RULE_MATH, Severity.CRITICAL),
                    title="Declared grand total does not match calculated total",
                    evidence=(
                        f"Declared total is {currency} {abs(delta):,.0f} {direction} calculated expected total"
                    ),
                    sheet=offer_ws.title,
                    cell=offer_ws.cell(declared_label.row, declared_label.column + 1).coordinate,
                    amount=abs(delta),
                    currency=currency,
                ))

    props = wb.properties
    metadata_text = " | ".join(
        str(v) for v in (
            props.title,
            props.subject,
            props.description,
            props.keywords,
            props.category,
        ) if v
    )
    prior_customer = _extract_prior_customer(metadata_text)
    if prior_customer and prior_customer.casefold() not in customer.casefold():
        findings.append(Finding(
            rule_id=RULE_PRIOR_CUSTOMER,
            severity=policy.severity(RULE_PRIOR_CUSTOMER),
            title="Prior-customer metadata detected",
            evidence=(
                f"Workbook metadata contains {prior_customer} while proposal customer is {customer}"
            ),
        ))

    valid_label = _find_label_cell(offer_ws, {"valid until", "validity date", "valid thru"})
    if valid_label:
        valid_raw = _right_value(offer_ws, valid_label)
        valid_until = _to_date(valid_raw)
        if valid_until and valid_until < as_of:
            findings.append(Finding(
                rule_id=RULE_EXPIRED,
                severity=policy.severity(RULE_EXPIRED),
                title="Quote validity has expired",
                evidence=f"Valid Until = {valid_until.isoformat()}; scan date = {as_of.isoformat()}",
                sheet=offer_ws.title,
                cell=offer_ws.cell(valid_label.row, valid_label.column + 1).coordinate,
            ))

    rule_order = {
        RULE_DISCOUNT: 10,
        RULE_PAYMENT: 20,
        RULE_HIDDEN_COMMERCIAL: 30,
        RULE_MATH: 40,
        RULE_PRIOR_CUSTOMER: 50,
        RULE_EXPIRED: 60,
    }
    findings.sort(key=lambda f: (rule_order.get(f.rule_id, 999), f.rule_id, f.evidence))

    if any(f.severity == Severity.CRITICAL for f in findings):
        verdict = Verdict.CRITICAL
    elif findings:
        verdict = Verdict.REVIEW
    else:
        verdict = Verdict.PASS

    return ScanResult(
        file_name=path.name,
        verdict=verdict,
        findings=tuple(findings),
        checks_run=6,
        as_of_date=as_of.isoformat(),
    )
