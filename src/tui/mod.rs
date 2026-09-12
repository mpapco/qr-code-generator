//! The interactive form.

pub mod app;
pub mod field;
pub mod ui;

use std::time::Duration;

use anyhow::Result;
use ratatui::crossterm::event::{self, Event};

pub use app::App;

/// Run the form until the user quits.
pub fn run() -> Result<()> {
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal);
    // `ratatui::init` also installs a panic hook that restores the terminal.
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal) -> Result<()> {
    let mut app = App::new();

    while !app.should_quit {
        terminal.draw(|frame| ui::draw(frame, &app))?;

        // Poll rather than block, so a terminal resize redraws promptly.
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) => app.on_key(key),
            Event::Resize(_, _) => {}
            _ => {}
        }
    }
    Ok(())
}
