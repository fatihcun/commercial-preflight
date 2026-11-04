# PDF regression fixtures

These PDFs contain only synthetic test data and are generated deterministically.
Names beginning with `Example` are reserved fixture identities and do not refer
to customers or real organizations.

```powershell
cargo run -p commercial-preflight-pdf --example generate_fixtures -- tests/fixtures
```

Expected SHA-256 values after generation:

- `Project_Orion_Quote_v0.1.pdf`: `2DCD0B7CCD92FD74DA6640CC29E66F1FC1963BCAFE3989A334ABD91B36AB341F`
- `Project_Orion_Quote_clean.pdf`: `1146A76F54A926A6312BBB4910763DED945DDC11107DC7E576C8980ED6590D18`
- `Project_Orion_Quote_scanned.pdf`: `3AEFC60441B6E4B4C440AC01783630E5480835D2B33F366FD8DA3A1802D21E5B`
- `Project_Orion_Quote_empty.pdf`: `1C572AFEE6A9DCB0B4F64EB530187A88E50DAF75F1D26F0EF25F3C5076DCC838`

`Project_Orion_Quote_v0.1.pdf` is a two-page, selectable-text proposal. Its
expected rule contract at `2026-10-30` is:

- `CP-POL-001`: finding (18% discount)
- `CP-POL-002`: finding (90-day payment term)
- `CP-HYG-001`: `NOT_APPLICABLE` (XLSX hidden-sheet check)
- `CP-MATH-001`: finding (declared total is EUR 4,250 low)
- `CP-META-001`: finding (synthetic previous-customer metadata)
- `CP-DATE-001`: finding (expired validity)

`Project_Orion_Quote_clean.pdf` completes the same five applicable checks with
no findings. `Project_Orion_Quote_scanned.pdf` contains an image object and no
text operators, so it must return `INCOMPLETE`; OCR is intentionally out of
scope. `Project_Orion_Quote_empty.pdf` is a valid one-page PDF with no content
and also returns `INCOMPLETE`.

PDF safety defaults are defined as public constants in `crates/pdf/src/lib.rs`:
20 MiB input, 50,000 objects, 200 pages, 8 MiB decompression per load-time
stream, 2 MiB decompressed content per page, 4 MiB total extracted text, and
16 KiB per inspected metadata value. Limit failures are never treated as
successful truncation.
