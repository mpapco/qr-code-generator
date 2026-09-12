//! Non-interactive mode: build a link and write the QR code from flags.

use std::path::PathBuf;

use anyhow::{Result};
use clap::Parser;

use crate::contacts::Contacts;
use crate::payment::Payment;
use crate::qr::{self, TerminalStyle};
use crate::reference::{self, Symbols};
use crate::spec::{PaymentType, Preset, CURRENCY_EUR};
use crate::{link, Field};

/// Exit code used when the payment does not satisfy the standard.
pub const EXIT_INVALID: i32 = 2;

#[derive(Parser, Debug)]
#[command(
    name = "payme-qr",
    version,
    about = "Generate PayMe payment links and QR codes offline (SBA Payment Link Standard 2.0)",
    long_about = "Generate PayMe payment links and QR codes offline.\n\n\
                  Run without arguments to open the interactive terminal UI.",
    after_help = "EXAMPLES:\n  \
        payme-qr --iban SK6807200002891987426353 --name 'Alice Payee' --amount 8.59\n  \
        payme-qr --preset eshop --iban SK68... --name 'The Best e-shops ltd' \\\n    \
            --amount 200.30 --vs 2546874464 --ss 2019568456 --ks 1118 --png invoice.png\n  \
        payme-qr --preset donation --iban SK68... --name 'Hope charity' --svg donate.svg"
)]
pub struct Args {
    /// Form preset: p2p, invoice, store, eshop or donation
    #[arg(long, value_name = "PRESET")]
    pub preset: Option<Preset>,

    /// Type path component, if you would rather pick it directly: p, m, e or q
    #[arg(long = "type", value_name = "TYPE", conflicts_with = "preset")]
    pub payment_type: Option<PaymentType>,

    /// Recipient IBAN (a 20-digit Slovak BBAN is also accepted). Optional when
    /// the name is one that has been remembered from an earlier code.
    #[arg(long, value_name = "IBAN")]
    pub iban: Option<String>,

    /// Recipient name (creditor's name, CN)
    #[arg(long, value_name = "NAME", required_unless_present = "list_contacts")]
    pub name: Option<String>,

    /// Amount, up to 9 digits or 7 digits with 1-2 decimals
    #[arg(long, value_name = "AMOUNT")]
    pub amount: Option<String>,

    /// Currency code; version 2 of the standard only supports EUR
    #[arg(long, default_value = CURRENCY_EUR, value_name = "CCY")]
    pub currency: String,

    /// Due date, YYYY-MM-DD; only carried by /p/ links
    #[arg(long, value_name = "DATE")]
    pub due: Option<String>,

    /// Payment identification (PI) as free text
    #[arg(long = "ref", value_name = "TEXT", conflicts_with_all = ["vs", "ss", "ks"])]
    pub reference: Option<String>,

    /// Variable symbol, up to 10 digits
    #[arg(long, value_name = "DIGITS")]
    pub vs: Option<String>,

    /// Specific symbol, up to 10 digits
    #[arg(long, value_name = "DIGITS")]
    pub ss: Option<String>,

    /// Constant symbol, up to 4 digits
    #[arg(long, value_name = "DIGITS")]
    pub ks: Option<String>,

    /// Message for the recipient (MSG)
    #[arg(long, value_name = "TEXT")]
    pub msg: Option<String>,

    /// Write the QR code to this PNG file
    #[arg(long, value_name = "FILE")]
    pub png: Option<PathBuf>,

    /// Write the QR code to this SVG file
    #[arg(long, value_name = "FILE")]
    pub svg: Option<PathBuf>,

    /// Pixels (PNG) or units (SVG) per QR module
    #[arg(long, default_value_t = 8, value_name = "N")]
    pub scale: u32,

    /// Print the payment link to stdout
    #[arg(long)]
    pub print_link: bool,

    /// Print the QR code to stdout
    #[arg(long)]
    pub print_qr: bool,

    /// Draw the printed QR code with two characters per module
    #[arg(long)]
    pub blocks: bool,

    /// Keep national characters instead of folding them to the Annex A set
    #[arg(long)]
    pub no_normalize: bool,

    /// Recipient history file; defaults to $XDG_CONFIG_HOME/payme-qr/contacts.yml
    #[arg(long, value_name = "FILE")]
    pub contacts: Option<PathBuf>,

    /// Do not add this recipient to the history file
    #[arg(long)]
    pub no_remember: bool,

    /// List the remembered recipients and exit
    #[arg(long, conflicts_with_all = ["iban", "name"])]
    pub list_contacts: bool,
}

impl Args {
    /// Which preset the flags select. `--type` picks the plainest preset for
    /// that type path component.
    fn preset(&self) -> Preset {
        if let Some(preset) = self.preset {
            return preset;
        }
        match self.payment_type {
            Some(PaymentType::M) => Preset::Store,
            Some(PaymentType::E) => Preset::Eshop,
            Some(PaymentType::Q) => Preset::Donation,
            Some(PaymentType::P) | None => Preset::P2p,
        }
    }

