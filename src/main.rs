//! `vocab` — the binary. Wires the real dependencies to [`App`] and runs the
//! event loop.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use ratatui::crossterm::event::{self, Event};
use tokio::runtime::Builder;
use vocab::{App, Dictionary, Notes, Store};

fn main() -> Result<()> {
    let data_directory = data_directory()?;
    let store = Store::open(&data_directory.join("vocab.db"))?;
    let dictionary = Dictionary::unpack_into(&data_directory)?;

    // One worker thread is plenty: Notes are written one per capture, and the
    // point of the runtime is only that the reader never waits on one.
    let runtime = Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .thread_name("vocab-notes")
        .build()?;
    let notes = Notes::new(None, runtime.handle().clone());
    let mut app = App::new(store, dictionary, notes)?;

    // Restores the terminal to what was on it before, however the loop ends —
    // including on a panic.
    let mut terminal = ratatui::init();
    let outcome = run(&mut app, &mut terminal);
    ratatui::restore();
    outcome
}

fn run(app: &mut App, terminal: &mut ratatui::DefaultTerminal) -> Result<()> {
    while app.is_running() {
        // Whatever the background writer finished since the last pass, so a
        // Note appears on the screen the reader is already looking at.
        app.collect_notes()?;
        terminal.draw(|frame| app.draw(frame))?;

        // Poll rather than block so a resize repaints promptly — and so a Note
        // that lands while nothing is being typed is still picked up.
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
