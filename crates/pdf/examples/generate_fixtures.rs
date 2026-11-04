use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, dictionary};

fn main() -> Result<(), Box<dyn Error>> {
    let output = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("tests/fixtures"));
    fs::create_dir_all(&output)?;
    save_text_fixture(&output.join("Project_Orion_Quote_v0.1.pdf"), true)?;
    save_text_fixture(&output.join("Project_Orion_Quote_clean.pdf"), false)?;
    save_scanned_fixture(&output.join("Project_Orion_Quote_scanned.pdf"))?;
    save_empty_fixture(&output.join("Project_Orion_Quote_empty.pdf"))?;
    save_reconciliation_fixture(
        &output.join("SprintE_Quote_Q-2026-184_Rev3.pdf"),
        &[
            "COMMERCIAL QUOTE",
            "Customer: Example Electrical",
            "Quote No: Q-2026-184",
            "Quote Revision: 3",
            "Quote Date: 2026-10-20",
            "Currency: EUR",
            "Payment Term: 30 days",
            "Delivery: 3 weeks",
            "Item: SKU: FG-200G | Description: Firewall Gateway | Quantity: 2 | UOM: EA | Unit Price: 8400.00 | Discount: 0% | Line Total: 16800.00",
            "Item: SKU: SUP-3Y | Description: Support 3Y | Quantity: 2 | UOM: EA | Unit Price: 2100.00 | Discount: 0% | Line Total: 4200.00",
            "Subtotal: 21000.00",
            "VAT / Tax: 0.00",
            "Grand Total: 21000.00",
        ],
    )?;
    save_reconciliation_fixture(
        &output.join("SprintE_PO-45590_exact.pdf"),
        &order_lines("PO-45590", "Q-2026-184", "3", false),
    )?;
    save_reconciliation_fixture(
        &output.join("SprintE_PO-45591_changed.pdf"),
        &order_lines("PO-45591", "Q-2026-184", "3", true),
    )?;
    save_reconciliation_fixture(
        &output.join("SprintE_PO-45592_old-revision.pdf"),
        &order_lines("PO-45592", "Q-2026-184", "2", false),
    )?;
    save_reconciliation_fixture(
        &output.join("SprintE_PO-45593_unknown-quote.pdf"),
        &order_lines("PO-45593", "Q-2099-999", "1", false),
    )?;
    Ok(())
}

fn order_lines(
    po: &'static str,
    quote: &'static str,
    revision: &'static str,
    changed: bool,
) -> Vec<&'static str> {
    vec![
        "CUSTOMER PURCHASE ORDER",
        if po == "PO-45590" {
            "PO Number: PO-45590"
        } else if po == "PO-45591" {
            "PO Number: PO-45591"
        } else if po == "PO-45592" {
            "PO Number: PO-45592"
        } else {
            "PO Number: PO-45593"
        },
        "Customer: Example Electrical",
        if quote == "Q-2026-184" {
            "Quote Reference: Q-2026-184"
        } else {
            "Quote Reference: Q-2099-999"
        },
        if revision == "3" {
            "Quote Revision: 3"
        } else if revision == "2" {
            "Quote Revision: 2"
        } else {
            "Quote Revision: 1"
        },
        "Order Date: 2026-10-25",
        "Currency: EUR",
        if changed {
            "Payment Term: 60 days"
        } else {
            "Payment Term: 30 days"
        },
        if changed {
            "Delivery: 2 weeks"
        } else {
            "Delivery: 3 weeks"
        },
        "Item: SKU: FG-200G | Description: Firewall Gateway | Quantity: 2 | UOM: EA | Unit Price: 8400.00 | Discount: 0% | Line Total: 16800.00",
        if changed {
            "Item: SKU: SUP-3Y | Description: Support 3Y | Quantity: 1 | UOM: EA | Unit Price: 2100.00 | Discount: 0% | Line Total: 2100.00"
        } else {
            "Item: SKU: SUP-3Y | Description: Support 3Y | Quantity: 2 | UOM: EA | Unit Price: 2100.00 | Discount: 0% | Line Total: 4200.00"
        },
        if changed {
            "Subtotal: 18900.00"
        } else {
            "Subtotal: 21000.00"
        },
        "VAT / Tax: 0.00",
        if changed {
            "Grand Total: 18900.00"
        } else {
            "Grand Total: 21000.00"
        },
    ]
}

fn save_reconciliation_fixture(path: &Path, lines: &[&str]) -> Result<(), Box<dyn Error>> {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let font_id = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let resources_id =
        document.add_object(dictionary! { "Font" => dictionary! { "F1" => font_id } });
    let page_id = add_text_page(&mut document, pages_id, resources_id, lines)?;
    finish_pages(&mut document, pages_id, vec![page_id.into()]);
    let info_id = document.add_object(dictionary! {
        "Title" => Object::string_literal("Commercial Preflight Sprint E synthetic fixture"),
        "Author" => Object::string_literal("Commercial Preflight Test Data"),
    });
    finish_document(&mut document, pages_id, info_id);
    document.save(path)?;
    Ok(())
}

