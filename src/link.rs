//! Building the Payment Link URL (SS 3.2 - SS 3.4).

use crate::payment::Resolved;
use crate::spec::{PAYMENT_LINK_DOMAIN, SCHEME_ID, VERSION};

/// Percent-encode one attribute value.
///
/// This is the `application/x-www-form-urlencoded` serialisation that the
/// payme.sk generator produces via `URLSearchParams`: everything outside
/// `A-Z a-z 0-9 * - . _` is percent-encoded and a space becomes `+`. The
/// standard explicitly allows both `+` and `%20` for spaces and recommends `+`
/// for readability (SS 3.4.6).
pub fn encode_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Decode a form-urlencoded value. Used by the tests and by `parse`.
pub fn decode_value(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or_default();
                match u8::from_str_radix(hex, 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The scheme, authority and path components: `https://payme.sk/2/{type}/PME`.
pub fn base_url(resolved: &Resolved) -> String {
    format!(
        "https://{PAYMENT_LINK_DOMAIN}/{VERSION}/{}/{SCHEME_ID}",
        resolved.payment_type
    )
}

/// The complete Payment Link.
pub fn build(resolved: &Resolved) -> String {
    let query = resolved
        .values
        .iter()
        .map(|(field, value)| format!("{}={}", field.key(), encode_value(value)))
        .collect::<Vec<_>>()
        .join("&");

    let base = base_url(resolved);
    if query.is_empty() {
        base
    } else {
        format!("{base}?{query}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payment::Payment;
    use crate::spec::Preset;

    #[test]
    fn encodes_like_url_search_params() {
        assert_eq!(encode_value("Alice Payee"), "Alice+Payee");
        assert_eq!(
            encode_value("/VS2546874464/SS2019568456/KS1118"),
            "%2FVS2546874464%2FSS2019568456%2FKS1118"
        );
        assert_eq!(encode_value("my e-shop, Kosice"), "my+e-shop%2C+Kosice");
        assert_eq!(encode_value("8.59"), "8.59");
        assert_eq!(encode_value("O'Brien (Ltd.)"), "O%27Brien+%28Ltd.%29");
    }

    #[test]
    fn decoding_round_trips() {
        for value in ["Alice Payee", "my e-shop, Kosice", "/VS1/SS/KS", "a+b%c"] {
            assert_eq!(decode_value(&encode_value(value)), value);
        }
    }

    #[test]
    fn builds_the_person_to_person_example() {
        // SS 4.1.1, first example.
        let payment = Payment {
            iban: "SK6807200002891987426353".into(),
            creditor_name: "Alice Payee".into(),
            amount: "8.59".into(),
            due_date: "2028-04-30".into(),
            message: "Thank you for lunch".into(),
            ..Payment::default()
        };
        assert_eq!(
            build(&payment.resolve().unwrap()),
            "https://payme.sk/2/p/PME?IBAN=SK6807200002891987426353&AM=8.59&CC=EUR\
             &DT=20280430&MSG=Thank+you+for+lunch&CN=Alice+Payee"
        );
    }

    #[test]
    fn builds_the_static_donation_example() {
        // SS 4.2.1, last example.
        let payment = Payment {
            preset: Preset::Donation,
            iban: "SK6807200002891987426353".into(),
            creditor_name: "Hope charity".into(),
            ..Payment::default()
        };
        assert_eq!(
            build(&payment.resolve().unwrap()),
            "https://payme.sk/2/q/PME?IBAN=SK6807200002891987426353&CN=Hope+charity"
        );
    }
}
