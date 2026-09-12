//! Drawing the form, the QR preview and the dialogs.

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::qr::{half_block_rows, Matrix, TerminalStyle};
use crate::spec::Requirement;
use crate::tui::app::{App, Mode, Outcome, Row, StatusKind, ROWS};

/// True black and white, so the symbol stays scannable whatever the terminal
/// theme maps its ANSI colours to.
const QR_DARK: Color = Color::Rgb(0, 0, 0);
const QR_LIGHT: Color = Color::Rgb(255, 255, 255);

const LABEL_WIDTH: u16 = 17;

pub fn draw(frame: &mut Frame, app: &App) {
    let [body, status, footer] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    let [form_area, preview_area] =
        Layout::horizontal([Constraint::Length(58), Constraint::Min(24)]).areas(body);

    let [fields_area, issues_area] = Layout::vertical([
        Constraint::Length(ROWS.len() as u16 + 2),
        Constraint::Min(3),
    ])
    .areas(form_area);

    draw_fields(frame, app, fields_area);
    draw_issues(frame, app, issues_area);
    draw_preview(frame, app, preview_area);
    draw_status(frame, app, status);
    draw_footer(frame, app, footer);

    if let Mode::Save { kind, path } = &app.mode {
        let area = centered(frame.area(), 60, 3);
        frame.render_widget(Clear, area);
        let block = Block::bordered().title(format!(" Save {} as ", kind_label(*kind)));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(Paragraph::new(path.as_str()), inner);
        frame.set_cursor_position((
            inner.x
                + path
                    .cursor_col()
                    .min(inner.width.saturating_sub(1) as usize) as u16,
            inner.y,
        ));
    }
}

fn kind_label(kind: crate::tui::app::SaveKind) -> &'static str {
    match kind {
        crate::tui::app::SaveKind::Png => "PNG",
        crate::tui::app::SaveKind::Svg => "SVG",
    }
}

