//! Normalisation of free-text attributes to the Annex A character set.
//!
//! SS 3.4.7 and SS 5.1 recommend replacing national Slovak characters with their
//! ASCII equivalents and dropping anything outside the recommended set, so that
//! the link stays readable and the QR code stays small.

use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

use crate::spec::is_recommended_char;

/// Result of normalising one field.
pub struct Normalized {
    pub value: String,
    /// True when the transmitted value differs from what the user typed.
    pub changed: bool,
}

/// Map to the Annex A character set: decompose accents away, fold whitespace to
/// spaces, and drop whatever is still outside the set.
pub fn normalize(input: &str) -> Normalized {
    let mut value = String::with_capacity(input.len());

    for c in input.nfd() {
        if is_combining_mark(c) {
            continue;
        }
        if c.is_whitespace() {
            value.push(' ');
        } else if is_recommended_char(c) {
            value.push(c);
        }
        // Anything else is dropped, as the standard permits.
    }

    Normalized {
        changed: value != input,
        value,
    }
}

/// Characters of `input` that are outside the recommended set, de-duplicated
/// and in order of first appearance. Used to explain what normalisation will do.
pub fn unsupported_chars(input: &str) -> Vec<char> {
    let mut out: Vec<char> = Vec::new();
    for c in input.chars() {
        if !is_recommended_char(c) && !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_slovak_diacritics() {
        let n = normalize("\u{17d}ilina, Ko\u{161}ice \u{2013} \u{10c}au");
        assert_eq!(n.value, "Zilina, Kosice  Cau");
        assert!(n.changed);
    }

    #[test]
    fn covers_every_slovak_accented_letter() {
        let n = normalize("\u{e1}\u{e4}\u{10d}\u{10f}\u{e9}\u{ed}\u{13a}\u{13e}\u{148}\u{f3}\u{f4}\u{155}\u{161}\u{165}\u{fa}\u{fd}\u{17e}");
        assert_eq!(n.value, "aacdeillnoorstuyz");
    }

    #[test]
    fn keeps_recommended_text_untouched() {
        let n = normalize("Cafe on the corner, Zilina (2nd floor) - table 4");
        assert_eq!(n.value, "Cafe on the corner, Zilina (2nd floor) - table 4");
        assert!(!n.changed);
    }

    #[test]
    fn drops_characters_outside_annex_a() {
        let n = normalize("Ivan & Co. #7 \u{201e}best\u{201c}");
        assert_eq!(n.value, "Ivan  Co. 7 best");
    }

    #[test]
    fn reports_unsupported_characters_once_each() {
        assert_eq!(unsupported_chars("a&b&c#"), vec!['&', '#']);
        assert!(unsupported_chars("Plain text 123").is_empty());
    }
}