    fn symbols(&self) -> Symbols {
        Symbols {
            vs: reference::digits_only(self.vs.as_deref().unwrap_or_default()),
            ss: reference::digits_only(self.ss.as_deref().unwrap_or_default()),
            ks: reference::digits_only(self.ks.as_deref().unwrap_or_default()),
        }
    }

    /// Check the symbol lengths, which the standard fixes per symbol (SS 3.4.5).
    fn validate_symbols(&self) -> Result<()> {
        let symbols = self.symbols();
        for (name, value, max) in [
            ("--vs", &symbols.vs, 10),
            ("--ss", &symbols.ss, 10),
            ("--ks", &symbols.ks, 4),
        ] {
            if value.len() > max {
                anyhow::bail!("{name} takes at most {max} digits");
            }
        }
        Ok(())
    }

    /// The history file this run reads and writes.
    fn contacts_path(&self) -> Option<PathBuf> {
        self.contacts.clone().or_else(Contacts::default_path)
    }

    pub fn to_payment(&self, contacts: &Contacts) -> Result<Payment> {
        self.validate_symbols()?;
        let symbols = self.symbols();
        let reference = if symbols.is_empty() {
            self.reference.clone().unwrap_or_default()
        } else {
            reference::build(&symbols)
        };

        let name = self.name.clone().unwrap_or_default();
        // An omitted IBAN is looked up under the recipient's name, so a payee
        // paid once can be paid again by name alone.
        let iban = match &self.iban {
            Some(iban) => iban.clone(),
            None => contacts
                .iban_for(&name)
                .map(str::to_string)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "no IBAN given and \"{name}\" is not in the recipient history; \
                         pass --iban once and it will be remembered"
                    )
                })?,
        };

        Ok(Payment {
            preset: self.preset(),
            iban,
            creditor_name: name,
            amount: self.amount.clone().unwrap_or_default(),
            currency: self.currency.clone(),
            due_date: self.due.clone().unwrap_or_default(),
            reference,
            message: self.msg.clone().unwrap_or_default(),
            normalize_text: !self.no_normalize,
        })
    }

    fn style(&self) -> TerminalStyle {
        if self.blocks {
            TerminalStyle::FullBlocks
        } else {
            TerminalStyle::HalfBlocks
        }
    }

    /// With no output selected, show the link and the code.
    fn nothing_requested(&self) -> bool {
        !self.print_link && !self.print_qr && self.png.is_none() && self.svg.is_none()
    }
}

/// Run the non-interactive mode. Returns the process exit code.
pub fn run(args: &Args) -> Result<i32> {
    let path = args.contacts_path();
    let mut contacts = match &path {
        Some(path) => Contacts::load(path)?,
        None => Contacts::default(),
    };

    if args.list_contacts {
        for (name, iban) in contacts.iter() {
            println!("{name}\t{iban}");
        }
        return Ok(0);
    }

    // Input that breaks the standard exits with EXIT_INVALID, whether it is a
    // flag the standard limits or a payment that does not add up.
    let payment = match args.to_payment(&contacts) {
        Ok(payment) => payment,
        Err(error) => {
            eprintln!("error: {error:#}");
            return Ok(EXIT_INVALID);
        }
    };

    let resolved = match payment.resolve() {
        Ok(resolved) => resolved,
        Err(errors) => {
            eprintln!("The payment does not satisfy the Payment Link Standard:");
            for error in errors {
                eprintln!("  {error}");
            }
            return Ok(EXIT_INVALID);
        }
    };

    let url = link::build(&resolved);
    let matrix = qr::encode(&url)?;

    if let Some(path) = &args.png {
        qr::write_png(&matrix, path, args.scale)?;
        eprintln!("Wrote {}", path.display());
    }
    if let Some(path) = &args.svg {
        qr::write_svg(&matrix, path, args.scale)?;
        eprintln!("Wrote {}", path.display());
    }

    if args.print_qr || args.nothing_requested() {
        print!("{}", qr::to_ansi(&matrix, args.style()));
    }
    if args.print_link || args.nothing_requested() {
        println!("{url}");
    }

    // The code was produced, so keep its recipient for next time.
    if !args.no_remember {
        if let (Some(path), Some(name), Some(iban)) = (
            &path,
            resolved.get(Field::CreditorName),
            resolved.get(Field::Iban),
        ) {
            if contacts.remember(name, iban) {
                contacts.save(path)?;
                eprintln!("note: remembered {name}");
            }
        }
    }

    // Point out anything the standard had us change, so nothing is silent.
    if payment.normalize_text {
        for (field, original) in [
            (Field::CreditorName, &payment.creditor_name),
            (Field::Message, &payment.message),
            (Field::PaymentId, &payment.reference),
        ] {
            let transmitted = resolved.get(field).unwrap_or_default();
            if !original.is_empty() && transmitted != original.trim() {
                eprintln!("note: {} sent as \"{transmitted}\"", field.label());
            }
        }
    }

    Ok(0)
}
