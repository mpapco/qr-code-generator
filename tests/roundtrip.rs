//! End-to-end check: what we write out is what a scanner reads back.

use payme_qr::payment::Payment;
use payme_qr::spec::Preset;
use payme_qr::{generate, qr};

/// rqrr reports the raw format-information value; index 0 is level M.
const ECC_LEVEL_M: u16 = 0;

fn payment() -> Payment {
    Payment {
        preset: Preset::Eshop,
        iban: "SK6807200002891987426353".into(),
        creditor_name: "The Best e-shops ltd".into(),
        amount: "200.30".into(),
        reference: "/VS2546874464/SS2019568456/KS1118".into(),
        message: "my e-shop, Kosice".into(),
        ..Payment::default()
    }
}

fn decode(path: &std::path::Path) -> (u16, String) {
    let image = image::open(path).expect("the PNG we just wrote").to_luma8();
    let mut prepared = rqrr::PreparedImage::prepare(image);
    let grids = prepared.detect_grids();
    assert_eq!(grids.len(), 1, "exactly one QR symbol in the image");
    let (meta, content) = grids[0].decode().expect("the symbol decodes");
    (meta.ecc_level, content)
}

#[test]
fn a_saved_png_decodes_back_to_the_payment_link() {
    let dir = std::env::temp_dir().join("payme-qr-roundtrip");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("roundtrip.png");

    let (url, matrix) = generate(&payment()).expect("a valid payment");
    qr::write_png(&matrix, &path, 8).unwrap();

    let (ecc_level, decoded) = decode(&path);
    assert_eq!(decoded, url);
    // SS 5.3 requires error correction level M.
    assert_eq!(ecc_level, ECC_LEVEL_M);

    std::fs::remove_file(&path).ok();
}

#[test]
fn a_small_scale_png_still_decodes() {
    let dir = std::env::temp_dir().join("payme-qr-roundtrip");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("small.png");

    let (url, matrix) = generate(&payment()).expect("a valid payment");
    qr::write_png(&matrix, &path, 2).unwrap();

    assert_eq!(decode(&path).1, url);
    std::fs::remove_file(&path).ok();
}

#[test]
fn the_longest_permitted_payment_still_fits_into_a_qr_code() {
    // Every attribute at its maximum length (SS 3.4, Table 1).
    let payment = Payment {
        preset: Preset::Eshop,
        iban: "SK6807200002891987426353".into(),
        creditor_name: "N".repeat(70),
        amount: "9999999.99".into(),
        reference: "R".repeat(35),
        message: "M".repeat(140),
        ..Payment::default()
    };
    let (url, matrix) = generate(&payment).expect("a valid payment");
    assert!(url.len() > 280);
    assert!(matrix.size > 0);
}
