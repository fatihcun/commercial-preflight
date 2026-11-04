# Quick Start for Windows

Türkçe: [QUICK_START.tr.md](QUICK_START.tr.md)

This guide covers the v0.1 desktop workflow. Commercial Preflight works with
local files and does not require a Commercial Preflight cloud account.

## 1. Install the Windows application

1. Open the project's GitHub **Releases** page.
2. Download the v0.1 Windows setup executable (`*-setup.exe`).
3. Confirm that it came from the expected repository, then run it under your
   organization's software policy.
4. Open **Commercial Preflight** from the Start menu.

If no installer is published yet, ask your administrator or developer to build
the Windows application from the instructions in the main README.

## 2. Complete first-run company setup

The first launch opens **Initial Setup**.

1. Enter your company name and default currency.
2. Review the initial commercial limits, including maximum discount, maximum
   payment term, minimum margin (optional), and quote validity.
3. Choose which controls should be enabled.
4. Review the summary and choose **Finish Setup**.

These settings are stored on this Windows account. They can be reviewed later
under **Policies**, **Templates**, and **Settings**.

## 3. Add and register a quote

1. Open **Quotes**.
2. Choose **Add Quote** and select a supported XLSX or selectable-text PDF.
3. Confirm that the quote number and revision shown by the application are the
   ones you expect.
4. Use **Mark Sent** for a revision that was sent to the customer, or **Mark
   Baseline** for the revision your process treats as the comparison baseline.

Draft quotes are registered but are not eligible for automatic order matching.
Keep each revision as a separate registered record.

## 4. Add a customer order

1. Open **Order Inbox**.
2. Choose **Add Order** and select the customer order file.
3. Commercial Preflight reads the order's quote number and revision and looks
   for an eligible registered quote.
4. If the exact association cannot be made safely, open the order and use
   **Select Quote** to associate it manually. The manual selection is recorded
   in local history.

## 5. Review `MATCHED` and `REVIEW` results

- `MATCHED` means the configured comparison found no commercial difference
  between the order and its associated quote.
- `REVIEW` means at least one difference needs a person to assess. Open the
  order to compare quote and order values by field or line.
- `CRITICAL` identifies a difference configured as critical.
- `INCOMPLETE` means the document, association, or comparison could not be
  completed safely. It is not a match.

Check the customer, quote revision, quantities, prices, discounts, totals, and
terms shown in the comparison. **Mark Reviewed** records that someone reviewed
the case; it does not change the comparison result into `MATCHED`.

## 6. Use Proposal Check

Proposal Check is separate from quote-to-order reconciliation.

1. Open **Tools → Proposal Check**.
2. Drop a supported proposal onto the window or choose **Select Proposal**.
3. Review the summary and choose **View Findings** for evidence and expected
   values.
4. Correct the original file in its normal application, then check it again.

Commercial Preflight reads the source file but does not modify it.

## 7. Configure watched folders

Watched folders can register incoming files without using a file picker.

1. Create two dedicated local folders: one for quotes and one for customer
   orders.
2. Open **Settings → Application**.
3. Under **Automated folder intake**, choose the **Quote intake folder** and
   **Order intake folder**.
4. Choose **Save Folders**.
5. Copy supported PDF or XLSX files into the appropriate folder while
   Commercial Preflight is running.

Folder watching is non-recursive. A file is processed only after it is stable
across two polls, so it may take several seconds to appear. Locked files are
deferred, duplicate file content is ignored, and source files are never
modified. Keep unrelated or sensitive files out of the watched folders.

## Getting help safely

Use public issues only with synthetic examples. Never upload a customer quote,
order, policy, database, or screenshot containing real commercial information.
Follow [SECURITY.md](../SECURITY.md) for sensitive security reports.