fn draw_fields(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::bordered().title(" Payment ".bold());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    for (index, row) in ROWS.iter().enumerate() {
        let y = inner.y + index as u16;
        if y >= inner.y + inner.height {
            break;
        }
        let focused = app.focus == index && matches!(app.mode, Mode::Edit);
        let disabled = app.is_disabled(*row);
        let has_error = !app.errors_for(*row).is_empty();

        let mut label_style = Style::default();
        if disabled {
            label_style = label_style.add_modifier(Modifier::DIM);
        } else if has_error {
            label_style = label_style.fg(Color::Red);
        } else if focused {
            label_style = label_style.fg(Color::Cyan).add_modifier(Modifier::BOLD);
        }

        let mandatory = app.requirement(*row) == Some(Requirement::Mandatory);
        let label = format!("{}{}", row.label(), if mandatory { " *" } else { "" });
        let label_area = Rect {
            x: inner.x,
            y,
            width: LABEL_WIDTH.min(inner.width),
            height: 1,
        };
        frame.render_widget(Paragraph::new(Span::styled(label, label_style)), label_area);

        let value_area = Rect {
            x: inner.x + LABEL_WIDTH,
            y,
            width: inner.width.saturating_sub(LABEL_WIDTH),
            height: 1,
        };
        if value_area.width == 0 {
            continue;
        }

        if *row == Row::Preset {
            let arrows = if focused {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().dim()
            };
            let line = Line::from(vec![
                Span::styled("\u{25c2} ", arrows),
                Span::styled(
                    app.preset.label(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(" \u{25b8}", arrows),
                Span::styled(
                    format!("  /{}/", app.preset.payment_type()),
                    Style::default().dim(),
                ),
            ]);
            frame.render_widget(Paragraph::new(line), value_area);
            if focused {
                frame.set_cursor_position((value_area.x, y));
            }
            continue;
        }

        let field = app.input(*row);
        let width = value_area.width as usize;
        // Scroll the value horizontally so the cursor stays visible.
        let offset = field.cursor_col().saturating_sub(width.saturating_sub(1));
        let visible: String = field.as_str().chars().skip(offset).take(width).collect();

        let value_style = if disabled {
            Style::default().dim()
        } else {
            Style::default()
        };
        // A remembered recipient is offered inline, greyed out after the
        // cursor, and the IBAN it brought with it is marked as such.
        let suggestion = (*row == Row::Name).then(|| app.suggestion()).flatten();
        let remembered_iban = *row == Row::Iban && !focused && app.iban_from_history();

        let line = if disabled {
            // The attribute is not transmitted for this type; say so rather
            // than leaving a value on screen that never reaches the link.
            let note = format!("not sent in /{}/ links", app.preset.payment_type());
            if field.is_empty() {
                Line::from(Span::styled(note, value_style))
            } else {
                Line::from(vec![
                    Span::styled(visible, value_style.add_modifier(Modifier::CROSSED_OUT)),
                    Span::styled(format!("  {note}"), value_style),
                ])
            }
        } else if let Some(suggestion) = suggestion {
            let mut spans = vec![
                Span::styled(visible, value_style),
                Span::styled(suggestion.completion, Style::default().dim()),
            ];
            if suggestion.total > 1 {
                spans.push(Span::styled(
                    format!(
                        "  {}/{} \u{2191}\u{2193}",
                        suggestion.index + 1,
                        suggestion.total
                    ),
                    Style::default().fg(Color::Cyan).dim(),
                ));
            }
            Line::from(spans)
        } else if remembered_iban {
            Line::from(vec![
                Span::styled(visible, value_style),
                Span::styled("  remembered", Style::default().dim()),
            ])
        } else if field.is_empty() && focused {
            Line::from(Span::styled(row.hint(), Style::default().dim()))
        } else {
            Line::from(Span::styled(visible, value_style))
        };
        frame.render_widget(Paragraph::new(line), value_area);

        if focused {
            frame.set_cursor_position((value_area.x + (field.cursor_col() - offset) as u16, y));
        }
    }
}

fn draw_issues(frame: &mut Frame, app: &App, area: Rect) {
    let lines: Vec<Line> = match &app.outcome {
        Outcome::Invalid(errors) if !errors.is_empty() => errors
            .iter()
            .map(|error| {
                Line::from(vec![
                    Span::styled("\u{2022} ", Style::default().fg(Color::Red)),
                    Span::styled(
                        format!("{}: ", error.field.label()),
                        Style::default().bold(),
                    ),
                    Span::raw(error.message.clone()),
                ])
            })
            .collect(),
        Outcome::Invalid(_) => vec![Line::from(
            "Fill in the mandatory fields marked with *.".dim(),
        )],
        Outcome::Ready { .. } => {
            let mut lines = vec![Line::from("\u{2713} Valid payment link.".fg(Color::Green))];
            for note in normalisation_notes(app) {
                lines.push(Line::from(note.dim()));
            }
            lines
        }
    };

    let block = Block::bordered().title(" Status ".bold());
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }).block(block),
        area,
    );
}

/// Report free text that had to change to fit the recommended character set.
fn normalisation_notes(app: &App) -> Vec<String> {
    let Outcome::Ready { .. } = app.outcome else {
        return Vec::new();
    };
    let payment = app.payment();
    let Ok(resolved) = payment.resolve() else {
        return Vec::new();
    };

    [
        (crate::Field::CreditorName, payment.creditor_name.clone()),
        (crate::Field::Message, payment.message.clone()),
        (crate::Field::PaymentId, payment.reference.clone()),
    ]
    .into_iter()
    .filter_map(|(field, original)| {
        let sent = resolved.get(field).unwrap_or_default();
        (!original.is_empty() && sent != original.trim())
            .then(|| format!("{} is sent as \"{sent}\"", field.label()))
    })
    .collect()
}

