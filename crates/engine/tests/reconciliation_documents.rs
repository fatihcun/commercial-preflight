use std::path::{Path, PathBuf};

use commercial_preflight_core::FieldAliases;
use commercial_preflight_engine::parse_commercial_file;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn quote_revision_pdf_and_xlsx_normalize_to_the_same_commercial_identity() {
    for name in [
        "SprintE_Quote_Q-2026-184_Rev3.pdf",
        "SprintE_Quote_Q-2026-184_Rev3.xlsx",
    ] {
        let quote = parse_commercial_file(fixture(name), &FieldAliases::default())
            .expect("synthetic quote parses");
        assert_eq!(
            quote.quote_number.expect("quote number").value,
            "Q-2026-184"
        );
        assert_eq!(quote.quote_revision.expect("revision").value, "3");
        assert_eq!(quote.line_items.len(), 2);
        assert_eq!(
            quote.line_items[0].sku.as_ref().expect("sku").value,
            "FG-200G"
        );
        assert_eq!(quote.line_items[0].uom.as_ref().expect("uom").value, "EA");
        assert_eq!(quote.grand_total.expect("total").value, 21_000.0);
    }
}

#[test]
fn exact_and_changed_customer_order_pdfs_extract_deterministically() {
    let exact = parse_commercial_file(
        fixture("SprintE_PO-45590_exact.pdf"),
        &FieldAliases::default(),
    )
    .expect("exact order parses");
    assert_eq!(exact.order_number.expect("po number").value, "PO-45590");
    assert_eq!(
        exact.quote_number.expect("quote reference").value,
        "Q-2026-184"
    );
    assert_eq!(exact.quote_revision.expect("quote revision").value, "3");
    assert_eq!(exact.payment_term_days.expect("payment").value, 30);
    assert_eq!(exact.delivery_term.expect("delivery").value, "3 weeks");

    let changed = parse_commercial_file(
        fixture("SprintE_PO-45591_changed.pdf"),
        &FieldAliases::default(),
    )
    .expect("changed order parses");
    assert_eq!(
        changed.line_items[1]
            .quantity
            .as_ref()
            .expect("quantity")
            .value,
        1.0
    );
    assert_eq!(changed.payment_term_days.expect("payment").value, 60);
    assert_eq!(changed.delivery_term.expect("delivery").value, "2 weeks");
    assert_eq!(changed.grand_total.expect("total").value, 18_900.0);
}
