use std::process::ExitCode;

use clap::Parser;

use payme_qr::cli;
use payme_qr::tui;

fn main() -> ExitCode {
    // With no arguments at all, open the interactive form.
    let interactive = std::env::args_os().len() <= 1;

    let result = if interactive {
        tui::run().map(|()| 0)
    } else {
        cli::run(&cli::Args::parse())
    };

    match result {
        Ok(code) => ExitCode::from(code as u8),
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}
