//! The payment as the user enters it, and its validation against the standard.

use chrono::NaiveDate;

use crate::iban;
use crate::reference;
use crate::spec::{Field, PaymentType, Preset, Requirement, CURRENCY_EUR, FIELD_ORDER};
use crate::text;

/// One problem with one field. Validation collects all of them so that the TUI
/// can show every issue at once instead of one per keystroke.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    pub field: Field,
    pub message: String,
}

impl FieldError {
    fn new(field: Field, message: impl Into<String>) -> Self {
        Self {
            field,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for FieldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.field.label(), self.message)
    }
}

/// Raw user input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payment {
    pub preset: Preset,
    pub iban: String,
    pub creditor_name: String,
    pub amount: String,
    pub currency: String,
    /// `YYYY-MM-DD` or `YYYYMMDD`; only carried by `/p/` links.
    pub due_date: String,
    /// Payment identification (PI): free text or the canonical symbol encoding.
    pub reference: String,
    pub message: String,
    /// Fold free text down to the Annex A character set before transmitting.
    pub normalize_text: bool,
}

impl Default for Payment {
    fn default() -> Self {
        Self {
            preset: Preset::P2p,
            iban: String::new(),
            creditor_name: String::new(),
            amount: String::new(),
            currency: CURRENCY_EUR.to_string(),
            due_date: String::new(),
            reference: String::new(),
            message: String::new(),
            normalize_text: true,
        }
    }
}

/// A payment that satisfies the standard, with every attribute in its
/// transmitted form and in the order they appear in the query string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub payment_type: PaymentType,
    pub values: Vec<(Field, String)>,
}

impl Resolved {
    pub fn get(&self, field: Field) -> Option<&str> {
        self.values
            .iter()
            .find(|(f, _)| *f == field)
            .map(|(_, v)| v.as_str())
    }
}

impl Payment {
    /// Validate every field and produce the transmitted attribute values.
    pub fn resolve(&self) -> Result<Resolved, Vec<FieldError>> {
        let mut errors = Vec::new();
        let mut values: Vec<(Field, String)> = Vec::new();

        let iban = match iban::parse(&self.iban) {
            Ok(v) => Some(v),
            Err(msg) => {
                errors.push(FieldError::new(Field::Iban, msg));
                None
            }
        };

        let amount = self.resolve_amount(&mut errors);
        let currency = self.resolve_currency(amount.is_some(), &mut errors);
        let due_date = self.resolve_due_date(&mut errors);
        let payment_id = self.resolve_payment_id(&mut errors);
        let message = self.resolve_text(Field::Message, &self.message, &mut errors);
        let creditor_name =
            self.resolve_text(Field::CreditorName, &self.creditor_name, &mut errors);

        for field in FIELD_ORDER {
            let value = match field {
                Field::Iban => iban.clone(),
                Field::Amount => amount.clone(),
                Field::Currency => currency.clone(),
                Field::DueDate => due_date.clone(),
                Field::PaymentId => payment_id.clone(),
                Field::Message => message.clone(),
                Field::CreditorName => creditor_name.clone(),
            };
            if let Some(value) = value.filter(|v| !v.is_empty()) {
                values.push((field, value));
            }
        }

        if errors.is_empty() {
            Ok(Resolved {
                payment_type: self.preset.payment_type(),
                values,
            })
        } else {
            errors.sort_by_key(|e| e.field);
            Err(errors)
        }
    }

    fn requirement(&self, field: Field) -> Requirement {
        self.preset.requirement(field)
    }

    /// Report a mandatory field left empty. Returns true when it did.
    fn check_missing(&self, field: Field, value: &str, errors: &mut Vec<FieldError>) -> bool {
        if value.trim().is_empty() && self.requirement(field) == Requirement::Mandatory {
            errors.push(FieldError::new(
                field,
                format!("Required for /{}/ links.", self.preset.payment_type()),
            ));
            return true;
        }
        false
    }

    fn resolve_amount(&self, errors: &mut Vec<FieldError>) -> Option<String> {
        let raw = self.amount.trim();
        if self.check_missing(Field::Amount, raw, errors) || raw.is_empty() {
            return None;
        }

        let value = raw.replace(',', ".");
        // The standard gives the amount a maximum length of 9 (SS 3.4, Table 1);
        // in practice that is nine digits, so `9999999.99` is accepted.
        let (integer, decimals) = match value.split_once('.') {
            Some((i, d)) => (i, Some(d)),
            None => (value.as_str(), None),
        };

        let digits_ok = !integer.is_empty()
            && integer.chars().all(|c| c.is_ascii_digit())
            && match decimals {
                None => integer.len() <= 9,
                Some(d) => {
                    integer.len() <= 7
                        && (1..=2).contains(&d.len())
                        && d.chars().all(|c| c.is_ascii_digit())
                }
            };

        if !digits_ok {
            errors.push(FieldError::new(
                Field::Amount,
                "Enter an amount of up to 9 digits, or 7 digits with 1-2 decimal places.",
            ));
            return None;
        }
        Some(value)
    }

