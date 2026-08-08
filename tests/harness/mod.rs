//! The one seam.
//!
//! The application is constructed with its dependencies injected, driven with
//! synthetic key events, and rendered through Ratatui's `TestBackend` into an
//! in-memory buffer that the test asserts against. Tests read what a reader
//! would see; none of them call into the application's internals or inspect
//! database rows.
//!
//! Of the three injected dependencies, only fakes-in-waiting would be faked:
//! the user database is a real SQLite file in a temporary directory, fresh per
//! test, and the dictionary is the real bundled WordNet, which is read-only and
//! deterministic.

#![allow(dead_code)] // Each integration test file uses a different subset.

use std::path::PathBuf;
use std::sync::OnceLock;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use tempfile::TempDir;
use vocab::{App, Dictionary, Store};

const WIDTH: u16 = 100;
const HEIGHT: u16 = 40;

/// The bundled dictionary, unpacked once for the whole test binary.
///
/// It is read-only, so every test can share one copy rather than paying to
/// write twelve megabytes each.
fn dictionary() -> Dictionary {
    static UNPACKED: OnceLock<PathBuf> = OnceLock::new();
    let directory = UNPACKED.get_or_init(|| {
        let directory = std::env::temp_dir().join("vocab-tests-dictionary");
        Dictionary::unpack_into(&directory).expect("unpacking the bundled dictionary");
        directory
    });
    Dictionary::open(&directory.join("wordnet.db")).expect("opening the bundled dictionary")
}

pub struct Harness {
    app: App,
    terminal: Terminal<TestBackend>,
    directory: TempDir,
}

impl Harness {
    pub fn new() -> Self {
        let directory = tempfile::tempdir().expect("creating a temporary directory");
        Self::open_in(directory)
    }

    fn open_in(directory: TempDir) -> Self {
        let store = Store::open(&directory.path().join("vocab.db")).expect("opening the store");
        let app = App::new(store, dictionary()).expect("constructing the app");
        let terminal =
            Terminal::new(TestBackend::new(WIDTH, HEIGHT)).expect("constructing the terminal");
        Self {
            app,
            terminal,
            directory,
        }
    }

    /// Close the tool and open it again against the same database, as a reader
    /// would between reading sessions.
    pub fn restart(self) -> Self {
        let Self { directory, .. } = self;
        Self::open_in(directory)
    }

    // -- Driving ----------------------------------------------------------

    pub fn type_text(&mut self, text: &str) -> &mut Self {
        for character in text.chars() {
            self.key(KeyCode::Char(character));
        }
        self
    }

    pub fn press(&mut self, code: KeyCode) -> &mut Self {
        self.key(code);
        self
    }

    pub fn enter(&mut self) -> &mut Self {
        self.key(KeyCode::Enter)
    }

    /// Type a whole line and submit it.
    pub fn submit(&mut self, text: &str) -> &mut Self {
        self.type_text(text).enter()
    }

    fn key(&mut self, code: KeyCode) -> &mut Self {
        let event = KeyEvent::new(code, KeyModifiers::NONE);
        let event = KeyEvent {
            kind: KeyEventKind::Press,
            ..event
        };
        self.app.handle_key(event).expect("handling a key event");
        self
    }

    pub fn is_running(&self) -> bool {
        self.app.is_running()
    }

    // -- Asserting --------------------------------------------------------

    /// Everything currently rendered, one line per terminal row.
    pub fn screen(&mut self) -> String {
        let app = &self.app;
        self.terminal
            .draw(|frame| app.draw(frame))
            .expect("drawing the frame");

        let buffer = self.terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Just the input line — the bottom row, where what you type and the
    /// argument hint after it are drawn.
    pub fn input_line(&mut self) -> String {
        let screen = self.screen();
        screen.lines().last().unwrap_or_default().trim().to_string()
    }

    pub fn assert_shows(&mut self, expected: &str) -> &mut Self {
        let screen = self.screen();
        assert!(
            flatten(&screen).contains(&flatten(expected)),
            "expected the screen to show {expected:?}\n\n--- screen ---\n{screen}\n--------------"
        );
        self
    }

    pub fn assert_does_not_show(&mut self, unexpected: &str) -> &mut Self {
        let screen = self.screen();
        assert!(
            !flatten(&screen).contains(&flatten(unexpected)),
            "expected the screen not to show {unexpected:?}\n\n--- screen ---\n{screen}\n--------------"
        );
        self
    }

    /// How many times `needle` appears, for asserting a Word is listed once.
    pub fn count_of(&mut self, needle: &str) -> usize {
        flatten(&self.screen()).matches(&flatten(needle)).count()
    }

    /// Assert both are on screen and that `first` is above `second`.
    pub fn assert_shows_in_order(&mut self, first: &str, second: &str) -> &mut Self {
        let screen = self.screen();
        let flattened = flatten(&screen);
        let (Some(above), Some(below)) =
            (flattened.find(&flatten(first)), flattened.find(&flatten(second)))
        else {
            panic!(
                "expected the screen to show both {first:?} and {second:?}\
                 \n\n--- screen ---\n{screen}\n--------------"
            );
        };
        assert!(
            above < below,
            "expected {first:?} to come before {second:?}\
             \n\n--- screen ---\n{screen}\n--------------"
        );
        self
    }
}

/// Collapse the screen to a single whitespace-normalised line.
///
/// Rendered text is padded to the terminal width and long sentences wrap, so an
/// assertion on what a reader sees should not depend on where the line breaks
/// fell.
fn flatten(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
