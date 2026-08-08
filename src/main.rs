//! `vocab` — the binary. Wires the real dependencies to [`App`] and runs the
//! event loop.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use ratatui::crossterm::event::{self, Event};
use tokio::runtime::Builder;
use vocab::{Anki, App, Cards, Config, DeepSeek, Dictionary, Notes, Store};

fn main() -> Result<()> {
    let data_directory = data_directory()?;
    let store = Store::open(&data_directory.join("vocab.db"))?;
    let dictionary = Dictionary::unpack_into(&data_directory)?;
    let config = Config::load_or_create(&config_directory()?.join("config.toml"))?;

    // One worker thread is plenty: Notes are written one per capture, cards go
    // a handful at a time, and the point of the runtime is only that the reader
    // never waits on either.
    let runtime = Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .thread_name("vocab-background")
        .build()?;
    // Both HTTP clients want a runtime to register themselves with. The
    // DeepSeek key comes from the environment — its absence turns Notes off for
    // the run rather than erroring on every capture.
    let (notes, cards) = {
        let _inside = runtime.enter();
        (
            Notes::new(DeepSeek::from_environment(), runtime.handle().clone()),
            Cards::new(Anki::on_this_machine()?, runtime.handle().clone()),
        )
    };
    let mut app = App::new(store, dictionary, notes, cards, config)?;

    // Restores the terminal to what was on it before, however the loop ends —
    // including on a panic.
    let mut terminal = ratatui::init();
    let outcome = run(&mut app, &mut terminal);

    let leaving = leave(&mut app, &mut terminal, &runtime);

    ratatui::restore();

    // Said after the alternate screen has gone, because that is the only place
    // the reader can still read it once the tool has left.
    if let Some(farewell) = app.farewell() {
        println!("{farewell}");
    }
    outcome.and(leaving)
}

fn run(app: &mut App, terminal: &mut ratatui::DefaultTerminal) -> Result<()> {
    while app.is_running() {
        // Whatever the background writer finished since the last pass, so a
        // Note appears on the screen the reader is already looking at.
        app.collect_notes()?;
        app.collect_synced()?;
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

/// Push what changed on the way out.
///
/// Bounded, so that Anki — closed, wedged, or slow — can never be the reason
/// the reader is still here. The screen is drawn once before the wait so that
/// it is something to watch rather than a frozen frame.
fn leave(
    app: &mut App,
    terminal: &mut ratatui::DefaultTerminal,
    runtime: &tokio::runtime::Runtime,
) -> Result<()> {
    app.start_leaving()?;
    terminal.draw(|frame| app.draw(frame))?;
    runtime.block_on(app.settle_sync())
}

/// User data lives in the OS application-data directory, never the working
/// directory, so `vocab` behaves the same from any folder.
fn data_directory() -> Result<PathBuf> {
    Ok(project_directories()?.data_dir().to_path_buf())
}

/// The one file the reader edits, kept where their OS keeps such things rather
/// than beside the database they never open.
fn config_directory() -> Result<PathBuf> {
    Ok(project_directories()?.config_dir().to_path_buf())
}

fn project_directories() -> Result<directories::ProjectDirs> {
    directories::ProjectDirs::from("", "", "vocab")
        .context("locating your application-data directory")
}
