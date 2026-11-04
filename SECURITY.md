# Security Policy

Commercial Preflight is early-stage software. Security reports that help make
local document processing safer are welcome.

## Reporting a vulnerability

Use GitHub's private vulnerability reporting flow for this repository:
**Security → Report a vulnerability**. Do not open a public issue when a report
contains sensitive details or when disclosure could put users at risk.

Include, when possible:

- the affected Commercial Preflight version;
- the Windows version and whether the desktop app or CLI is affected;
- a concise impact description and reproducible steps;
- a minimal synthetic test document; and
- any relevant error output after removing personal, customer, path, and
  commercial information.

If private GitHub reporting is unavailable, contact a maintainer through a
private channel shown on the repository owner profile. Do not publish exploit
details while a report is being assessed.

## Protect customer information

Do **not** submit customer proposals, purchase orders, company policies, local
databases, or screenshots containing real commercial data. Do not include
credentials, local usernames, absolute paths, or document contents in a public
issue. Create a synthetic reproduction instead. If a sensitive document was
already posted publicly, remove access where possible and notify the
maintainers privately.

## Local processing boundary

Commercial Preflight processes supported files locally on the Windows device.
The application does not upload document contents to a Commercial Preflight
cloud service. It stores local configuration, hashes and source references,
results, and structured commercial values required for quote/order history;
source files and complete extracted document bodies are not copied into the
application database.

This local-first design reduces data transfer but does not replace operating
system security. Users remain responsible for device access, filesystem
permissions, backups, and handling of the local database.

## Public issues

Public issues are appropriate for non-sensitive bugs and feature discussions.
Security issues should not be filed publicly when they include sensitive
information, customer data, or actionable vulnerability details.
