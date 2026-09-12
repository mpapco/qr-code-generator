//! Every payment link printed in the Payment Link Standard 2.0 is reproduced
//! here from the inputs a user would type.
//!
//! Where an example in the PDF lists the attributes in a different order, or
//! leaves a character unencoded that `URLSearchParams` would escape, the
//! decoded attributes are compared instead of the raw string: those links carry
//! the same payment, and the encoding we emit is the one the payme.sk generator
//! produces.

use payme_qr::link::{self, decode_value};
use payme_qr::payment::Payment;
use payme_qr::spec::Preset;

const SPEC_IBAN: &str = "SK6807200002891987426353";

/// Split a link into its `scheme://authority/path` and its decoded attributes.
fn parts(url: &str) -> (String, Vec<(String, String)>) {
    match url.split_once('?') {
        None => (url.to_string(), Vec::new()),
        Some((base, query)) => {
            let attributes = query
                .split('&')
                .map(|pair| {
                    let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
                    (key.to_string(), decode_value(value))
                })
                .collect();
            (base.to_string(), attributes)
        }
    }
}

/// Assert that our link carries exactly the payment of the link in the PDF.
#[track_caller]
fn assert_same_payment(built: &str, from_spec: &str) {
    let (built_base, mut built_attributes) = parts(built);
    let (spec_base, mut spec_attributes) = parts(from_spec);

    assert_eq!(built_base, spec_base, "path component");
    built_attributes.sort();
    spec_attributes.sort();
    assert_eq!(built_attributes, spec_attributes, "attributes of {built}");
}

fn build(payment: &Payment) -> String {
    link::build(&payment.resolve().expect("the example is a valid payment"))
}

#[test]
fn p2p_with_every_optional_attribute() {
    // SS 4.1.1, first example.
    let payment = Payment {
        iban: SPEC_IBAN.into(),
        creditor_name: "Alice Payee".into(),
        amount: "8.59".into(),
        due_date: "2028-04-30".into(),
        message: "Thank you for lunch".into(),
        ..Payment::default()
    };

    // This example is already in the order we emit, so compare it byte for byte.
    assert_eq!(
        build(&payment),
        "https://payme.sk/2/p/PME?IBAN=SK6807200002891987426353&AM=8.59&CC=EUR\
         &DT=20280430&MSG=Thank+you+for+lunch&CN=Alice+Payee"
    );
}

#[test]
fn p2p_with_an_amount_only() {
    // SS 4.1.1, second example; the PDF lists CN before AM.
    let payment = Payment {
        iban: SPEC_IBAN.into(),
        creditor_name: "Alice Payee".into(),
        amount: "8.59".into(),
        ..Payment::default()
    };
    assert_same_payment(
        &build(&payment),
        "https://payme.sk/2/p/PME?IBAN=SK6807200002891987426353&CN=Alice+Payee&AM=8.59&CC=EUR",
    );
}

#[test]
fn e_commerce_with_the_slovak_symbols() {
    // SS 4.1.1, e-commerce example.
    let payment = Payment {
        preset: Preset::Eshop,
        iban: SPEC_IBAN.into(),
        creditor_name: "The Best e-shops ltd".into(),
        amount: "200.30".into(),
        reference: "/VS2546874464/SS2019568456/KS1118".into(),
        message: "my e-shop, Kosice".into(),
        ..Payment::default()
    };
    assert_same_payment(
        &build(&payment),
        "https://payme.sk/2/e/PME?IBAN=SK6807200002891987426353&AM=200.30&CC=EUR\
         &PI=%2FVS2546874464%2FSS2019568456%2FKS1118&CN=The+Best+e-shops+ltd\
         &MSG=my+e-shop,+Kosice",
    );
}

#[test]
fn dynamic_qr_at_the_point_of_interaction() {
    // SS 4.2.1, first example.
    let payment = Payment {
        preset: Preset::Store,
        iban: SPEC_IBAN.into(),
        creditor_name: "The Best Cafes ltd".into(),
        amount: "200.30".into(),
        reference: "QR-ab29e346f1d841c8a95a63d857490818".into(),
        message: "Cafe on the corner Zilina".into(),
        ..Payment::default()
    };
    assert_same_payment(
        &build(&payment),
        "https://payme.sk/2/m/PME?IBAN=SK6807200002891987426353&AM=200.30&CC=EUR\
         &PI=QR-ab29e346f1d841c8a95a63d857490818&CN=The+Best+Cafes+ltd\
         &MSG=Cafe+on+the+corner+Zilina",
    );
}

#[test]
fn static_qr_at_the_point_of_interaction() {
    // SS 4.2.1, second example.
    let payment = Payment {
        preset: Preset::Donation,
        iban: SPEC_IBAN.into(),
        creditor_name: "The Best Cafes td".into(),
        message: "Cafe on the corner Trnava".into(),
        ..Payment::default()
    };
    // The PDF lists CN before MSG; we emit them in the order of Table 1.
    assert_same_payment(
        &build(&payment),
        "https://payme.sk/2/q/PME?IBAN=SK6807200002891987426353&CN=The+Best+Cafes+td\
         &MSG=Cafe+on+the+corner+Trnava",
    );
}

#[test]
fn donation() {
    // SS 4.2.1, third example.
    let payment = Payment {
        preset: Preset::Donation,
        iban: SPEC_IBAN.into(),
        creditor_name: "Hope charity".into(),
        ..Payment::default()
    };
    assert_eq!(
        build(&payment),
        "https://payme.sk/2/q/PME?IBAN=SK6807200002891987426353&CN=Hope+charity"
    );
}

#[test]
fn attribute_examples_from_chapter_3_4() {
    let payment = Payment {
        preset: Preset::P2p,
        iban: "SK68 0720 0002 8919 8742 6353".into(), // SS 3.4.1
        amount: "200,30".into(),                      // SS 3.4.2
        currency: "eur".into(),                       // SS 3.4.3
        due_date: "2025-04-30".into(),                // SS 3.4.4
        reference: "/VS2546874464/SS2019568456/KS1118".into(), // SS 3.4.5
        message: "Caf\u{e9} on the corner, Zilina".into(), // SS 3.4.6
        creditor_name: "The Best Cafes sro".into(),   // SS 3.4.7
        normalize_text: true,
    };

    let url = build(&payment);
    let (_, attributes) = parts(&url);
    let get = |key: &str| {
        attributes
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .unwrap_or_default()
    };

    assert_eq!(get("IBAN"), SPEC_IBAN);
    assert_eq!(get("AM"), "200.30");
    assert_eq!(get("CC"), "EUR");
    assert_eq!(get("DT"), "20250430");
    assert_eq!(get("PI"), "/VS2546874464/SS2019568456/KS1118");
    // SS 3.4.6 recommends replacing national characters with ASCII.
    assert_eq!(get("MSG"), "Cafe on the corner, Zilina");
    assert_eq!(get("CN"), "The Best Cafes sro");

    // The slash of the payment identification is always escaped (SS 3.4.5).
    assert!(url.contains("PI=%2FVS2546874464%2FSS2019568456%2FKS1118"));
}
