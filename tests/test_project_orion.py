from __future__ import annotations

import json
import shutil
import tempfile
import unittest
from datetime import date
from pathlib import Path

from openpyxl import load_workbook

from preflight.policy import load_policy
from preflight.xlsx_scanner import scan_xlsx

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests" / "fixtures" / "Project_Orion_Quote_v0.1.xlsx"
EXPECTED = ROOT / "tests" / "fixtures" / "project_orion_expected_findings.json"
POLICY = ROOT / "config" / "company_policy_orion.json"


class ProjectOrionTests(unittest.TestCase):
    def setUp(self) -> None:
        self.policy = load_policy(POLICY)

    def test_project_orion_exact_rule_contract(self) -> None:
        expected = json.loads(EXPECTED.read_text(encoding="utf-8"))
        result = scan_xlsx(FIXTURE, self.policy, date(2026, 10, 30))
        self.assertEqual(result.verdict.value, "CRITICAL")
        self.assertEqual(
            [f.rule_id for f in result.findings],
            [x["rule_id"] for x in expected["expected_findings"]],
        )
        self.assertEqual(len(result.findings), 6)
        evidence_by_rule = {f.rule_id: f.evidence for f in result.findings}
        for expected_finding in expected["expected_findings"]:
            self.assertEqual(
                evidence_by_rule[expected_finding["rule_id"]],
                expected_finding["evidence"],
            )

    def test_cleaned_quote_passes(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            clean = Path(td) / "clean.xlsx"
            shutil.copy2(FIXTURE, clean)
            wb = load_workbook(clean)
            ws = wb["Customer Offer"]
            ws["F15"] = 0.10
            ws["B9"] = "60 days"
            ws["B8"] = date(2026, 11, 29)
            subtotal = (
                10 * 12500 * (1 - 0.10)
                + 6 * 8750 * (1 - 0.05)
                + 12 * 1350
                + 5400
            )
            ws["H25"] = subtotal * 1.20
            del wb["Internal Cost"]
            wb.properties.subject = "Example Energy Commercial Proposal"
            wb.properties.description = "Synthetic clean regression fixture."
            wb.properties.keywords = "proposal, Example Energy"
            wb.save(clean)
            result = scan_xlsx(clean, self.policy, date(2026, 10, 30))
            self.assertEqual(result.verdict.value, "PASS")
            self.assertEqual(result.findings, ())


if __name__ == "__main__":
    unittest.main()
