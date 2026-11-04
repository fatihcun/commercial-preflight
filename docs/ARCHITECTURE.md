# Architecture

Commercial Preflight v0.1 is a local-first Windows desktop application with a
Rust domain engine, a Tauri backend, and a TypeScript user interface. The same
proposal engine is also exposed through a Rust CLI.

## Design goals

- Keep document processing on the user's Windows device.
- Parse supported input read-only and fail closed as `INCOMPLETE` when a safe,
  deterministic result is not possible.
- Keep format parsing separate from policy and reconciliation logic.
- Preserve exact quote revision identity; do not guess an association from a
  customer name, date, or total.
- Store enough structured local history for operational review without copying
  complete source documents into the database.

## Workspace components

| Path | Responsibility |
| --- | --- |
| `apps/desktop` | Tauri Windows shell, TypeScript UI, tray/startup behavior, file dialogs, and polling orchestration. |
| `apps/cli` | `preflight scan` command for policy-based proposal checks. |
| `crates/core` | Format-independent dates, evidence, findings, normalized proposal/commercial models, and JSON output. |
| `crates/xlsx` | Bounded XLSX package extraction and normalization through a fixed PowerShell/.NET helper. |
| `crates/pdf` | Bounded selectable-text PDF parsing and normalization with `lopdf`. |
| `crates/policy` | Strict company-policy parsing, defaults, and validation. |
| `crates/rules` | Deterministic proposal policy and hygiene checks shared by XLSX and PDF. |
| `crates/engine` | Format selection and the safe/strict proposal scan boundary. |
| `crates/intake` | Manual/watched-folder source identities, stability checks, hashes, and supported-file discovery. |
| `crates/reconciliation` | Exact quote association and deterministic quote/order difference calculation. |
| `crates/storage` | SQLite migrations, configuration, policy/template versions, scan history, document records, reconciliation runs, and audit events. |
| `preflight` | Frozen Python reference for the original XLSX behavior; not part of the production runtime. |

## Proposal Check flow

```text
file path
  -> engine format boundary
  -> bounded XLSX or PDF parser
  -> NormalizedProposal + SourceEvidence
  -> applicable policy/rule checks
  -> PASS / REVIEW / CRITICAL / INCOMPLETE
  -> local history and desktop result
```

Both parsers emit the same normalized model. Rules do not inspect XLSX cells or
raw PDF operators directly. Each check records whether it passed, found an
issue, was not applicable, or was incomplete. Any incomplete applicable check
prevents a clean result.

## Quote-to-order flow

```text
manual or watched-folder intake
  -> hash and parse
  -> normalized quote or customer order
  -> persist registered quote/order
  -> exact quote number + revision association
  -> deterministic field and line reconciliation
  -> MATCHED / REVIEW / CRITICAL / INCOMPLETE
  -> Order Inbox, Exceptions, and local audit history
```

Draft quotes are excluded from automatic association. With a revision present,
the quote number and revision must both match. Without a revision, association
is allowed only when exactly one eligible sent/baseline revision exists. A
manual association is stored as an audit event.

Line matching uses SKU when every relevant line has one. Otherwise it uses an
exact normalized description only when the key is unique. Duplicate or missing
deterministic keys make the comparison incomplete; there is no fuzzy matching.

## Desktop and watched folders

The TypeScript UI invokes Rust commands through Tauri. The backend owns the
SQLite store and parsing/reconciliation operations. The window can run from the
system tray, and Windows startup is controlled by the Tauri autostart plugin.

Watched-folder intake is non-recursive and currently polled by the UI every
five seconds. A candidate must have the same size and modification time in two
consecutive polls. Locked files are deferred. SHA-256 identities are checked in
memory and in SQLite so the same content is not processed repeatedly across
application restarts.

## Local persistence and privacy boundary

The per-user Tauri application-data directory contains
`commercial-preflight.sqlite3`. Schema migrations are transactional; the
current schema version is 2.

The database stores:

- company, versioned policy, template, and application settings;
- source path, filename, format, SHA-256 hash, timestamps, and parse status;
- proposal scan verdicts, rule identifiers, and aggregate counts;
- normalized quote/order identity, lifecycle, commercial fields, and line
  values needed for historical reconciliation;
- associations, reconciliation runs/differences, review timestamps, and
  operational audit events.

It does not store source files, complete worksheet bodies, complete extracted
PDF text, or source evidence excerpts. When a source file is still available,
it remains the authoritative document. The application contains no account,
telemetry, cloud document-processing, OCR, LLM, Outlook, ERP, or CRM
integration in v0.1.

## Parser safety boundaries

The XLSX boundary validates ZIP structure, entry and expanded-size limits,
sheet/cell limits, and relationships before normalization. It rejects macros
and unsafe package relationships, disables XML DTD processing, passes the path
through an environment variable, and never writes to the workbook. The helper
requires Windows PowerShell/.NET.

The PDF boundary requires a PDF signature, uses strict in-memory parsing, and
does not execute or follow actions, JavaScript, links, annotations, or embedded
files. Current limits include:

- 20 MiB input;
- 50,000 parsed objects;
- 200 pages;
- 8 MiB per load-time decompressed stream;
- 2 MiB decompressed page content;
- 4 MiB total extracted text; and
- 16 KiB per inspected metadata value.

Limit failures, malformed input, and files without a usable text layer return
`INCOMPLETE`. PDF support is limited to the documented deterministic labels and
line layout represented by the synthetic fixtures.

## Dependency boundaries

Rust versions are locked in `Cargo.lock`; desktop packages are locked in
`apps/desktop/package-lock.json`. The desktop's web assets are compiled into the
Tauri application. No runtime package manager or Python interpreter is needed
for the installed desktop application.

The project is licensed under Apache-2.0. Some transitive dependencies use
MPL-2.0 or other compatible licenses; distributors remain responsible for
preserving third-party license notices and satisfying their terms.