fn save_text_fixture(path: &Path, problematic: bool) -> Result<(), Box<dyn Error>> {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let font_id = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let resources_id = document.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
    });

    let page_one = if problematic {
        vec![
            "COMMERCIAL PROPOSAL",
            "Customer: Example Energy Ltd.",
            "Quote No: ORION-PDF-001",
            "Quote Date: 2026-09-30",
            "Valid Until: 2026-10-29",
            "Payment Term: Net 90 days",
            "Discount: 18%",
            "Currency: EUR",
        ]
    } else {
        vec![
            "COMMERCIAL PROPOSAL",
            "Customer: Example Energy Ltd.",
            "Quote No: ORION-PDF-CLEAN",
            "Quote Date: 2026-10-30",
            "Valid Until: 2026-12-31",
            "Payment Term: Net 30 days",
            "Discount: 5%",
            "Currency: EUR",
        ]
    };
    let page_two = if problematic {
        vec![
            "LINE ITEMS",
            "Item: Industrial Gateway Controller | Quantity: 10 | Unit Price: 15000.00 | Discount: 18% | Line Total: 123000.00",
            "Item: Engineering Services | Quantity: 1 | Unit Price: 25000.00 | Discount: 0% | Line Total: 25000.00",
            "Subtotal: 148000.00",
            "VAT / Tax: 20%",
            "Grand Total: 173350.00",
        ]
    } else {
        vec![
            "LINE ITEMS",
            "Item: Industrial Gateway Controller | Quantity: 2 | Unit Price: 10000.00 | Discount: 5% | Line Total: 19000.00",
            "Item: Engineering Services | Quantity: 1 | Unit Price: 5000.00 | Discount: 0% | Line Total: 5000.00",
            "Subtotal: 24000.00",
            "VAT / Tax: 20%",
            "Grand Total: 28800.00",
        ]
    };

    let first_page_id = add_text_page(&mut document, pages_id, resources_id, &page_one)?;
    let second_page_id = add_text_page(&mut document, pages_id, resources_id, &page_two)?;
    finish_pages(
        &mut document,
        pages_id,
        vec![first_page_id.into(), second_page_id.into()],
    );
    let info_id = document.add_object(dictionary! {
        "Title" => Object::string_literal("Project Orion Synthetic Proposal"),
        "Subject" => Object::string_literal(if problematic {
            "Previous customer: Example Legacy Machinery; synthetic regression fixture"
        } else {
            "Synthetic clean commercial proposal"
        }),
        "Author" => Object::string_literal("Commercial Preflight Test Data"),
        "Producer" => Object::string_literal("Commercial Preflight deterministic fixture generator"),
    });
    finish_document(&mut document, pages_id, info_id);
    document.save(path)?;
    Ok(())
}

fn add_text_page(
    document: &mut Document,
    pages_id: (u32, u16),
    resources_id: (u32, u16),
    lines: &[&str],
) -> Result<(u32, u16), Box<dyn Error>> {
    let mut operations = vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 10.into()]),
        Operation::new("TL", vec![18.into()]),
        Operation::new("Td", vec![45.into(), 740.into()]),
    ];
    for (index, line) in lines.iter().enumerate() {
        operations.push(Operation::new("Tj", vec![Object::string_literal(*line)]));
        if index + 1 != lines.len() {
            operations.push(Operation::new("T*", vec![]));
        }
    }
    operations.push(Operation::new("ET", vec![]));
    let content = Content { operations }.encode()?;
    let content_id = document.add_object(Stream::new(dictionary! {}, content));
    Ok(document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "Resources" => resources_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    }))
}

fn save_scanned_fixture(path: &Path) -> Result<(), Box<dyn Error>> {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let image_id = document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => 1,
            "Height" => 1,
            "ColorSpace" => "DeviceGray",
            "BitsPerComponent" => 8,
        },
        vec![210],
    ));
    let resources_id = document.add_object(dictionary! {
        "XObject" => dictionary! { "Im1" => image_id },
    });
    let content = Content {
        operations: vec![
            Operation::new("q", vec![]),
            Operation::new(
                "cm",
                vec![
                    500.into(),
                    0.into(),
                    0.into(),
                    700.into(),
                    56.into(),
                    46.into(),
                ],
            ),
            Operation::new("Do", vec![Object::Name(b"Im1".to_vec())]),
            Operation::new("Q", vec![]),
        ],
    }
    .encode()?;
    let content_id = document.add_object(Stream::new(dictionary! {}, content));
    let page_id = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "Resources" => resources_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    });
    finish_pages(&mut document, pages_id, vec![page_id.into()]);
    let info_id = document.add_object(dictionary! {
        "Title" => Object::string_literal("Synthetic image-only proposal"),
        "Author" => Object::string_literal("Commercial Preflight Test Data"),
    });
    finish_document(&mut document, pages_id, info_id);
    document.save(path)?;
    Ok(())
}

fn save_empty_fixture(path: &Path) -> Result<(), Box<dyn Error>> {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let content_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
    let page_id = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "Resources" => dictionary! {},
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    });
    finish_pages(&mut document, pages_id, vec![page_id.into()]);
    let info_id = document.add_object(dictionary! {
        "Title" => Object::string_literal("Synthetic empty proposal"),
        "Author" => Object::string_literal("Commercial Preflight Test Data"),
    });
    finish_document(&mut document, pages_id, info_id);
    document.save(path)?;
    Ok(())
}

fn finish_pages(document: &mut Document, pages_id: (u32, u16), kids: Vec<Object>) {
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids.clone(),
            "Count" => kids.len() as i64,
        }),
    );
}

fn finish_document(document: &mut Document, pages_id: (u32, u16), info_id: (u32, u16)) {
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    document.trailer.set("Info", info_id);
}
