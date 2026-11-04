from __future__ import annotations

import argparse
import json
from datetime import date, datetime
from pathlib import Path

from .policy import load_policy
from .xlsx_scanner import scan_xlsx


def _parse_date(value: str) -> date:
    return datetime.strptime(value, "%Y-%m-%d").date()


def main() -> int:
    parser = argparse.ArgumentParser(prog="preflight", description="Commercial Preflight reference scanner")
    sub = parser.add_subparsers(dest="command", required=True)
    scan = sub.add_parser("scan", help="Scan an XLSX proposal")
    scan.add_argument("file", type=Path)
    scan.add_argument("--policy", required=True, type=Path)
    scan.add_argument("--as-of", type=_parse_date, default=date.today())
    scan.add_argument("--json", action="store_true", help="Emit machine-readable JSON")
    args = parser.parse_args()
    policy = load_policy(args.policy)
    result = scan_xlsx(args.file, policy, args.as_of)
    if args.json:
        print(json.dumps(result.to_dict(), ensure_ascii=False, indent=2))
    else:
        print(f"Commercial Preflight: {result.verdict.value}")
        print(f"File: {result.file_name}")
        print(f"Checks: {result.checks_run}")
        for finding in result.findings:
            print(f"[{finding.severity.value}] {finding.rule_id} - {finding.title}")
            print(f"  {finding.evidence}")
    return 2 if result.verdict.value == "INCOMPLETE" else 0


if __name__ == "__main__":
    raise SystemExit(main())
