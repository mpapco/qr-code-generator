//! Constants and structural rules of the SBA Payment Link Standard 2.0.
//!
//! References in comments point at chapters of `Payment_Link_standard_2_0.pdf`.

use std::fmt;
use std::str::FromStr;

/// Second-level domain of the Payment Link Website (SS 3.2.1).
pub const PAYMENT_LINK_DOMAIN: &str = "payme.sk";
/// Path segment `{Version}`; the standard only carries major versions (SS 3.3.1).
pub const VERSION: &str = "2";
/// Path segment `{PaymentLinkSchemeID}`; identifies the payme.sk implementation (SS 3.3.3).
pub const SCHEME_ID: &str = "PME";
/// The only currency valid in version 2 of the standard (SS 3.4.3).
pub const CURRENCY_EUR: &str = "EUR";

/// Type path component: the payment context the link is created for (SS 3.3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentType {
    /// `/m/` mobile payment at the point of interaction (dynamic QR).
    M,
    /// `/e/` e-commerce.
    E,
    /// `/q/` static QR code at the point of interaction, and donations.
    Q,
    /// `/p/` person-to-person.
    P,
}

impl PaymentType {
    pub fn letter(self) -> &'static str {
        match self {
            PaymentType::M => "m",
            PaymentType::E => "e",
            PaymentType::Q => "q",
            PaymentType::P => "p",
        }
    }
}

impl fmt::Display for PaymentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.letter())
    }
}

impl FromStr for PaymentType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "m" => Ok(PaymentType::M),
            "e" => Ok(PaymentType::E),
            "q" => Ok(PaymentType::Q),
            "p" => Ok(PaymentType::P),
            other => Err(format!(
                "unknown payment type `{other}` (expected m, e, q or p)"
            )),
        }
    }
}

/// A query attribute of the payment link (SS 3.4, Table 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Field {
    Iban,
    Amount,
    Currency,
    DueDate,
    PaymentId,
    Message,
    CreditorName,
}

/// Attributes in the order they are emitted into the query string.
pub const FIELD_ORDER: [Field; 7] = [
    Field::Iban,
    Field::Amount,
    Field::Currency,
    Field::DueDate,
    Field::PaymentId,
    Field::Message,
    Field::CreditorName,
];

impl Field {
    /// Encoded attribute name used in the query string (SS 3.4, Table 1).
    pub fn key(self) -> &'static str {
        match self {
            Field::Iban => "IBAN",
            Field::Amount => "AM",
            Field::Currency => "CC",
            Field::DueDate => "DT",
            Field::PaymentId => "PI",
            Field::Message => "MSG",
            Field::CreditorName => "CN",
        }
    }

    /// Human label used by the TUI and by CLI diagnostics.
    pub fn label(self) -> &'static str {
        match self {
            Field::Iban => "IBAN",
            Field::Amount => "Amount",
            Field::Currency => "Currency",
            Field::DueDate => "Due date",
            Field::PaymentId => "Reference",
            Field::Message => "Message",
            Field::CreditorName => "Recipient name",
        }
    }

    /// Maximum length of the value *before* URL encoding (SS 3.4, Table 1).
    pub fn max_len(self) -> usize {
        match self {
            Field::Iban => 34,
            Field::Amount => 9,
            Field::Currency => 3,
            Field::DueDate => 8,
            Field::PaymentId => 35,
            Field::Message => 140,
            Field::CreditorName => 70,
        }
    }
}

/// Whether an attribute must, may, or must not appear for a given context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requirement {
    Mandatory,
    Optional,
    /// The attribute is omitted for this type; a value is not transmitted.
    Omitted,
}

/// A form preset, mirroring the tabs of the payme.sk generator.
///
/// Presets exist because two of them share the `/p/` type but differ in which
/// attributes the user is expected to fill in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    /// "Bezna platba" - ordinary person-to-person payment.
    P2p,
    /// "Platba faktur" - invoice payment: `/p/`, but the amount is expected.
    Invoice,
    /// "Platba v obchode" - payment at the point of interaction.
    Store,
    /// "Platba v e-shope" - e-commerce payment.
    Eshop,
    /// "Dary" - donations and other static QR codes.
    Donation,
}

pub const PRESETS: [Preset; 5] = [
    Preset::P2p,
    Preset::Invoice,
    Preset::Store,
    Preset::Eshop,
    Preset::Donation,
];