    fn resolve_currency(&self, has_amount: bool, errors: &mut Vec<FieldError>) -> Option<String> {
        let raw = self.currency.trim().to_ascii_uppercase();
        // The currency only travels alongside an amount.
        if !has_amount && self.requirement(Field::Currency) != Requirement::Mandatory {
            return None;
        }
        if raw.is_empty() {
            self.check_missing(Field::Currency, &raw, errors);
            return None;
        }
        if raw != CURRENCY_EUR {
            errors.push(FieldError::new(
                Field::Currency,
                "Version 2 of the standard only supports EUR.",
            ));
            return None;
        }
        Some(raw)
    }

    fn resolve_due_date(&self, errors: &mut Vec<FieldError>) -> Option<String> {
        // `/m/`, `/e/` and `/q/` omit the due date entirely (SS 3.4.4), so a value
        // left over from another preset is simply not transmitted.
        if self.requirement(Field::DueDate) == Requirement::Omitted {
            return None;
        }
        let raw = self.due_date.trim();
        if raw.is_empty() {
            return None;
        }
        let parsed = NaiveDate::parse_from_str(raw, "%Y-%m-%d")
            .or_else(|_| NaiveDate::parse_from_str(raw, "%Y%m%d"));
        match parsed {
            Ok(date) => Some(date.format("%Y%m%d").to_string()),
            Err(_) => {
                errors.push(FieldError::new(
                    Field::DueDate,
                    "Enter a date as YYYY-MM-DD or YYYYMMDD.",
                ));
                None
            }
        }
    }

    fn resolve_payment_id(&self, errors: &mut Vec<FieldError>) -> Option<String> {
        let value = self.resolve_text(Field::PaymentId, &self.reference, errors)?;
        if value.is_empty() {
            return None;
        }
        // SS 3.4.5: outside the canonical symbol encoding, a reference may not
        // start or end with a slash, nor contain a double slash.
        if !reference::is_canonical(&value) {
            if value.starts_with('/') || value.ends_with('/') {
                errors.push(FieldError::new(
                    Field::PaymentId,
                    "Must not start or end with `/`.",
                ));
                return None;
            }
            if value.contains("//") {
                errors.push(FieldError::new(Field::PaymentId, "Must not contain `//`."));
                return None;
            }
        }
        Some(value)
    }

