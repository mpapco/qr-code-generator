//! A single-line text input: the value plus a cursor, in byte offsets.

/// Which characters a field accepts, so that invalid input cannot be typed at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accepts {
    Any,
    /// IBAN: letters, digits and spaces, upper-cased as you type.
    Iban,
    /// An amount: digits and a single decimal separator.
    Amount,
    /// A date: digits and dashes.
    Date,
    /// Digits only, as the Slovak payment symbols require.
    Digits,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextField {
    value: String,
    /// Byte offset of the cursor within `value`; always on a char boundary.
    cursor: usize,
}

impl TextField {
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        let cursor = value.len();
        Self { value, cursor }
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }

    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    /// Whether the cursor sits past the last character, where a completion can
    /// be accepted without swallowing a plain cursor movement.
    pub fn at_end(&self) -> bool {
        self.cursor == self.value.len()
    }

    /// Number of characters before the cursor, i.e. the column to draw it at.
    pub fn cursor_col(&self) -> usize {
        self.value[..self.cursor].chars().count()
    }

    pub fn set(&mut self, value: impl Into<String>) {
        self.value = value.into();
        self.cursor = self.cursor.min(self.value.len());
        while !self.value.is_char_boundary(self.cursor) {
            self.cursor -= 1;
        }
    }

    /// Insert `c` unless the field's rules reject it or `max_len` is reached.
    pub fn insert(&mut self, c: char, accepts: Accepts, max_len: usize) {
        let Some(c) = sanitize(c, accepts, &self.value) else {
            return;
        };
        if self.value.chars().count() >= max_len {
            return;
        }
        self.value.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    pub fn backspace(&mut self) {
        if let Some(prev) = self.prev_boundary() {
            self.value.remove(prev);
            self.cursor = prev;
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.value.len() {
            self.value.remove(self.cursor);
        }
    }

    /// Delete the word before the cursor (Ctrl+W).
    pub fn delete_word(&mut self) {
        let head = &self.value[..self.cursor];
        let trimmed = head.trim_end();
        let start = match trimmed.rfind(char::is_whitespace) {
            Some(at) => at + 1,
            None => 0,
        };
        self.value.replace_range(start..self.cursor, "");
        self.cursor = start;
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
    }

    pub fn left(&mut self) {
        if let Some(prev) = self.prev_boundary() {
            self.cursor = prev;
        }
    }

    pub fn right(&mut self) {
        if self.cursor < self.value.len() {
            self.cursor += self.value[self.cursor..]
                .chars()
                .next()
                .map_or(0, char::len_utf8);
        }
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.value.len();
    }

    fn prev_boundary(&self) -> Option<usize> {
        self.value[..self.cursor]
            .chars()
            .next_back()
            .map(|c| self.cursor - c.len_utf8())
    }
}

/// Apply a field's input rules to one character.
fn sanitize(c: char, accepts: Accepts, current: &str) -> Option<char> {
    if c.is_control() {
        return None;
    }
    match accepts {
        Accepts::Any => Some(c),
        Accepts::Iban => (c.is_ascii_alphanumeric() || c == ' ').then(|| c.to_ascii_uppercase()),
        Accepts::Amount => match c {
            '0'..='9' => Some(c),
            // Only one decimal separator, normalised to a dot on the way out.
            '.' | ',' => (!current.contains(['.', ','])).then_some(c),
            _ => None,
        },
        Accepts::Date => (c.is_ascii_digit() || c == '-').then_some(c),
        Accepts::Digits => c.is_ascii_digit().then_some(c),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(text: &str, accepts: Accepts, max_len: usize) -> TextField {
        let mut field = TextField::default();
        for c in text.chars() {
            field.insert(c, accepts, max_len);
        }
        field
    }

    #[test]
    fn typing_respects_the_field_rules() {
        assert_eq!(
            typed("sk68 0720x!", Accepts::Iban, 34).as_str(),
            "SK68 0720X"
        );
        assert_eq!(typed("12,3,4a", Accepts::Amount, 12).as_str(), "12,34");
        assert_eq!(
            typed("2028-04-30", Accepts::Date, 10).as_str(),
            "2028-04-30"
        );
        assert_eq!(typed("12a34", Accepts::Digits, 10).as_str(), "1234");
    }

    #[test]
    fn the_length_limit_stops_input() {
        assert_eq!(typed("123456", Accepts::Digits, 4).as_str(), "1234");
    }

    #[test]
    fn editing_moves_the_cursor_over_multibyte_text() {
        let mut field = TextField::new("K\u{f3}\u{161}a");
        assert_eq!(field.cursor_col(), 4);
        field.left();
        field.backspace();
        assert_eq!(field.as_str(), "K\u{f3}a");
        assert_eq!(field.cursor_col(), 2);
        field.home();
        field.delete();
        assert_eq!(field.as_str(), "\u{f3}a");
    }

    #[test]
    fn deletes_a_word_at_a_time() {
        let mut field = TextField::new("Thank you for lunch");
        field.delete_word();
        assert_eq!(field.as_str(), "Thank you for ");
        field.delete_word();
        assert_eq!(field.as_str(), "Thank you ");
    }

    #[test]
    fn setting_a_value_keeps_the_cursor_valid() {
        let mut field = TextField::new("/VS123456/SS/KS");
        field.set("/VS1/SS/KS");
        assert!(field.cursor_col() <= field.as_str().chars().count());
        field.end();
        assert_eq!(field.cursor_col(), 10);
    }
}