impl Preset {
    pub fn payment_type(self) -> PaymentType {
        match self {
            Preset::P2p | Preset::Invoice => PaymentType::P,
            Preset::Store => PaymentType::M,
            Preset::Eshop => PaymentType::E,
            Preset::Donation => PaymentType::Q,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Preset::P2p => "P2P payment",
            Preset::Invoice => "Invoice",
            Preset::Store => "In-store (POI)",
            Preset::Eshop => "E-commerce",
            Preset::Donation => "Donation / static QR",
        }
    }

    /// Requirement of `field` under this preset (SS 3.4 Table 2, refined by SS 5.2).
    pub fn requirement(self, field: Field) -> Requirement {
        use Field::*;
        use Requirement::*;

        match field {
            // Mandatory for every type.
            Iban | CreditorName => Mandatory,
            // Only `/p/` transmits a due date at all.
            DueDate => match self.payment_type() {
                PaymentType::P => Optional,
                _ => Omitted,
            },
            Message => Optional,
            Amount | Currency | PaymentId => match self {
                Preset::Store | Preset::Eshop => Mandatory,
                // The invoice preset expects an amount even though the type is `/p/`.
                Preset::Invoice if matches!(field, Amount | Currency) => Mandatory,
                _ => Optional,
            },
        }
    }
}

impl fmt::Display for Preset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

impl FromStr for Preset {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "p2p" | "normal" => Ok(Preset::P2p),
            "invoice" => Ok(Preset::Invoice),
            "store" => Ok(Preset::Store),
            "eshop" | "ecommerce" => Ok(Preset::Eshop),
            "donation" | "qr" => Ok(Preset::Donation),
            other => Err(format!(
                "unknown preset `{other}` (expected p2p, invoice, store, eshop or donation)"
            )),
        }
    }
}

/// Characters recommended for PI, MSG and CN (Annex A).
pub fn is_recommended_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            '/' | '-' | '?' | ':' | '(' | ')' | '.' | ',' | '\'' | '+' | ' '
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn due_date_is_only_carried_by_p2p_types() {
        for preset in PRESETS {
            let expected = match preset.payment_type() {
                PaymentType::P => Requirement::Optional,
                _ => Requirement::Omitted,
            };
            assert_eq!(preset.requirement(Field::DueDate), expected, "{preset:?}");
        }
    }

    #[test]
    fn requirement_matrix_matches_table_2() {
        use Field::*;
        use Requirement::*;

        // (preset, iban, amount, currency, due, pi, msg, cn)
        let rows = [
            (
                Preset::Store,
                Mandatory,
                Mandatory,
                Mandatory,
                Omitted,
                Mandatory,
                Optional,
                Mandatory,
            ),
            (
                Preset::Eshop,
                Mandatory,
                Mandatory,
                Mandatory,
                Omitted,
                Mandatory,
                Optional,
                Mandatory,
            ),
            (
                Preset::Donation,
                Mandatory,
                Optional,
                Optional,
                Omitted,
                Optional,
                Optional,
                Mandatory,
            ),
            (
                Preset::P2p,
                Mandatory,
                Optional,
                Optional,
                Optional,
                Optional,
                Optional,
                Mandatory,
            ),
            // Invoice is `/p/` with a mandatory amount; PI stays optional.
            (
                Preset::Invoice,
                Mandatory,
                Mandatory,
                Mandatory,
                Optional,
                Optional,
                Optional,
                Mandatory,
            ),
        ];

        for (preset, iban, am, cc, dt, pi, msg, cn) in rows {
            assert_eq!(preset.requirement(Iban), iban, "{preset:?} IBAN");
            assert_eq!(preset.requirement(Amount), am, "{preset:?} AM");
            assert_eq!(preset.requirement(Currency), cc, "{preset:?} CC");
            assert_eq!(preset.requirement(DueDate), dt, "{preset:?} DT");
            assert_eq!(preset.requirement(PaymentId), pi, "{preset:?} PI");
            assert_eq!(preset.requirement(Message), msg, "{preset:?} MSG");
            assert_eq!(preset.requirement(CreditorName), cn, "{preset:?} CN");
        }
    }

    #[test]
    fn annex_a_charset() {
        for c in "abzABZ09/-?:().,'+ ".chars() {
            assert!(is_recommended_char(c), "{c:?} should be recommended");
        }
        for c in "\u{e1}\u{161}#@_&%\"".chars() {
            assert!(!is_recommended_char(c), "{c:?} should not be recommended");
        }
    }
}
