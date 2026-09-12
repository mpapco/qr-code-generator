//! The Slovak payment symbols carried inside the Payment identification (PI).
//!
//! SS 3.4.5 defines the encoding `/VS{0,10}/SS{0,10}/KS{0,4}`. The payme.sk form
//! keeps a free-text reference field and the three symbol fields in sync; that
//! two-way sync is reproduced here so the TUI behaves the same way.

/// The three Slovak payment symbols, as digit strings.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Symbols {
    pub vs: String,
    pub ss: String,
    pub ks: String,
}

impl Symbols {
    pub fn is_empty(&self) -> bool {
        self.vs.is_empty() && self.ss.is_empty() && self.ks.is_empty()
    }
}

/// Keep only digits, the way the form sanitises the symbol inputs.
pub fn digits_only(input: &str) -> String {
    input.chars().filter(char::is_ascii_digit).collect()
}

/// Build the canonical reference from the symbols. All three tokens are always
/// present once any symbol is filled, matching `buildReference()` on the site.
pub fn build(symbols: &Symbols) -> String {
    if symbols.is_empty() {
        return String::new();
    }
    format!("/VS{}/SS{}/KS{}", symbols.vs, symbols.ss, symbols.ks)
}

/// Recover the symbols from a reference. Partial input is tolerated so the
/// fields can be kept in sync while the user is still typing.
pub fn parse(reference: &str) -> Symbols {
    let up = reference.to_ascii_uppercase();
    Symbols {
        vs: capture(&up, "/VS"),
        ss: capture(&up, "/SS"),
        ks: capture(&up, "/KS"),
    }
}

fn capture(haystack: &str, token: &str) -> String {
    match haystack.find(token) {
        Some(at) => haystack[at + token.len()..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect(),
        None => String::new(),
    }
}

/// True when `reference` is exactly the canonical symbol encoding.
pub fn is_canonical(reference: &str) -> bool {
    let up = reference.to_ascii_uppercase();
    let Some(rest) = up.strip_prefix("/VS") else {
        return false;
    };
    let Some((vs, rest)) = split_at_token(rest, "/SS") else {
        return false;
    };
    let Some((ss, ks)) = split_at_token(rest, "/KS") else {
        return false;
    };
    vs.len() <= 10
        && ss.len() <= 10
        && ks.len() <= 4
        && [vs, ss, ks]
            .iter()
            .all(|part| part.chars().all(|c| c.is_ascii_digit()))
}

fn split_at_token<'a>(input: &'a str, token: &str) -> Option<(&'a str, &'a str)> {
    let at = input.find(token)?;
    Some((&input[..at], &input[at + token.len()..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbols(vs: &str, ss: &str, ks: &str) -> Symbols {
        Symbols {
            vs: vs.into(),
            ss: ss.into(),
            ks: ks.into(),
        }
    }

    #[test]
    fn builds_the_reference_from_the_spec_example() {
        // SS 3.4.5: /VS2546874464 /SS2019568456 /KS1118
        let s = symbols("2546874464", "2019568456", "1118");
        assert_eq!(build(&s), "/VS2546874464/SS2019568456/KS1118");
    }

    #[test]
    fn empty_symbols_produce_no_reference() {
        assert_eq!(build(&Symbols::default()), "");
    }

    #[test]
    fn a_single_symbol_still_emits_all_three_tokens() {
        assert_eq!(build(&symbols("123", "", "")), "/VS123/SS/KS");
    }

    #[test]
    fn round_trips_through_parse() {
        let s = symbols("2546874464", "2019568456", "1118");
        assert_eq!(parse(&build(&s)), s);
        assert_eq!(
            parse(&build(&symbols("123", "", ""))),
            symbols("123", "", "")
        );
    }

    #[test]
    fn parsing_free_text_yields_nothing() {
        assert_eq!(parse("INVOICE-2026-01"), Symbols::default());
    }

    #[test]
    fn recognises_the_canonical_form() {
        assert!(is_canonical("/VS2546874464/SS2019568456/KS1118"));
        assert!(is_canonical("/VS/SS/KS"));
        assert!(!is_canonical("/VS12345678901/SS/KS")); // VS too long
        assert!(!is_canonical("/VS1/SS/KS12345")); // KS too long
        assert!(!is_canonical("QR-ab29e346f1d8"));
    }

    #[test]
    fn keeps_only_digits() {
        assert_eq!(digits_only(" 12 34a5 "), "12345");
    }
}