fn draw_preview(frame: &mut Frame, app: &App, area: Rect) {
    let [qr_area, link_area] =
        Layout::vertical([Constraint::Min(3), Constraint::Length(6)]).areas(area);

    let block = Block::bordered().title(" QR payment code \u{2013} EC level M ".bold());
    let inner = block.inner(qr_area);
    frame.render_widget(block, qr_area);

    match app.matrix() {
        Some(matrix) => draw_matrix(frame, matrix, app.style, inner),
        None => frame.render_widget(
            Paragraph::new("The QR code appears once the payment is complete.".dim())
                .wrap(Wrap { trim: true })
                .alignment(Alignment::Center),
            inner,
        ),
    }

    let link_block = Block::bordered().title(" Payment link ".bold());
    let text = match app.url() {
        Some(url) => Paragraph::new(url).wrap(Wrap { trim: false }),
        None => Paragraph::new("\u{2014}".dim()),
    };
    frame.render_widget(text.block(link_block), link_area);
}

fn draw_matrix(frame: &mut Frame, matrix: &Matrix, style: TerminalStyle, area: Rect) {
    let (cols, rows) = style.cell_size(matrix);
    if cols > area.width as usize || rows > area.height as usize {
        let message = format!(
            "Not enough room for the QR code: it needs {cols}x{rows} cells, this pane has {}x{}.\n\
             Enlarge the terminal, or press Ctrl+P for the other drawing style.",
            area.width, area.height
        );
        frame.render_widget(
            Paragraph::new(message)
                .wrap(Wrap { trim: true })
                .fg(Color::Yellow),
            area,
        );
        return;
    }

    // Centre the symbol in the pane.
    let target = Rect {
        x: area.x + (area.width - cols as u16) / 2,
        y: area.y + (area.height.saturating_sub(rows as u16)) / 2,
        width: cols as u16,
        height: rows as u16,
    };

    let lines: Vec<Line> = match style {
        TerminalStyle::HalfBlocks => half_block_rows(matrix)
            .into_iter()
            .map(|row| {
                Line::from(
                    row.into_iter()
                        .map(|(top, bottom)| {
                            Span::styled(
                                "\u{2580}",
                                Style::default()
                                    .fg(if top { QR_DARK } else { QR_LIGHT })
                                    .bg(if bottom { QR_DARK } else { QR_LIGHT }),
                            )
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect(),
        TerminalStyle::FullBlocks => (0..matrix.size)
            .map(|y| {
                Line::from(
                    (0..matrix.size)
                        .map(|x| {
                            let color = if matrix.is_dark(x, y) {
                                QR_DARK
                            } else {
                                QR_LIGHT
                            };
                            Span::styled("  ", Style::default().bg(color))
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect(),
    };

    frame.render_widget(Paragraph::new(lines), target);
}

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let color = match app.status.kind {
        StatusKind::Info => Color::Cyan,
        StatusKind::Success => Color::Green,
        StatusKind::Error => Color::Red,
    };
    let text = if app.status.text.is_empty() {
        Span::styled(app.focused_row().hint(), Style::default().dim())
    } else {
        Span::styled(app.status.text.clone(), Style::default().fg(color))
    };
    frame.render_widget(Paragraph::new(Line::from(text)), area);
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    // On the name row the completion keys matter more than the drawing ones.
    let keys = if app.focused_row() == Row::Name && app.suggestion().is_some() {
        "\u{2192}/Ctrl+F accept  \u{2191}\u{2193} other recipient  Ctrl+D forget  \
         Ctrl+S PNG  Ctrl+E SVG  Esc quit"
            .to_string()
    } else {
        let normalize = if app.normalize { "on" } else { "off" };
        format!(
            "Tab move  \u{2190}\u{2192} edit/choose  Ctrl+S PNG  Ctrl+E SVG  Ctrl+R remember  \
             Ctrl+N normalise ({normalize})  Ctrl+U clear  Esc quit"
        )
    };
    frame.render_widget(Paragraph::new(keys.dim()), area);
}

/// A `width` x `height` rectangle in the middle of `area`.
fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}
