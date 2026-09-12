//! IBAN normalisation and validation.
//!
//! The standard itself only requires the ISO 20022 pattern and the ISO 13616
//! mod-97 checksum (SS 3.4.1). The payme.sk generator applies extra checks to
//! Slovak accounts - bank code and the two mod-11 weight checks - and also
//! accepts a 20-digit Slovak BBAN, converting it to an IBAN. Both are mirrored
//! here so that this tool accepts and rejects exactly what the website does.

/// Bank codes accepted for Slovak accounts by the payme.sk generator.
const SK_BANK_CODES: [&str; 36] = [
    "0200", "0720", "0900", "1100", "1111", "3000", "3100", "5200", "5600", "5900", "6500", "7300",
    "7500", "7930", "8050", "8100", "8120", "8130", "8160", "8170", "8180", "8191", "8320", "8330",
    "8360", "8370", "8390", "8400", "8420", "8430", "8440", "8450", "9950", "9951", "9953", "9952",
];

const SK_PREFIX_WEIGHTS: [u32; 6] = [10, 5, 8, 4, 2, 1];
const SK_ACCOUNT_WEIGHTS: [u32; 10] = [6, 3, 7, 9, 10, 5, 8, 4, 2, 1];

/// Strip whitespace and upper-case, the way the generator does before validating.
pub fn normalize(input: &str) -> String {
    input
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(|c| c.to_uppercase())
        .collect()
}

/// Validate an IBAN (or a Slovak BBAN) and return it in canonical IBAN form.
pub fn parse(input: &str) -> Result<String, String> {
    let normalized = normalize(input);
    if normalized.is_empty() {
        return Err("IBAN is required.".into());
    }

    if looks_like_iban(&normalized) {
        validate_iban(&normalized)?;
        return Ok(normalized);
    }

    // Otherwise: a Slovak BBAN, with separators already stripped.
    if !normalized.chars().all(|c| c.is_ascii_digit()) {
        return Err("Not a valid IBAN.".into());
    }
    if normalized.len() != 20 {
        return Err("Not a valid BBAN (a Slovak BBAN has 20 digits).".into());
    }
    let iban = bban_to_iban("SK", &normalized);
    validate_iban(&iban)?;
    Ok(iban)
}

fn looks_like_iban(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.len() >= 5
        && bytes[0].is_ascii_uppercase()
        && bytes[1].is_ascii_uppercase()
        && bytes[2].is_ascii_digit()
        && bytes[3].is_ascii_digit()
        && bytes[4..].iter().all(|b| b.is_ascii_alphanumeric())
}

fn validate_iban(iban: &str) -> Result<(), String> {
    if iban.len() > 34 {
        return Err("Not a valid IBAN (longer than 34 characters).".into());
    }
    if mod97(iban) != 1 {
        return Err("Not a valid IBAN (checksum).".into());
    }
    if iban.starts_with("SK") {
        validate_sk(iban)?;
    }
    Ok(())
}

/// ISO 7064 MOD 97-10 over the IBAN with the first four characters rotated to the end.
fn mod97(iban: &str) -> u32 {
    let rotated = iban[4..].chars().chain(iban[..4].chars());
    rotated.fold(0u32, |acc, c| {
        // 'A' -> 10 ... 'Z' -> 35, digits map to themselves.
        if c.is_ascii_digit() {
            (acc * 10 + (c as u32 - '0' as u32)) % 97
        } else {
            let v = c as u32 - 'A' as u32 + 10;
            (acc * 100 + v) % 97
        }
    })
}

/// Slovak specifics: fixed length, known bank code, mod-11 prefix and account.
fn validate_sk(iban: &str) -> Result<(), String> {
    if iban.len() != 24 || !iban[2..].chars().all(|c| c.is_ascii_digit()) {
        return Err("Not a valid IBAN (a Slovak IBAN has 24 characters).".into());
    }

    let bank_code = &iban[4..8];
    if !SK_BANK_CODES.contains(&bank_code) {
        return Err(format!("Not a valid IBAN (unknown bank code {bank_code})."));
    }

    if !weighted_sum(&iban[8..14], &SK_PREFIX_WEIGHTS).is_multiple_of(11) {
        return Err("Not a valid IBAN (account prefix).".into());
    }
    if !weighted_sum(&iban[14..24], &SK_ACCOUNT_WEIGHTS).is_multiple_of(11) {
        return Err("Not a valid IBAN (account number).".into());
    }
    Ok(())
}

fn weighted_sum(digits: &str, weights: &[u32]) -> u32 {
    digits
        .chars()
        .zip(weights)
        .map(|(c, w)| c.to_digit(10).unwrap_or(0) * w)
        .sum()
}

/// Build an IBAN from a country code and a BBAN by computing the check digits.
pub fn bban_to_iban(country: &str, bban: &str) -> String {
    let check = 98 - mod97(&format!("{country}00{bban}"));
    format!("{country}{check:02}{bban}")
}

/// Group an IBAN into blocks of four for display.
pub fn format_pretty(iban: &str) -> String {
    iban.as_bytes()
        .chunks(4)
        .map(|c| std::str::from_utf8(c).unwrap_or_default())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The IBAN used throughout the standard's examples.
    const SPEC_IBAN: &str = "SK6807200002891987426353";

    #[test]
    fn accepts_the_spec_example_in_any_spacing() {
        assert_eq!(parse(SPEC_IBAN).unwrap(), SPEC_IBAN);
        assert_eq!(parse("SK68 0720 0002 8919 8742 6353").unwrap(), SPEC_IBAN);
        assert_eq!(parse("sk68 0720 0002 8919 8742 6353").unwrap(), SPEC_IBAN);
    }

    #[test]
    fn rejects_a_broken_checksum() {
        assert!(parse("SK6907200002891987426353").is_err());
    }

    #[test]
    fn rejects_an_unknown_bank_code() {
        // Same account, bank code 0721, check digits recomputed so only the
        // Slovak bank-code rule can reject it.
        let iban = bban_to_iban("SK", "072100028919874263 53".replace(' ', "").as_str());
        assert!(mod97(&iban) == 1);
        assert!(parse(&iban).unwrap_err().contains("bank code"));
    }

    #[test]
    fn converts_a_slovak_bban() {
        assert_eq!(parse("0720 000289 1987426353").unwrap(), SPEC_IBAN);
        assert_eq!(
            parse("072000028919874263").unwrap_err(),
            "Not a valid BBAN (a Slovak BBAN has 20 digits)."
        );
    }

    #[test]
    fn accepts_a_foreign_iban_without_slovak_rules() {
        assert_eq!(
            parse("CZ65 0800 0000 1920 0014 5399").unwrap(),
            "CZ6508000000192000145399"
        );
    }

    #[test]
    fn pretty_printing_groups_by_four() {
        assert_eq!(format_pretty(SPEC_IBAN), "SK68 0720 0002 8919 8742 6353");
    }
}