    /// Shared handling of the free-text attributes: normalisation, the mandatory
    /// check, the length limit and the recommended character set.
    fn resolve_text(
        &self,
        field: Field,
        raw: &str,
        errors: &mut Vec<FieldError>,
    ) -> Option<String> {
        let trimmed = raw.trim();
        if self.check_missing(field, trimmed, errors) {
            return None;
        }
        if trimmed.is_empty() {
            return Some(String::new());
        }

        let value = if self.normalize_text {
            text::normalize(trimmed).value.trim().to_string()
        } else {
            trimmed.to_string()
        };

        if value.chars().count() > field.max_len() {
            errors.push(FieldError::new(
                field,
                format!("Maximum length is {} characters.", field.max_len()),
            ));
            return None;
        }
        if !self.normalize_text {
            let unsupported = text::unsupported_chars(&value);
            if !unsupported.is_empty() {
                let list: String = unsupported.iter().collect();
                errors.push(FieldError::new(
                    field,
                    format!("Unsupported characters: {list} (enable normalisation to strip them)."),
                ));
                return None;
            }
        }
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC_IBAN: &str = "SK6807200002891987426353";

    fn base() -> Payment {
        Payment {
            iban: SPEC_IBAN.into(),
            creditor_name: "Alice Payee".into(),
            ..Payment::default()
        }
    }

    #[test]
    fn a_minimal_p2p_payment_resolves() {
        let resolved = base().resolve().unwrap();
        assert_eq!(resolved.payment_type, PaymentType::P);
        assert_eq!(resolved.get(Field::Iban), Some(SPEC_IBAN));
        assert_eq!(resolved.get(Field::CreditorName), Some("Alice Payee"));
        // No amount, so no currency travels either.
        assert_eq!(resolved.get(Field::Currency), None);
    }

    #[test]
    fn mandatory_attributes_are_reported_per_type() {
        let payment = Payment {
            preset: Preset::Eshop,
            ..base()
        };
        let errors = payment.resolve().unwrap_err();
        let fields: Vec<Field> = errors.iter().map(|e| e.field).collect();
        assert_eq!(fields, vec![Field::Amount, Field::PaymentId]);
    }

    #[test]
    fn every_error_is_collected_at_once() {
        let payment = Payment {
            iban: "SK0000000000000000000000".into(),
            creditor_name: String::new(),
            amount: "12.345".into(),
            ..Payment::default()
        };
        let errors = payment.resolve().unwrap_err();
        assert_eq!(errors.len(), 3);
    }

    #[test]
    fn amount_accepts_comma_and_bounds() {
        let ok = |a: &str| {
            Payment {
                amount: a.into(),
                ..base()
            }
            .resolve()
        };
        assert_eq!(ok("8,59").unwrap().get(Field::Amount), Some("8.59"));
        assert_eq!(
            ok("9999999.99").unwrap().get(Field::Amount),
            Some("9999999.99")
        );
        assert_eq!(
            ok("999999999").unwrap().get(Field::Amount),
            Some("999999999")
        );
        assert!(ok("1234567890").is_err());
        assert!(ok("12345678.9").is_err());
        assert!(ok("1.234").is_err());
        assert!(ok("abc").is_err());
    }

    #[test]
    fn currency_is_emitted_only_with_an_amount() {
        let with = Payment {
            amount: "8.59".into(),
            ..base()
        }
        .resolve()
        .unwrap();
        assert_eq!(with.get(Field::Currency), Some("EUR"));

        let bad = Payment {
            amount: "8.59".into(),
            currency: "CZK".into(),
            ..base()
        };
        assert_eq!(bad.resolve().unwrap_err()[0].field, Field::Currency);
    }

    #[test]
    fn due_date_is_dropped_for_types_that_omit_it() {
        let p2p = Payment {
            due_date: "2028-04-30".into(),
            ..base()
        }
        .resolve()
        .unwrap();
        assert_eq!(p2p.get(Field::DueDate), Some("20280430"));

        let donation = Payment {
            preset: Preset::Donation,
            due_date: "2028-04-30".into(),
            ..base()
        }
        .resolve()
        .unwrap();
        assert_eq!(donation.get(Field::DueDate), None);
    }

    #[test]
    fn due_date_accepts_both_input_shapes_and_rejects_impossible_dates() {
        let compact = Payment {
            due_date: "20280430".into(),
            ..base()
        }
        .resolve()
        .unwrap();
        assert_eq!(compact.get(Field::DueDate), Some("20280430"));
        assert!(Payment {
            due_date: "2028-02-30".into(),
            ..base()
        }
        .resolve()
        .is_err());
    }

    #[test]
    fn reference_slash_rules() {
        let pi = |r: &str| {
            Payment {
                reference: r.into(),
                ..base()
            }
            .resolve()
        };
        assert!(pi("/VS2546874464/SS2019568456/KS1118").is_ok());
        assert!(pi("QR-ab29e346f1d841c8a95a63d857490818").is_ok());
        assert!(pi("/invoice").is_err());
        assert!(pi("invoice/").is_err());
        assert!(pi("in//voice").is_err());
    }

    #[test]
    fn free_text_is_normalised_and_length_checked() {
        let payment = Payment {
            creditor_name: "Ko\u{161}ick\u{e1} kaviare\u{148}".into(),
            message: "Kava & \u{10d}aj".into(),
            ..base()
        };
        let resolved = payment.resolve().unwrap();
        assert_eq!(resolved.get(Field::CreditorName), Some("Kosicka kaviaren"));
        assert_eq!(resolved.get(Field::Message), Some("Kava  caj"));

        let long = Payment {
            message: "x".repeat(141),
            ..base()
        };
        assert_eq!(long.resolve().unwrap_err()[0].field, Field::Message);
    }

    #[test]
    fn normalisation_can_be_turned_off_and_then_rejects_unsupported_characters() {
        let payment = Payment {
            creditor_name: "Ko\u{161}ick\u{e1} kaviare\u{148}".into(),
            normalize_text: false,
            ..base()
        };
        assert_eq!(payment.resolve().unwrap_err()[0].field, Field::CreditorName);
    }
}
