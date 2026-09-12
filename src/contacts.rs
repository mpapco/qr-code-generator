//! Recipients remembered between sessions.
//!
//! A payment is nearly always sent to someone the user has paid before, so the
//! recipient's name and IBAN are kept in a small YAML file — the name is the
//! key, the IBAN the value:
//!
//! ```yaml
//! # ~/.config/payme-qr/contacts.yml
//! "The Best e-shops ltd": SK3709008482989234185969
//! ```
//!
//! The file is meant to be readable and editable by hand, so it is parsed as a
//! flat mapping of scalars: comments, blank lines and a leading `---` are
//! ignored, and both plain and double-quoted scalars are accepted. Anything
//! richer than that (nesting, anchors, block scalars) is not YAML we write, and
//! lines we cannot read are skipped rather than treated as an error — a
//! corrupted history must never stop the generator from working.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Where the history lives when the user does not say otherwise.
const DIR_NAME: &str = "payme-qr";
const FILE_NAME: &str = "contacts.yml";
/// Overrides the location of the history file.
pub const PATH_ENV: &str = "PAYME_QR_CONTACTS";

/// Name-to-IBAN pairs, ordered by name so the file has a stable diff.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Contacts {
    entries: BTreeMap<String, String>,
}

impl Contacts {
    /// `$PAYME_QR_CONTACTS`, else `$XDG_CONFIG_HOME/payme-qr/contacts.yml`,
    /// else `~/.config/payme-qr/contacts.yml`.
    pub fn default_path() -> Option<PathBuf> {
        if let Some(path) = std::env::var_os(PATH_ENV).filter(|p| !p.is_empty()) {
            return Some(PathBuf::from(path));
        }
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .filter(|p| !p.is_empty())
                    .map(|home| PathBuf::from(home).join(".config"))
            })?;
        Some(config.join(DIR_NAME).join(FILE_NAME))
    }

    /// Read the file. A file that is not there yet is simply an empty history.
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(Self::parse(&text)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => {
                Err(anyhow::Error::new(error).context(format!("reading {}", path.display())))
            }
        }
    }

    /// Load from the default location, treating every failure as "no history".
    /// Remembering recipients is a convenience; it must never break the form.
    pub fn load_default() -> Self {
        Self::default_path()
            .and_then(|path| Self::load(&path).ok())
            .unwrap_or_default()
    }

    pub fn parse(text: &str) -> Self {
        let mut entries = BTreeMap::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line == "---" || line == "..." {
                continue;
            }
            let Some((name, iban)) = split_pair(line) else {
                continue;
            };
            if !name.is_empty() && !iban.is_empty() {
                entries.insert(name, iban);
            }
        }
        Self { entries }
    }

    pub fn to_yaml(&self) -> String {
        let mut out = String::from(
            "# Recipients remembered by payme-qr: name (key) to IBAN (value).\n\
             # Edit or delete entries freely; the file is rewritten as codes are generated.\n",
        );
        for (name, iban) in &self.entries {
            out.push_str(&format!("{}: {}\n", write_scalar(name), write_scalar(iban)));
        }
        out
    }

    /// Write the file, creating the directory it lives in.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::write(path, self.to_yaml()).with_context(|| format!("writing {}", path.display()))
    }

    /// Record a recipient. Returns `false` when the history already said this,
    /// so a caller can skip rewriting an unchanged file.
    pub fn remember(&mut self, name: &str, iban: &str) -> bool {
        let name = clean(name);
        let iban = clean(iban);
        if name.is_empty() || iban.is_empty() {
            return false;
        }
        // A name is one recipient however it was capitalised at the time.
        let existing = self.key_for(&name);
        if let Some(key) = existing {
            if key == name && self.entries.get(&key).is_some_and(|v| *v == iban) {
                return false;
            }
            self.entries.remove(&key);
        }
        self.entries.insert(name, iban);
        true
    }

    pub fn forget(&mut self, name: &str) -> bool {
        match self.key_for(&clean(name)) {
            Some(key) => self.entries.remove(&key).is_some(),
            None => false,
        }
    }

    fn key_for(&self, name: &str) -> Option<String> {
        self.entries
            .keys()
            .find(|key| key.eq_ignore_ascii_case(name))
            .cloned()
    }

    /// The IBAN stored for a name, matched without regard to capitalisation.
    pub fn iban_for(&self, name: &str) -> Option<&str> {
        let name = clean(name);
        self.entries
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(&name))
            .map(|(_, iban)| iban.as_str())
    }

    /// Every remembered name starting with what has been typed so far, in
    /// alphabetical order. An empty prefix suggests nothing: completion should
    /// appear as the user types, not before.
    pub fn matches(&self, prefix: &str) -> Vec<(&str, &str)> {
        let prefix = prefix.trim_start();
        if prefix.is_empty() {
            return Vec::new();
        }
        let folded = prefix.to_lowercase();
        self.entries
            .iter()
            .filter(|(name, _)| name.to_lowercase().starts_with(&folded))
            .map(|(name, iban)| (name.as_str(), iban.as_str()))
            .collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries
            .iter()
            .map(|(name, iban)| (name.as_str(), iban.as_str()))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Collapse the whitespace of a value on its way into the history, so that the
/// same recipient typed with a stray trailing space is not stored twice.
fn clean(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Split `key: value`, honouring a quoted key so that a name containing a
/// colon still reads back correctly.
fn split_pair(line: &str) -> Option<(String, String)> {
    if let Some(rest) = line.strip_prefix('"') {
        let (key, after) = read_quoted(rest)?;
        let value = after.trim_start().strip_prefix(':')?;
        return Some((key, read_scalar(value)));
    }
    // A plain key ends at the first colon that is followed by a space or the
    // end of the line, which is what YAML requires of a plain scalar.
    let bytes = line.as_bytes();
    let at = (0..line.len())
        .find(|i| bytes[*i] == b':' && bytes.get(i + 1).is_none_or(|c| c.is_ascii_whitespace()))?;
    Some((read_scalar(&line[..at]), read_scalar(&line[at + 1..])))
}

/// Read a double-quoted scalar, returning it and whatever follows the closing
/// quote. Only the escapes we emit are recognised.
fn read_quoted(rest: &str) -> Option<(String, &str)> {
    let mut value = String::new();
    let mut chars = rest.char_indices();
    while let Some((index, c)) = chars.next() {
        match c {
            '"' => return Some((value, &rest[index + 1..])),
            '\\' => match chars.next()?.1 {
                'n' => value.push('\n'),
                't' => value.push('\t'),
                other => value.push(other),
            },
            other => value.push(other),
        }
    }
    None
}

fn read_scalar(raw: &str) -> String {
    let raw = raw.trim();
    if let Some(rest) = raw.strip_prefix('"') {
        if let Some((value, _)) = read_quoted(rest) {
            return value;
        }
    }
    // Strip a trailing comment; ` #` only starts one when it follows a space.
    match raw.find(" #") {
        Some(at) => raw[..at].trim_end().to_string(),
        None => raw.to_string(),
    }
}

fn write_scalar(value: &str) -> String {
    if is_plain_safe(value) {
        return value.to_string();
    }
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// Whether a scalar can be written without quotes: it must not start with a
/// YAML indicator, must not hold a `:`, a `#` or an edge space, and must not
/// look like a number or a boolean to a real YAML reader.
fn is_plain_safe(value: &str) -> bool {
    if value.is_empty() || value.trim() != value {
        return false;
    }
    if value.starts_with(|c: char| "-?:,[]{}#&*!|>'\"%@`".contains(c)) {
        return false;
    }
    if value
        .chars()
        .any(|c| c.is_control() || c == ':' || c == '#')
    {
        return false;
    }
    let folded = value.to_ascii_lowercase();
    if matches!(
        folded.as_str(),
        "true" | "false" | "yes" | "no" | "on" | "off" | "null" | "~"
    ) {
        return false;
    }
    value.parse::<f64>().is_err()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Contacts {
        let mut contacts = Contacts::default();
        contacts.remember("Alice Payee", "SK6807200002891987426353");
        contacts.remember("Alan Turing", "SK3709008482989234185969");
        contacts.remember("Bob's Bakery", "SK2256002926649607918409");
        contacts
    }

    #[test]
    fn a_recipient_survives_a_write_and_a_read() {
        let contacts = sample();
        assert_eq!(Contacts::parse(&contacts.to_yaml()), contacts);
        assert_eq!(
            contacts.iban_for("alice payee"),
            Some("SK6807200002891987426353")
        );
    }

    #[test]
    fn awkward_names_are_quoted_and_read_back() {
        let mut contacts = Contacts::default();
        contacts.remember("Sport: klub, o.z.", "SK6807200002891987426353");
        contacts.remember("42", "SK3709008482989234185969");
        let yaml = contacts.to_yaml();
        assert!(yaml.contains("\"Sport: klub, o.z.\":"), "{yaml}");
        assert!(yaml.contains("\"42\":"), "{yaml}");
        assert_eq!(Contacts::parse(&yaml), contacts);
    }

    #[test]
    fn a_hand_written_file_is_understood() {
        let contacts = Contacts::parse(
            "---\n\
             # my payees\n\
             \n\
             Alice Payee: SK6807200002891987426353  # lunch money\n\
             \"Bob's Bakery\" : \"SK2256002926649607918409\"\n\
             nonsense without a separator\n",
        );
        assert_eq!(contacts.len(), 2);
        assert_eq!(
            contacts.iban_for("Bob's Bakery"),
            Some("SK2256002926649607918409")
        );
        assert_eq!(
            contacts.iban_for("Alice Payee"),
            Some("SK6807200002891987426353"),
            "an inline comment is not part of the IBAN"
        );
    }

    #[test]
    fn completion_offers_every_name_with_the_prefix() {
        let contacts = sample();
        let names: Vec<&str> = contacts.matches("al").iter().map(|(n, _)| *n).collect();
        assert_eq!(names, ["Alan Turing", "Alice Payee"]);
        assert_eq!(contacts.matches("ali").len(), 1);
        assert!(
            contacts.matches("").is_empty(),
            "nothing is suggested before the first keystroke"
        );
        assert!(contacts.matches("zz").is_empty());
    }

    #[test]
    fn remembering_the_same_pair_twice_changes_nothing() {
        let mut contacts = sample();
        assert!(!contacts.remember("Alice Payee", "SK6807200002891987426353"));
        assert!(!contacts.remember("  Alice   Payee ", "SK6807200002891987426353"));
        assert!(contacts.remember("Alice Payee", "SK3709008482989234185969"));
        assert_eq!(contacts.len(), 3, "the IBAN was replaced, not added");
        assert!(!contacts.remember("Nobody", ""));
    }

    #[test]
    fn a_new_capitalisation_replaces_the_old_entry() {
        let mut contacts = sample();
        assert!(contacts.remember("ALICE PAYEE", "SK6807200002891987426353"));
        assert_eq!(contacts.len(), 3);
        assert_eq!(
            contacts.matches("ALI"),
            [("ALICE PAYEE", "SK6807200002891987426353")]
        );
    }

    #[test]
    fn the_file_is_created_along_with_its_directory() {
        let dir = std::env::temp_dir().join("payme-qr-tests/history/nested");
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
        let path = dir.join("contacts.yml");

        let contacts = sample();
        contacts.save(&path).unwrap();
        assert_eq!(Contacts::load(&path).unwrap(), contacts);

        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
        assert_eq!(
            Contacts::load(&path).unwrap(),
            Contacts::default(),
            "a missing file is an empty history, not an error"
        );
    }
}
