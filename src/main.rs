//! `vocab` — the binary. Wires the real dependencies to [`App`] and runs the
//! event loop.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use ratatui::crossterm::event::{self, Event};
use vocab::{App, Dictionary, Store};

fn main() -> Result<()> {
    let data_directory = data_directory()?;
    let store = Store::open(&data_directory.join("vocab.db"))?;
    let dictionary = Dictionary::unpack_into(&data_directory)?;
    let mut app = App::new(store, dictionary)?;

    // Restores the terminal to what was on it before, however the loop ends —
    // including on a panic.
    let mut terminal = ratatui::init();
    let outcome = run(&mut app, &mut terminal);
    ratatui::restore();
    outcome
}

fn run(app: &mut App, terminal: &mut ratatui::DefaultTerminal) -> Result<()> {
    while app.is_running() {
        terminal.draw(|frame| app.draw(frame))?;

        // Poll rather than block so a resize repaints promptly.
        if event::poll(Duration::from_millis(250))?
            && let Event::Key(key) = event::read()?
        {
            app.handle_key(key)?;
        }
    }
    Ok(())
}

/// User data lives in the OS application-data directory, never the working
/// directory, so `vocab` behaves the same from any folder.
fn data_directory() -> Result<PathBuf> {
    let directories = directories::ProjectDirs::from("", "", "vocab")
        .context("locating your application-data directory")?;
    Ok(directories.data_dir().to_path_buf())
}
