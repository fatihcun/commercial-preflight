# Commercial Preflight

Türkçe: [README.tr.md](README.tr.md)

**Local-first commercial document assurance for Windows.**

> [!IMPORTANT]
> Commercial Preflight v0.1.0 is early-stage open-source software. Validate it
> with your own synthetic test cases before operational use. It supports human
> review; it is not a substitute for commercial, accounting, or legal approval.

Commercial Preflight checks XLSX and selectable-text PDF proposals, applies
configurable company policies, and compares customer orders with registered
quote revisions. It identifies commercial differences, keeps local history,
and runs without cloud document processing.

## Feature summary

- Check proposals for arithmetic, discount, payment-term, validity, customer,
  metadata, and document-hygiene issues where the format provides the needed
  evidence.
- Register quote revisions and mark their lifecycle as draft, sent, accepted
  baseline, or superseded.
- Associate orders with an exact quote number and revision, then compare
  customers, lines, quantities, prices, discounts, totals, payment terms,
  delivery terms, and freight when available.
- Route deterministic outcomes to `MATCHED`, `REVIEW`, `CRITICAL`, or
  `INCOMPLETE`; no fuzzy or AI-based matching is used.
- Maintain company policies, proposal label templates, scan history, and order
  review history in a local SQLite database.
- Import files manually or from non-recursive watched folders while the desktop
  application is running.
- Use the Windows desktop application or the Rust command-line scanner.

## Privacy

Commercial Preflight processes supported documents on the Windows device. The
application has no account, cloud document service, or telemetry integration,
and source documents are opened read-only and are not copied into its database.

The local database does retain the configuration and operational data needed by
the product: document hashes, filenames and source paths, policy/template
versions, verdicts and findings, and structured quote/order values used for
historical reconciliation. It does not retain complete document bodies,
extracted PDF text, worksheet bodies, or source evidence excerpts. Protect the
Windows account and local database according to your organization's data policy.

Never attach customer documents to a public issue. See [SECURITY.md](SECURITY.md)
for private vulnerability reporting.

## Screenshots

Public screenshots will be added after the first release installer is
published.

> **Screenshot placeholder:** Order Inbox and reconciliation status

> **Screenshot placeholder:** Quote-to-order difference review

> **Screenshot placeholder:** Proposal Check findings

## Windows installation

### Release installer

When a prebuilt release is available:

1. Open this repository's **Releases** page.
2. Download the v0.1.0 Windows NSIS setup executable (`*-setup.exe`).
3. Confirm that the download came from the expected GitHub repository, then run
   the installer under your organization's software policy.
4. Launch **Commercial Preflight** from the Start menu.

Early releases may not be code-signed. If Windows or an organizational control
rejects an installer, do not bypass that control; build from source or ask your
administrator to review the release.

### Build the Windows application from source

Install:

- Rust 1.88 or later;
- a current Node.js 22 release (or later) with npm;
- Windows PowerShell 5.1 or later; and
- the [Tauri Windows prerequisites](https://v2.tauri.app/start/prerequisites/),
  including Microsoft C++ Build Tools and WebView2.

From a repository checkout:

```powershell
cd apps\desktop
npm ci
npm run tauri build
```

The configured NSIS bundle is written below
`target\release\bundle\nsis\` at the repository root. For a development run,
use `npm run tauri dev` from `apps\desktop`.

## Quick Start

1. Start the application and complete the one-time company, currency, and
   policy setup.
2. Open **Quotes**, choose **Add Quote**, and register the quote file. Mark a
   revision **Sent** or **Baseline** when it is eligible for order matching.
3. Open **Order Inbox**, choose **Add Order**, and select the customer order.
4. Open the result: `MATCHED` means no configured commercial difference was
   found; `REVIEW` means one or more differences need a person to assess.
5. Use **Tools → Proposal Check** to check a proposal directly against the
   active company policy.

See the non-technical [Quick Start guide](docs/QUICK_START.md) for first-run
setup, review guidance, and watched-folder configuration.

## Supported formats

| Format | Current v0.1 support |
| --- | --- |
| XLSX | Proposals, quotes, and orders with recognized labels and tabular line items. Files are inspected read-only through the bounded Windows PowerShell/.NET extractor. |
| PDF | Selectable-text proposals, quotes, and orders that follow the deterministic label/line layout understood by the parser. |

DOCX, legacy XLS, image-only PDFs, and arbitrary PDF table layouts are not
supported. Unsupported or unsafe input returns `INCOMPLETE`, never a clean
result.

## Current limitations

- The supported product target is Windows; XLSX extraction depends on Windows
  PowerShell/.NET.
- PDF support is intentionally narrow. OCR, encrypted PDFs, unusual font
  encodings, and heuristic table reconstruction are not implemented.
- Template recognition uses configured labels, not machine learning or fuzzy
  inference.
- Watched folders are non-recursive and are polled only while the desktop
  application is running.
- There is no cloud sync, central administration, authentication, Outlook/ERP
  integration, or automatic enforcement in v0.1.
- A `MATCHED` or `PASS` result only describes the configured checks that could
  be completed. Human approval remains necessary.
- Interfaces, storage schema, and supported document layouts may change during
  the v0.1 series.

## Command-line scanner

The CLI checks a proposal with an explicit JSON policy:

```powershell
cargo run --locked -- scan tests/fixtures/Project_Orion_Quote_v0.1.xlsx `
  --policy config/company_policy_orion.json `
  --as-of 2026-10-30
```

Add `--json` for machine-readable output. Exit code `0` means analysis
completed with `PASS`, `REVIEW`, or `CRITICAL`; exit code `2` means invalid
input/configuration or an `INCOMPLETE` scan.

All repository fixtures contain synthetic data only.

## Architecture overview

```text
Desktop UI / CLI
       |
manual or watched-folder intake
       |
bounded XLSX or PDF parsing
       |
normalized commercial document
       |
policy checks or quote/order reconciliation
       |
local SQLite history + desktop review
```

The Rust workspace separates format parsing, normalized domain models, policy
rules, intake, reconciliation, and storage. The Tauri desktop backend exposes
those capabilities to a TypeScript user interface. A Python implementation is
retained only as a reference for the original XLSX behavior; it is not used by
the production desktop or Rust CLI.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for component boundaries,
data flows, safety limits, and local persistence details.

## Development and build

Build the Rust workspace:

```powershell
cargo build --workspace --locked
```

Install and validate the desktop frontend:

```powershell
cd apps\desktop
npm ci
npm run typecheck
npm test
npm run build
```

Run the desktop in development mode with `npm run tauri dev`.

To exercise the frozen Python reference implementation, use Python 3.11 or
later in a virtual environment and install `requirements.txt`.

## Tests

Run the release checks from the repository root:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

Push-Location apps\desktop
npm ci
npm run typecheck
npm test
npm run build
Pop-Location

python -m unittest discover -s tests -p "test_*.py"
```

The regression suite covers synthetic XLSX and PDF proposal checks, hostile or
incomplete input, quote/order association and differences, local persistence,
watched-folder intake, and desktop localization/onboarding behavior.

## Contributing and security

Development workflow and fixture rules are in
[CONTRIBUTING.md](CONTRIBUTING.md). Report vulnerabilities using the private
process in [SECURITY.md](SECURITY.md), not a public issue containing sensitive
information.

## License

Commercial Preflight is licensed under the
[Apache License 2.0](LICENSE).
