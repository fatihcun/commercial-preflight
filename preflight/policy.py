from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path

from .models import Severity


@dataclass(frozen=True)
class Policy:
    company_name: str
    default_currency: str
    allowed_currencies: tuple[str, ...]
    minimum_gross_margin_pct: float
    maximum_sales_discount_pct: float
    maximum_payment_term_days: int
    maximum_quote_validity_days: int
    allowed_tax_rates_pct: tuple[float, ...]
    forbidden_outgoing_terms: tuple[str, ...]
    severity_by_rule: dict[str, Severity]

    def severity(self, rule_id: str, default: Severity = Severity.REVIEW) -> Severity:
        return self.severity_by_rule.get(rule_id, default)


def load_policy(path: str | Path) -> Policy:
    raw = json.loads(Path(path).read_text(encoding="utf-8"))
    commercial = raw["commercial_policy"]
    severity: dict[str, Severity] = {}
    for sev_name, rule_ids in raw.get("severity_policy", {}).items():
        sev = Severity(sev_name)
        for rule_id in rule_ids:
            severity[rule_id] = sev

    return Policy(
        company_name=raw["company"]["name"],
        default_currency=raw["company"]["default_currency"],
        allowed_currencies=tuple(raw["company"].get("allowed_currencies", [])),
        minimum_gross_margin_pct=float(commercial["minimum_gross_margin_pct"]),
        maximum_sales_discount_pct=float(commercial["maximum_sales_discount_pct"]),
        maximum_payment_term_days=int(commercial["maximum_payment_term_days"]),
        maximum_quote_validity_days=int(commercial["maximum_quote_validity_days"]),
        allowed_tax_rates_pct=tuple(float(x) for x in commercial.get("allowed_tax_rates_pct", [])),
        forbidden_outgoing_terms=tuple(str(x).casefold() for x in raw.get("forbidden_outgoing_terms", [])),
        severity_by_rule=severity,
    )
