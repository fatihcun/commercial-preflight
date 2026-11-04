from __future__ import annotations

from dataclasses import dataclass, asdict
from enum import Enum
from typing import Any


class Severity(str, Enum):
    CRITICAL = "CRITICAL"
    REVIEW = "REVIEW"
    INFO = "INFO"


class Verdict(str, Enum):
    PASS = "PASS"
    REVIEW = "REVIEW"
    CRITICAL = "CRITICAL"
    INCOMPLETE = "INCOMPLETE"


class EnforcementMode(str, Enum):
    SHADOW = "SHADOW"
    WARN = "WARN"
    SOFT_BLOCK = "SOFT_BLOCK"
    HARD_BLOCK = "HARD_BLOCK"


@dataclass(frozen=True)
class Finding:
    rule_id: str
    severity: Severity
    title: str
    evidence: str
    sheet: str | None = None
    cell: str | None = None
    amount: float | None = None
    currency: str | None = None

    def to_dict(self) -> dict[str, Any]:
        data = asdict(self)
        data["severity"] = self.severity.value
        return {k: v for k, v in data.items() if v is not None}


@dataclass(frozen=True)
class ScanResult:
    file_name: str
    verdict: Verdict
    findings: tuple[Finding, ...]
    checks_run: int
    as_of_date: str

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": "0.1",
            "file_name": self.file_name,
            "verdict": self.verdict.value,
            "checks_run": self.checks_run,
            "as_of_date": self.as_of_date,
            "findings": [f.to_dict() for f in self.findings],
        }
