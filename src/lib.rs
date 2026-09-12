//! Offline generator of PayMe payment links and QR payment codes.
//!
//! The implementation follows the Slovak Banking Association's *Payment Link
//! Standard, version 2.0* (2026-01-01). Chapter references in the source point
//! at that document; where the standard leaves a detail open, the behaviour of
//! the generator on <https://www.payme.sk/vytvorte-payme> is reproduced so that
//! links from this tool are byte-identical to the ones the website produces.

pub mod cli;
pub mod contacts;
pub mod iban;
pub mod link;
pub mod payment;
pub mod qr;
pub mod reference;
pub mod spec;
pub mod text;
pub mod tui;

pub use contacts::Contacts;
pub use payment::{FieldError, Payment, Resolved};
pub use spec::{Field, PaymentType, Preset, Requirement};

/// Validate a payment, build its link and encode the QR symbol in one step.
pub fn generate(payment: &Payment) -> Result<(String, qr::Matrix), Vec<FieldError>> {
    let resolved = payment.resolve()?;
    let url = link::build(&resolved);
    match qr::encode(&url) {
        Ok(matrix) => Ok((url, matrix)),
        Err(err) => Err(vec![FieldError {
            field: Field::Message,
            message: err.to_string(),
        }]),
    }
}
