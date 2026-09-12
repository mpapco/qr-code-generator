//! State of the interactive form and the key handling that drives it.

use std::path::PathBuf;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::contacts::Contacts;
use crate::payment::{FieldError, Payment};
use crate::qr::{self, Matrix, TerminalStyle};
use crate::reference::{self, Symbols};
use crate::spec::{Field, Preset, Requirement, CURRENCY_EUR, PRESETS};
use crate::tui::field::{Accepts, TextField};

/// One line of the form. `Preset` is the type selector, the rest are inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Preset,
    Name,
    Iban,
    Amount,
    Currency,
    DueDate,
    Reference,
    Vs,
    Ks,
    Ss,
    Message,
}

pub const ROWS: [Row; 11] = [
    Row::Preset,
    Row::Name,
    Row::Iban,
    Row::Amount,
    Row::Currency,
    Row::DueDate,
    Row::Reference,
    Row::Vs,
    Row::Ks,
    Row::Ss,
    Row::Message,
];

impl Row {
    fn index(self) -> usize {
        ROWS.iter()
            .position(|r| *r == self)
            .expect("row is listed in ROWS")
    }

    pub fn label(self) -> &'static str {
        match self {
            Row::Preset => "Payment type",
            Row::Name => "Recipient name",
            Row::Iban => "IBAN",
            Row::Amount => "Amount",
            Row::Currency => "Currency",
            Row::DueDate => "Due date",
            Row::Reference => "Reference",
            Row::Vs => "Variable symbol",
            Row::Ks => "Constant symbol",
            Row::Ss => "Specific symbol",
            Row::Message => "Message",
        }
    }

    /// The attribute this row feeds, when it maps to one directly.
    pub fn spec_field(self) -> Option<Field> {
        match self {
            Row::Name => Some(Field::CreditorName),
            Row::Iban => Some(Field::Iban),
            Row::Amount => Some(Field::Amount),
            Row::Currency => Some(Field::Currency),
            Row::DueDate => Some(Field::DueDate),
            // The symbols are folded into the payment identification.
            Row::Reference | Row::Vs | Row::Ks | Row::Ss => Some(Field::PaymentId),
            Row::Message => Some(Field::Message),
            Row::Preset => None,
        }
    }

    fn accepts(self) -> Accepts {
        match self {
            Row::Iban => Accepts::Iban,
            Row::Amount => Accepts::Amount,
            Row::DueDate => Accepts::Date,
            Row::Vs | Row::Ks | Row::Ss => Accepts::Digits,
            _ => Accepts::Any,
        }
    }

    /// Length limit enforced while typing.
    fn max_len(self) -> usize {
        match self {
            Row::Preset => 0,
            // Room for the spaces of a grouped IBAN.
            Row::Iban => 42,
            Row::DueDate => 10,
            Row::Vs | Row::Ss => 10,
            Row::Ks => 4,
            other => other.spec_field().map_or(64, Field::max_len),
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Row::Preset => "Left/Right choose the payment context",
            Row::Name => "Beneficiary of the payment (CN)",
            Row::Iban => "IBAN, or a 20-digit Slovak BBAN",
            Row::Amount => "Up to 9 digits, or 7 digits with 1-2 decimals",
            Row::Currency => "Version 2 of the standard only supports EUR",
            Row::DueDate => "YYYY-MM-DD; only person-to-person links carry it",
            Row::Reference => "End-to-end reference (PI); symbols fill it in",
            Row::Vs => "Variable symbol, up to 10 digits",
            Row::Ks => "Constant symbol, up to 4 digits",
            Row::Ss => "Specific symbol, up to 10 digits",
            Row::Message => "Message for the recipient, up to 140 characters",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveKind {
    Png,
    Svg,
}

impl SaveKind {
    fn default_path(self) -> &'static str {
        match self {
            SaveKind::Png => "payme.png",
            SaveKind::Svg => "payme.svg",
        }
    }

    fn label(self) -> &'static str {
        match self {
            SaveKind::Png => "PNG",
            SaveKind::Svg => "SVG",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Edit,
    /// Asking for a file name before writing the QR code.
    Save {
        kind: SaveKind,
        path: TextField,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Info,
    Success,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub text: String,
    pub kind: StatusKind,
}

/// What the current form produces: either a link and its symbol, or the reasons
/// why it is not a valid payment yet.
#[derive(Debug, Clone)]
pub enum Outcome {
    Ready { url: String, matrix: Matrix },
    Invalid(Vec<FieldError>),
}

/// A remembered recipient offered for the name being typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// The full remembered name.
    pub name: String,
    /// The part of it still to be typed, drawn after the cursor.
    pub completion: String,
    /// Position among the candidates, for the `2/4` counter.
    pub index: usize,
    pub total: usize,
}

/// What became of a recipient on its way to the history file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Remembered {
    /// Nothing to write: no valid payment, or the history already said this.
    Unchanged,
    Added(String),
    Failed(String),
}

pub struct App {
    pub preset: Preset,
    pub focus: usize,
    inputs: Vec<TextField>,
    pub normalize: bool,
    pub style: TerminalStyle,
    pub mode: Mode,
    pub status: Status,
    pub outcome: Outcome,
    pub should_quit: bool,
    /// Recipients from previous sessions, and where they are kept.
    contacts: Contacts,
    contacts_path: Option<PathBuf>,
    /// Remembered names that start with what has been typed into the name row.
    candidates: Vec<(String, String)>,
    candidate: usize,
    /// Whether the IBAN on screen was filled in from the history. Only such an
    /// IBAN may be replaced when the name changes; one the user typed is theirs.
    iban_from_history: bool,
}

impl Default for App {
    fn default() -> Self {
        Self::with_history(Contacts::load_default(), Contacts::default_path())
    }
}

impl App {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build the form over a given recipient history. Taking it as an argument
    /// keeps the tests off the user's real history file.
    pub fn with_history(contacts: Contacts, contacts_path: Option<PathBuf>) -> Self {
        let mut app = Self {
            preset: Preset::P2p,
            focus: Row::Name.index(),
            inputs: ROWS.iter().map(|_| TextField::default()).collect(),
            normalize: true,
            style: TerminalStyle::HalfBlocks,
            mode: Mode::Edit,
            status: Status {
                text: String::new(),
                kind: StatusKind::Info,
            },
            outcome: Outcome::Invalid(Vec::new()),
            should_quit: false,
            contacts,
            contacts_path,
            candidates: Vec::new(),
            candidate: 0,
            iban_from_history: false,
        };
        app.input_mut(Row::Currency).set(CURRENCY_EUR);
        app.recompute();
        app
    }

    pub fn input(&self, row: Row) -> &TextField {
        &self.inputs[row.index()]
    }

    fn input_mut(&mut self, row: Row) -> &mut TextField {
        &mut self.inputs[row.index()]
    }

    pub fn focused_row(&self) -> Row {
        ROWS[self.focus]
    }

    /// Rows whose attribute the current type omits are shown but skipped over.
    pub fn is_disabled(&self, row: Row) -> bool {
        row.spec_field()
            .is_some_and(|field| self.preset.requirement(field) == Requirement::Omitted)
    }

    pub fn requirement(&self, row: Row) -> Option<Requirement> {
        row.spec_field().map(|field| self.preset.requirement(field))
    }

    /// Errors attached to a row, so they can be shown next to the input.
    pub fn errors_for(&self, row: Row) -> Vec<&FieldError> {
        let Outcome::Invalid(errors) = &self.outcome else {
            return Vec::new();
        };
        let Some(field) = row.spec_field() else {
            return Vec::new();
        };
        // The symbols share one attribute; the message belongs on the reference row.
        if matches!(row, Row::Vs | Row::Ks | Row::Ss) {
            return Vec::new();
        }
        errors.iter().filter(|e| e.field == field).collect()
    }

    pub fn symbols(&self) -> Symbols {
        Symbols {
            vs: self.input(Row::Vs).as_str().to_string(),
            ss: self.input(Row::Ss).as_str().to_string(),
            ks: self.input(Row::Ks).as_str().to_string(),
        }
    }

    pub fn payment(&self) -> Payment {
        Payment {
            preset: self.preset,
            creditor_name: self.input(Row::Name).as_str().to_string(),
            iban: self.input(Row::Iban).as_str().to_string(),
            amount: self.input(Row::Amount).as_str().to_string(),
            currency: self.input(Row::Currency).as_str().to_string(),
            due_date: self.input(Row::DueDate).as_str().to_string(),
            reference: self.input(Row::Reference).as_str().to_string(),
            message: self.input(Row::Message).as_str().to_string(),
            normalize_text: self.normalize,
        }
    }

    /// Rebuild the link and the symbol from the current form.
    pub fn recompute(&mut self) {
        self.outcome = match crate::generate(&self.payment()) {
            Ok((url, matrix)) => Outcome::Ready { url, matrix },
            Err(errors) => Outcome::Invalid(errors),
        };
    }

    pub fn url(&self) -> Option<&str> {
        match &self.outcome {
            Outcome::Ready { url, .. } => Some(url),
            Outcome::Invalid(_) => None,
        }
    }

    pub fn matrix(&self) -> Option<&Matrix> {
        match &self.outcome {
            Outcome::Ready { matrix, .. } => Some(matrix),
            Outcome::Invalid(_) => None,
        }
    }

    // -- remembered recipients -------------------------------------------

    /// Whether the IBAN on screen came from the history rather than the keyboard.
    pub fn iban_from_history(&self) -> bool {
        self.iban_from_history && !self.input(Row::Iban).is_empty()
    }

    /// The completion to draw after the name being typed, if any is left.
    pub fn suggestion(&self) -> Option<Suggestion> {
        if !matches!(self.mode, Mode::Edit) || self.focused_row() != Row::Name {
            return None;
        }
        let (name, _) = self.candidates.get(self.candidate)?;
        let typed = self.input(Row::Name).as_str().chars().count();
        Some(Suggestion {
            name: name.clone(),
            completion: name.chars().skip(typed).collect(),
            index: self.candidate,
            total: self.candidates.len(),
        })
    }

    /// Recompute the candidates for the name typed so far. The highlight goes
    /// back to the best match on every edit, so cycling never outlives the
    /// prefix it was cycling through.
    fn update_candidates(&mut self) {
        let typed = self.input(Row::Name).as_str().to_string();
        self.candidates = self
            .contacts
            .matches(&typed)
            .into_iter()
            .map(|(name, iban)| (name.to_string(), iban.to_string()))
            .collect();
        self.candidate = 0;
        self.fill_iban_from_history();
    }

    /// Bring in the IBAN that goes with the name as soon as it is unambiguous:
    /// the name is one we know, or only one remembered name still matches.
    fn fill_iban_from_history(&mut self) {
        if !self.iban_from_history && !self.input(Row::Iban).is_empty() {
            return;
        }
        let typed = self.input(Row::Name).as_str();
        let iban = self
            .contacts
            .iban_for(typed)
            .or(match self.candidates.as_slice() {
                [(_, only)] => Some(only.as_str()),
                _ => None,
            })
            .map(str::to_string);

        match iban {
            Some(iban) if self.input(Row::Iban).as_str() != iban => {
                self.input_mut(Row::Iban).set(iban);
                self.iban_from_history = true;
            }
            Some(_) => self.iban_from_history = true,
            // The name no longer names anyone we know: take back what we filled in.
            None if self.iban_from_history => {
                self.input_mut(Row::Iban).clear();
                self.iban_from_history = false;
            }
            None => {}
        }
    }

    fn cycle_candidate(&mut self, delta: isize) {
        if self.candidates.len() < 2 {
            return;
        }
        let len = self.candidates.len() as isize;
        self.candidate = (self.candidate as isize + delta).rem_euclid(len) as usize;
    }

    /// Take the offered recipient: their full name and their IBAN.
    fn accept_suggestion(&mut self) {
        let Some((name, iban)) = self.candidates.get(self.candidate).cloned() else {
            return;
        };
        let name_field = self.input_mut(Row::Name);
        name_field.set(name.clone());
        name_field.end();
        self.input_mut(Row::Iban).set(iban);
        self.iban_from_history = true;
        self.candidates.clear();
        self.candidate = 0;
        self.recompute();
        self.set_status(
            StatusKind::Info,
            format!("Filled in {name} from the history."),
        );
    }

    /// Add the recipient of the payment now on screen to the history file.
    /// Called when a code is actually produced -- written to a file, or on the
    /// way out with a valid form -- never on every keystroke.
    pub fn remember_recipient(&mut self) -> Remembered {
        let Ok(resolved) = self.payment().resolve() else {
            return Remembered::Unchanged;
        };
        let (Some(name), Some(iban)) =
            (resolved.get(Field::CreditorName), resolved.get(Field::Iban))
        else {
            return Remembered::Unchanged;
        };
        let (name, iban) = (name.to_string(), iban.to_string());
        if !self.contacts.remember(&name, &iban) {
            return Remembered::Unchanged;
        }
        // With nowhere to write it -- no home directory -- the recipient still
        // completes for the rest of this session, quietly.
        let Some(path) = self.contacts_path.clone() else {
            return Remembered::Unchanged;
        };
        match self.contacts.save(&path) {
            Ok(()) => Remembered::Added(name),
            Err(error) => Remembered::Failed(format!("{error:#}")),
        }
    }

    /// Drop the recipient on screen from the history, for a name stored by
    /// mistake or an IBAN that has since changed.
    fn forget_recipient(&mut self) {
        let name = self.input(Row::Name).as_str().to_string();
        if !self.contacts.forget(&name) {
            self.set_status(StatusKind::Info, format!("{name} is not in the history."));
            return;
        }
        let Some(path) = self.contacts_path.clone() else {
            return;
        };
        match self.contacts.save(&path) {
            Ok(()) => {
                // The name matches nobody now, so `update_candidates` also
                // takes back the IBAN this entry had filled in.
                self.update_candidates();
                self.set_status(StatusKind::Success, format!("Forgot {name}."));
            }
            Err(error) => self.set_status(StatusKind::Error, format!("{error:#}")),
        }
    }

    fn set_status(&mut self, kind: StatusKind, text: impl Into<String>) {
        self.status = Status {
            text: text.into(),
            kind,
        };
    }

    // -- navigation ------------------------------------------------------

    fn step_focus(&mut self, delta: isize) {
        let len = ROWS.len() as isize;
        let mut next = self.focus as isize;
        for _ in 0..ROWS.len() {
            next = (next + delta).rem_euclid(len);
            if !self.is_disabled(ROWS[next as usize]) {
                break;
            }
        }
        self.focus = next as usize;
    }

    fn step_preset(&mut self, delta: isize) {
        let at = PRESETS.iter().position(|p| *p == self.preset).unwrap_or(0) as isize;
        let next = (at + delta).rem_euclid(PRESETS.len() as isize) as usize;
        self.preset = PRESETS[next];
        // The new type may omit whatever is focused right now.
        if self.is_disabled(self.focused_row()) {
            self.step_focus(1);
        }
        self.recompute();
    }

    // -- symbol synchronisation -----------------------------------------

    /// Mirror of the payme.sk form: the symbols drive the reference, and a
    /// reference typed by hand is parsed back into the symbols.
    fn sync_reference_from_symbols(&mut self) {
        let reference = reference::build(&self.symbols());
        self.input_mut(Row::Reference).set(reference);
    }

    fn sync_symbols_from_reference(&mut self) {
        let parsed = reference::parse(self.input(Row::Reference).as_str());
        self.input_mut(Row::Vs).set(parsed.vs);
        self.input_mut(Row::Ss).set(parsed.ss);
        self.input_mut(Row::Ks).set(parsed.ks);
    }

    fn after_edit(&mut self, row: Row) {
        match row {
            Row::Vs | Row::Ks | Row::Ss => self.sync_reference_from_symbols(),
            Row::Reference => self.sync_symbols_from_reference(),
            Row::Name => self.update_candidates(),
            // An IBAN typed over an offered one is the user's own from then on.
            Row::Iban => self.iban_from_history = false,
            _ => {}
        }
        self.recompute();
    }

    // -- key handling ----------------------------------------------------

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        if let Mode::Save { .. } = self.mode {
            self.on_key_save(key);
            return;
        }
        self.on_key_edit(key);
    }

    fn on_key_edit(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let row = self.focused_row();
        let completing = row == Row::Name && !self.candidates.is_empty();

        match key.code {
            KeyCode::Esc => self.quit(),
            KeyCode::Char('q' | 'c') if ctrl => self.quit(),
            KeyCode::Char('s') if ctrl => self.begin_save(SaveKind::Png),
            KeyCode::Char('e') if ctrl => self.begin_save(SaveKind::Svg),
            KeyCode::Char('p') if ctrl => {
                self.style = self.style.toggled();
                self.set_status(StatusKind::Info, format!("QR drawing: {:?}", self.style));
            }
            KeyCode::Char('n') if ctrl => {
                self.normalize = !self.normalize;
                let state = if self.normalize { "on" } else { "off" };
                self.set_status(StatusKind::Info, format!("Text normalisation {state}"));
                self.recompute();
            }
            KeyCode::Char('r') if ctrl => {
                let outcome = self.remember_recipient();
                self.report_remembered(outcome, true);
            }
            KeyCode::Char('d') if ctrl => self.forget_recipient(),
            // Accept the offered recipient, the way a shell accepts its own
            // suggestion: Ctrl+F, or Right once there is nothing left to move over.
            KeyCode::Char('f') if ctrl && completing => self.accept_suggestion(),
            KeyCode::Right if completing && self.input(Row::Name).at_end() => {
                self.accept_suggestion()
            }
            // With more than one recipient behind the prefix, the arrows walk
            // the candidates the way a completion list is walked; Tab and Enter
            // still leave the row. A single candidate keeps the plain
            // behaviour, so navigation is only ever taken over when it earns it.
            KeyCode::Down | KeyCode::Up if completing && self.candidates.len() > 1 => {
                let delta = if key.code == KeyCode::Down { 1 } else { -1 };
                self.cycle_candidate(delta);
            }
            KeyCode::Char('u') if ctrl => {
                self.input_mut(row).clear();
                self.after_edit(row);
            }
            KeyCode::Char('w') if ctrl => {
                self.input_mut(row).delete_word();
                self.after_edit(row);
            }
            KeyCode::Tab | KeyCode::Down => self.step_focus(1),
            KeyCode::BackTab | KeyCode::Up => self.step_focus(-1),
            // On the name row a pending suggestion is taken first, so the
            // recipient is complete before the focus moves on to the IBAN.
            KeyCode::Enter if completing => self.accept_suggestion(),
            KeyCode::Enter => self.step_focus(1),
            KeyCode::Left if row == Row::Preset => self.step_preset(-1),
            KeyCode::Right if row == Row::Preset => self.step_preset(1),
            KeyCode::Left => self.input_mut(row).left(),
            KeyCode::Right => self.input_mut(row).right(),
            KeyCode::Home => self.input_mut(row).home(),
            KeyCode::End => self.input_mut(row).end(),
            KeyCode::Backspace => {
                self.input_mut(row).backspace();
                self.after_edit(row);
            }
            KeyCode::Delete => {
                self.input_mut(row).delete();
                self.after_edit(row);
            }
            KeyCode::Char(c) if !ctrl => {
                self.input_mut(row).insert(c, row.accepts(), row.max_len());
                self.after_edit(row);
            }
            _ => {}
        }
    }

    /// Leaving with a valid payment counts as having generated it, so the
    /// recipient is kept for next time.
    fn quit(&mut self) {
        self.remember_recipient();
        self.should_quit = true;
    }

    /// Show what happened to the history. `announce` also reports the quiet
    /// cases, for the explicit Ctrl+R; a failure is always worth saying.
    fn report_remembered(&mut self, outcome: Remembered, announce: bool) {
        match outcome {
            Remembered::Added(name) if announce => {
                self.set_status(StatusKind::Success, format!("Remembered {name}."))
            }
            Remembered::Unchanged if announce => {
                let text = match self.matrix() {
                    Some(_) => "Already in the history.",
                    None => "Fill in the form before remembering the recipient.",
                };
                self.set_status(StatusKind::Info, text)
            }
            Remembered::Failed(error) => self.set_status(
                StatusKind::Error,
                format!("Could not save the recipient history: {error}"),
            ),
            _ => {}
        }
    }

    fn begin_save(&mut self, kind: SaveKind) {
        if self.matrix().is_none() {
            self.set_status(StatusKind::Error, "Fill in the form before saving.");
            return;
        }
        self.mode = Mode::Save {
            kind,
            path: TextField::new(kind.default_path()),
        };
    }

    fn on_key_save(&mut self, key: KeyEvent) {
        let Mode::Save { kind, path } = &mut self.mode else {
            return;
        };
        let kind = *kind;

        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Edit;
                self.set_status(StatusKind::Info, "Save cancelled.");
            }
            KeyCode::Enter => {
                let target = PathBuf::from(path.as_str().trim());
                self.mode = Mode::Edit;
                self.write(kind, &target);
            }
            KeyCode::Backspace => path.backspace(),
            KeyCode::Delete => path.delete(),
            KeyCode::Left => path.left(),
            KeyCode::Right => path.right(),
            KeyCode::Home => path.home(),
            KeyCode::End => path.end(),
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => path.clear(),
            KeyCode::Char(c) => path.insert(c, Accepts::Any, 255),
            _ => {}
        }
    }

    /// Scale chosen so a saved symbol is comfortably scannable from a screen.
    const SAVE_SCALE: u32 = 8;

    fn write(&mut self, kind: SaveKind, path: &std::path::Path) {
        if path.as_os_str().is_empty() {
            self.set_status(StatusKind::Error, "Enter a file name.");
            return;
        }
        let Some(matrix) = self.matrix().cloned() else {
            self.set_status(StatusKind::Error, "Fill in the form before saving.");
            return;
        };

        let result = match kind {
            SaveKind::Png => qr::write_png(&matrix, path, Self::SAVE_SCALE),
            SaveKind::Svg => qr::write_svg(&matrix, path, Self::SAVE_SCALE),
        };

        match result {
            Ok(()) => {
                // The code exists now, so its recipient is worth keeping.
                let remembered = self.remember_recipient();
                let note = match &remembered {
                    Remembered::Added(name) => format!(" \u{25aa} remembered {name}"),
                    _ => String::new(),
                };
                self.set_status(
                    StatusKind::Success,
                    format!("Saved {} to {}{note}", kind.label(), path.display()),
                );
                self.report_remembered(remembered, false);
            }
            Err(error) => self.set_status(StatusKind::Error, format!("{error:#}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn type_str(app: &mut App, text: &str) {
        for c in text.chars() {
            app.on_key(key(KeyCode::Char(c)));
        }
    }

    /// A form over an empty history, so the tests never touch the user's own.
    fn blank() -> App {
        App::with_history(Contacts::default(), None)
    }

    fn remembering(contacts: Contacts) -> (App, PathBuf) {
        let dir = std::env::temp_dir().join("payme-qr-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("history-{}.yml", std::process::id()));
        let _ = std::fs::remove_file(&path);
        (App::with_history(contacts, Some(path.clone())), path)
    }

    fn known() -> Contacts {
        let mut contacts = Contacts::default();
        contacts.remember("Alice Payee", "SK6807200002891987426353");
        contacts.remember("Alan Turing", "SK3709008482989234185969");
        contacts
    }

    fn filled() -> App {
        let mut app = blank();
        app.focus = Row::Iban.index();
        type_str(&mut app, "SK6807200002891987426353");
        app.focus = Row::Name.index();
        type_str(&mut app, "Alice Payee");
        app
    }

    #[test]
    fn a_filled_form_produces_the_spec_link() {
        let app = filled();
        assert_eq!(
            app.url(),
            Some("https://payme.sk/2/p/PME?IBAN=SK6807200002891987426353&CN=Alice+Payee")
        );
        assert!(app.matrix().is_some());
    }

    #[test]
    fn typing_symbols_fills_the_reference() {
        let mut app = filled();
        app.focus = Row::Vs.index();
        type_str(&mut app, "2546874464");
        assert_eq!(app.input(Row::Reference).as_str(), "/VS2546874464/SS/KS");

        app.focus = Row::Ks.index();
        type_str(&mut app, "1118");
        assert_eq!(
            app.input(Row::Reference).as_str(),
            "/VS2546874464/SS/KS1118"
        );
        assert!(app
            .url()
            .unwrap()
            .contains("PI=%2FVS2546874464%2FSS%2FKS1118"));
    }

    #[test]
    fn typing_a_reference_backfills_the_symbols() {
        let mut app = filled();
        app.focus = Row::Reference.index();
        type_str(&mut app, "/VS123/SS456/KS0308");
        assert_eq!(app.input(Row::Vs).as_str(), "123");
        assert_eq!(app.input(Row::Ss).as_str(), "456");
        assert_eq!(app.input(Row::Ks).as_str(), "0308");
    }

    #[test]
    fn free_text_reference_clears_the_symbols() {
        let mut app = filled();
        app.focus = Row::Vs.index();
        type_str(&mut app, "42");
        app.focus = Row::Reference.index();
        app.on_key(ctrl('u'));
        type_str(&mut app, "QR-ab29e346");
        assert!(app.symbols().is_empty());
        assert!(app.url().unwrap().contains("PI=QR-ab29e346"));
    }

    #[test]
    fn focus_skips_attributes_the_type_omits() {
        let mut app = filled();
        app.preset = Preset::Donation;
        app.focus = Row::Currency.index();
        app.step_focus(1);
        assert_eq!(
            app.focused_row(),
            Row::Reference,
            "the due date row is skipped"
        );
    }

    #[test]
    fn changing_the_preset_moves_focus_off_an_omitted_row() {
        let mut app = filled();
        app.focus = Row::DueDate.index();
        app.focus = Row::Preset.index();
        app.on_key(key(KeyCode::Right)); // P2p -> Invoice
        assert_eq!(app.preset, Preset::Invoice);
        app.focus = Row::DueDate.index();
        app.focus = Row::Preset.index();
        app.on_key(key(KeyCode::Right)); // Invoice -> Store, which omits the due date
        assert_eq!(app.preset, Preset::Store);
    }

    #[test]
    fn the_due_date_is_dropped_when_the_type_omits_it() {
        let mut app = filled();
        app.focus = Row::DueDate.index();
        type_str(&mut app, "2028-04-30");
        assert!(app.url().unwrap().contains("DT=20280430"));

        app.preset = Preset::Donation;
        app.recompute();
        assert!(!app.url().unwrap().contains("DT="));
    }

    #[test]
    fn missing_mandatory_attributes_are_reported_on_their_row() {
        let mut app = blank();
        app.preset = Preset::Eshop;
        app.recompute();
        assert!(!app.errors_for(Row::Iban).is_empty());
        assert!(!app.errors_for(Row::Name).is_empty());
        assert!(!app.errors_for(Row::Amount).is_empty());
        assert!(app.matrix().is_none());
    }

    #[test]
    fn amount_input_rejects_a_second_separator() {
        let mut app = filled();
        app.focus = Row::Amount.index();
        type_str(&mut app, "8.5.9");
        assert_eq!(app.input(Row::Amount).as_str(), "8.59");
        assert!(app.url().unwrap().contains("AM=8.59&CC=EUR"));
    }

    #[test]
    fn saving_is_refused_until_the_form_is_valid() {
        let mut app = blank();
        app.on_key(ctrl('s'));
        assert_eq!(app.mode, Mode::Edit);
        assert_eq!(app.status.kind, StatusKind::Error);

        let mut app = filled();
        app.on_key(ctrl('s'));
        assert!(matches!(
            app.mode,
            Mode::Save {
                kind: SaveKind::Png,
                ..
            }
        ));
        app.on_key(key(KeyCode::Esc));
        assert_eq!(app.mode, Mode::Edit);
        assert!(
            !app.should_quit,
            "Esc closes the dialog rather than the app"
        );
    }

    #[test]
    fn saving_writes_the_file_at_the_typed_path() {
        let dir = std::env::temp_dir().join("payme-qr-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("saved.svg");
        let _ = std::fs::remove_file(&target);

        let mut app = filled();
        app.on_key(ctrl('e'));
        if let Mode::Save { path, .. } = &mut app.mode {
            path.clear();
        }
        type_str(&mut app, target.to_str().unwrap());
        app.on_key(key(KeyCode::Enter));

        assert_eq!(app.status.kind, StatusKind::Success, "{}", app.status.text);
        assert!(std::fs::read_to_string(&target).unwrap().contains("<svg"));
        let _ = std::fs::remove_file(&target);
    }

    #[test]
    fn normalisation_can_be_toggled() {
        let mut app = filled();
        app.focus = Row::Message.index();
        type_str(&mut app, "Ko\u{161}ice");
        assert!(app.url().unwrap().contains("MSG=Kosice"));

        app.on_key(ctrl('n'));
        assert!(!app.normalize);
        assert!(!app.errors_for(Row::Message).is_empty());
    }

    #[test]
    fn escape_quits_the_form() {
        let mut app = filled();
        app.on_key(key(KeyCode::Esc));
        assert!(app.should_quit);
    }

    #[test]
    fn a_remembered_name_completes_as_it_is_typed() {
        let mut app = App::with_history(known(), None);
        type_str(&mut app, "Ali");

        let suggestion = app.suggestion().expect("Alice Payee is offered");
        assert_eq!(suggestion.name, "Alice Payee");
        assert_eq!(suggestion.completion, "ce Payee");
        assert_eq!(suggestion.total, 1);

        // Right at the end of the line takes it, the way a shell would.
        app.on_key(key(KeyCode::Right));
        assert_eq!(app.input(Row::Name).as_str(), "Alice Payee");
        assert_eq!(
            app.input(Row::Iban).as_str(),
            "SK6807200002891987426353",
            "the IBAN comes with the name"
        );
        assert!(app.suggestion().is_none(), "nothing is left to complete");
        assert!(app.url().unwrap().ends_with("CN=Alice+Payee"));
    }

    #[test]
    fn several_recipients_share_a_prefix_and_can_be_cycled() {
        let mut app = App::with_history(known(), None);
        type_str(&mut app, "Al");
        let suggestion = app.suggestion().unwrap();
        assert_eq!(
            (suggestion.name.as_str(), suggestion.total),
            ("Alan Turing", 2)
        );
        assert!(
            app.input(Row::Iban).is_empty(),
            "an ambiguous prefix fills in no IBAN"
        );

        app.on_key(key(KeyCode::Down));
        assert_eq!(app.suggestion().unwrap().name, "Alice Payee");

        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.input(Row::Name).as_str(), "Alice Payee");
        assert_eq!(app.input(Row::Iban).as_str(), "SK6807200002891987426353");
    }

    #[test]
    fn an_ibanless_name_typed_in_full_still_finds_its_iban() {
        let mut app = App::with_history(known(), None);
        type_str(&mut app, "alice payee");
        assert_eq!(app.input(Row::Iban).as_str(), "SK6807200002891987426353");
        assert!(app.iban_from_history());
    }

    #[test]
    fn an_iban_typed_by_hand_is_never_overwritten() {
        let mut app = App::with_history(known(), None);
        app.focus = Row::Iban.index();
        type_str(&mut app, "SK2256002926649607918409");
        app.focus = Row::Name.index();
        type_str(&mut app, "Alice Payee");
        assert_eq!(app.input(Row::Iban).as_str(), "SK2256002926649607918409");
        assert!(!app.iban_from_history());
    }

    #[test]
    fn a_filled_in_iban_is_taken_back_when_the_name_stops_matching() {
        let mut app = App::with_history(known(), None);
        type_str(&mut app, "Alice Payee");
        assert!(!app.input(Row::Iban).is_empty());
        app.on_key(ctrl('u'));
        type_str(&mut app, "Someone Else");
        assert!(
            app.input(Row::Iban).is_empty(),
            "an offered IBAN does not outlive the name that brought it"
        );
    }

    #[test]
    fn saving_a_code_remembers_its_recipient() {
        let (mut app, history) = remembering(Contacts::default());
        app.focus = Row::Iban.index();
        type_str(&mut app, "SK6807200002891987426353");
        app.focus = Row::Name.index();
        type_str(&mut app, "Alice Payee");

        let target = std::env::temp_dir().join("payme-qr-tests/remembered.svg");
        app.on_key(ctrl('e'));
        if let Mode::Save { path, .. } = &mut app.mode {
            path.clear();
        }
        type_str(&mut app, target.to_str().unwrap());
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.status.kind, StatusKind::Success, "{}", app.status.text);

        let stored = Contacts::load(&history).unwrap();
        assert_eq!(
            stored.iban_for("Alice Payee"),
            Some("SK6807200002891987426353")
        );
        let _ = std::fs::remove_file(&target);
        let _ = std::fs::remove_file(&history);
    }

    #[test]
    fn quitting_on_a_valid_form_remembers_the_recipient() {
        let (mut app, history) = remembering(Contacts::default());
        app.focus = Row::Iban.index();
        type_str(&mut app, "SK6807200002891987426353");
        app.focus = Row::Name.index();
        type_str(&mut app, "Kov\u{e1}\u{10d}");
        app.on_key(key(KeyCode::Esc));

        assert!(app.should_quit);
        let stored = Contacts::load(&history).unwrap();
        assert_eq!(
            stored.iban_for("Kovac"),
            Some("SK6807200002891987426353"),
            "the name is stored as it is transmitted, folded to the Annex A set"
        );
        let _ = std::fs::remove_file(&history);
    }

    #[test]
    fn an_incomplete_form_leaves_the_history_alone() {
        let (mut app, history) = remembering(Contacts::default());
        type_str(&mut app, "Alice Payee");
        app.on_key(ctrl('r'));
        assert_eq!(app.status.kind, StatusKind::Info);
        app.on_key(key(KeyCode::Esc));
        assert!(
            !history.exists(),
            "no file is written for a payment that never was"
        );
    }

    #[test]
    fn a_recipient_can_be_forgotten_again() {
        let (mut app, history) = remembering(known());
        type_str(&mut app, "Alice Payee");
        app.on_key(ctrl('d'));

        assert_eq!(app.status.kind, StatusKind::Success, "{}", app.status.text);
        assert!(app.input(Row::Iban).is_empty());
        assert_eq!(
            Contacts::load(&history).unwrap().iban_for("Alice Payee"),
            None
        );
        let _ = std::fs::remove_file(&history);
    }
}
