# payme-qr

Offline generator of **PayMe payment links and QR payment codes**, implementing the Slovak
Banking Association's *Payment Link Standard, version 2.0* (2026-01-01) — the standard behind
[payme.sk](https://www.payme.sk). Nothing is sent anywhere: the link and the QR symbol are
built locally.

Two ways to use it: an interactive terminal form, or flags for scripts and invoicing.

## Build

```sh
cargo build --release      # binary at target/release/payme-qr
```

## Interactive form

```sh
cargo run
```

The form shows the payment on the left and the live QR code plus the generated link on the
right. Mandatory fields carry a `*`; which fields are mandatory depends on the payment type,
and attributes that the chosen type does not carry are marked *not sent*.

| Key | |
|---|---|
| `Tab` / `Shift+Tab`, `↑` / `↓` | move between fields |
| `←` / `→` | choose the payment type (on the first row), otherwise move the cursor |
| `Ctrl+S` / `Ctrl+E` | save the QR code as PNG / SVG |
| `Ctrl+P` | switch between half-block and full-block drawing |
| `Ctrl+N` | toggle folding of national characters to ASCII |
| `Ctrl+U` / `Ctrl+W` | clear the field / delete a word |
| `→` / `Ctrl+F` | accept the suggested recipient (on the name row) |
| `↑` / `↓` | pick another recipient, while several share what you typed |
| `Ctrl+R` / `Ctrl+D` | remember the recipient now / forget them |
| `Esc` | quit |

### Remembered recipients

Recipients are kept between sessions, so a name is typed once and afterwards completes itself
along with its IBAN:

```yaml
# ~/.config/payme-qr/contacts.yml
Alice Payee: SK6807200002891987426353
"The Best e-shops ltd": SK3709008482989234185969
```

Type into the name row and the first remembered name that starts with it is offered greyed
out after the cursor; `→` or `Ctrl+F` takes it and brings the IBAN with it, and `↑`/`↓` walk
the other candidates when several names share the prefix (`2/4` on the right counts them). An
IBAN that arrived this way is labelled *remembered* and is replaced freely as the name
changes; an IBAN you typed yourself is never touched.

A recipient is written to the file when a code is actually produced — the QR saved as PNG or
SVG, or the form closed on a valid payment — never on every keystroke. `Ctrl+R` stores the
current one immediately and `Ctrl+D` removes it again. The name stored is the one transmitted
in the link, so it is already folded to the Annex A character set.

The file location is `$PAYME_QR_CONTACTS`, else `$XDG_CONFIG_HOME/payme-qr/contacts.yml`, else
`~/.config/payme-qr/contacts.yml`. It is a plain map of name to IBAN, meant to be edited by
hand: comments, blank lines and quoted or unquoted scalars are all understood, and a line that
cannot be read is skipped rather than treated as an error.

Variable, constant and specific symbols stay in sync with the reference field, exactly as on
payme.sk: fill in any symbol and the reference becomes `/VS…/SS…/KS…`; type a reference by
hand and the symbols are read back out of it.

## Command line

```sh
# person-to-person: prints the link and the QR code
payme-qr --iban SK6807200002891987426353 --name 'Alice Payee' --amount 8.59

# an invoice with Slovak payment symbols, saved as PNG
payme-qr --preset eshop --iban SK6807200002891987426353 --name 'The Best e-shops ltd' \
         --amount 200.30 --vs 2546874464 --ss 2019568456 --ks 1118 \
         --msg 'my e-shop, Kosice' --png invoice.png

# a static donation code
payme-qr --preset donation --iban SK6807200002891987426353 --name 'Hope charity' --svg donate.svg

# the same payee again: the IBAN comes from the recipient history
payme-qr --name 'Alice Payee' --amount 12.00 --msg 'Coffee'
```

`--preset` picks the payment context (`p2p`, `invoice`, `store`, `eshop`, `donation`), or use
`--type` for the raw path component (`p`, `m`, `e`, `q`). Other flags: `--currency`, `--due`,
`--ref`, `--scale`, `--print-link`, `--print-qr`, `--blocks`, `--no-normalize`; `--help` lists
them all. The command line shares the recipient history with the form: every code produced
adds its payee, `--iban` may then be left out for a name already stored, `--list-contacts`
prints what is remembered, and `--contacts FILE` / `--no-remember` choose another file or keep
this run out of it. The exit code is `0` on success and `2` when the payment does not satisfy the
standard, with every problem listed on stderr.

## What the standard requires

The link is `https://payme.sk/2/{type}/PME?IBAN=…&AM=…&CC=…&DT=…&PI=…&MSG=…&CN=…`.

| | `/m/` POI | `/e/` e-commerce | `/q/` static QR | `/p/` person-to-person |
|---|---|---|---|---|
| IBAN, Creditor's name | mandatory | mandatory | mandatory | mandatory |
| Amount, Currency, Payment identification | mandatory | mandatory | optional | optional |
| Due date | not sent | not sent | not sent | optional |
| Message | optional | optional | optional | optional |

Also implemented: the ISO 13616 IBAN checksum plus the Slovak bank-code and mod-11 account
checks (a 20-digit Slovak BBAN is accepted and converted), the `/VS{0,10}/SS{0,10}/KS{0,4}`
reference encoding and its slash rules, the Annex A character set with Slovak diacritics
folded to ASCII, and QR symbols at error correction level M with a four-module quiet zone.

Where the standard leaves a detail open — attribute order, whether a space becomes `+` or
`%20` — the behaviour of the generator on payme.sk is reproduced, so links from this tool are
byte-identical to the ones the website produces.

## Tests

```sh
cargo test
```

Covers every example link printed in the standard, the requirement matrix per payment type,
IBAN and reference validation, character normalisation, the recipient history (its file
format, the completion, and what is written when), and a round trip that renders a PNG and
decodes it back to the same link at error correction level M.
