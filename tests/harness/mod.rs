//! The one seam.
//!
//! The application is constructed with its dependencies injected, driven with
//! synthetic key events, and rendered through Ratatui's `TestBackend` into an
//! in-memory buffer that the test asserts against. Tests read what a reader
//! would see; none of them call into the application's internals or inspect
//! database rows.
//!
//! Of the four injected dependencies, only one is faked: the user database is a
//! real SQLite file in a temporary directory, fresh per test, and the dictionary
//! is the real bundled WordNet, which is read-only and deterministic. Only the
//! `NoteWriter` is a stub, because the alternative is the network.

#![allow(dead_code)] // Each integration test file uses a different subset.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use tempfile::TempDir;
use tokio::runtime::Runtime;
use vocab::notes::{BoxedNote, NoteRequest, NoteWriter, Notes, Unwritten};
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

/// How the stub behaves when it is asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Answering {
    /// Answers, every time.
    Always,
    /// Answers, but each answer takes longer than the one after it — so an
    /// earlier answer always lands last, which is the ordering in which a stale
    /// answer could overwrite the Note that replaced it.
    Slowly,
    /// Answers, and the answer is no.
    Refusing,
    /// Cannot be reached at all, as on a train.
    Unreachable,
}

/// How many turns the slowest answer dawdles for. Anything above the number of
/// Notes a test asks for keeps the ordering strictly reversed.
const DAWDLE: usize = 8;

/// The one fake: a `NoteWriter` that never leaves the process.
///
/// The Note it returns echoes back the Word, the Book, and the sentence it was
/// handed, so a test can tell from the screen alone that all three reached the
/// writer. It is numbered, so a rewritten Note is distinguishable from the one
/// it replaced.
pub struct StubWriter {
    attempts: AtomicUsize,
    answering: Mutex<Answering>,
}

impl StubWriter {
    fn new(answering: Answering) -> Self {
        Self {
            attempts: AtomicUsize::new(0),
            answering: Mutex::new(answering),
        }
    }

    /// Let a writer that has been refusing or unreachable start answering, so a
    /// test can watch a queue drain.
    fn recover(&self) {
        *self.answering.lock().expect("the stub's mode") = Answering::Always;
    }
}

impl NoteWriter for StubWriter {
    fn write(&self, request: NoteRequest) -> BoxedNote {
        let attempt = self.attempts.fetch_add(1, Ordering::SeqCst) + 1;
        let answering = *self.answering.lock().expect("the stub's mode");
        Box::pin(async move {
            if answering == Answering::Slowly {
                for _ in 0..DAWDLE.saturating_sub(attempt) {
                    tokio::task::yield_now().await;
                }
            }
            match answering {
                Answering::Refusing => Err(Unwritten::Refused(anyhow::anyhow!(
                    "this stub was asked to refuse"
                ))),
                Answering::Unreachable => Err(Unwritten::Unreachable(anyhow::anyhow!(
                    "this stub cannot be reached"
                ))),
                Answering::Always | Answering::Slowly => Ok(format!(
                    "reading {attempt} of {:?} from {}: {}",
                    request.spelling, request.book_name, request.sentence
                )),
            }
        })
    }
}

pub struct Harness {
    app: App,
    terminal: Terminal<TestBackend>,
    directory: TempDir,
    /// Single-threaded and driven only by [`Harness::settle`], so a Note is
    /// written exactly when a test says so and never before.
    runtime: Runtime,
    writer: Option<Arc<StubWriter>>,
}

impl Harness {
    /// Notes on, and the writer answers every time.
    pub fn new() -> Self {
        Self::writing(Some(StubWriter::new(Answering::Always)))
    }

    /// Notes on, but the writer answers and the answer is no.
    pub fn failing() -> Self {
        Self::writing(Some(StubWriter::new(Answering::Refusing)))
    }

    /// Notes on, but there is nothing to reach — reading on a train.
    pub fn offline() -> Self {
        Self::writing(Some(StubWriter::new(Answering::Unreachable)))
    }

    /// Notes on, but each answer overtakes the one asked before it.
    pub fn dawdling() -> Self {
        Self::writing(Some(StubWriter::new(Answering::Slowly)))
    }

    /// No writer at all, as when the API key is missing.
    pub fn without_notes() -> Self {
        Self::writing(None)
    }

    fn writing(writer: Option<StubWriter>) -> Self {
        let directory = tempfile::tempdir().expect("creating a temporary directory");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("building the runtime");
        Self::open_in(directory, runtime, writer.map(Arc::new))
    }

    fn open_in(directory: TempDir, runtime: Runtime, writer: Option<Arc<StubWriter>>) -> Self {
        let store = Store::open(&directory.path().join("vocab.db")).expect("opening the store");
        let notes = Notes::new(
            writer.clone().map(|writer| writer as Arc<dyn NoteWriter>),
            runtime.handle().clone(),
        );
        let app = App::new(store, dictionary(), notes).expect("constructing the app");
        let terminal =
            Terminal::new(TestBackend::new(WIDTH, HEIGHT)).expect("constructing the terminal");
        Self {
            app,
            terminal,
            directory,
            runtime,
            writer,
        }
    }

    /// Close the tool and open it again against the same database, as a reader
    /// would between reading sessions. The writer carries over, so a test can
    /// tell whether it was asked for the same Note twice.
    pub fn restart(self) -> Self {
        let Self {
            directory,
            runtime,
            writer,
            ..
        } = self;
        Self::open_in(directory, runtime, writer)
    }

    // -- Driving ----------------------------------------------------------

    /// Drive every Note in flight to completion and apply what came back.
    ///
    /// This is what keeps the asynchronous half of the tool testable through
    /// the same screen seam as everything else: no sleeping, no polling, and no
    /// second seam onto the Note pipeline.
    pub fn settle(&mut self) -> &mut Self {
        let Self { app, runtime, .. } = self;
        runtime
            .block_on(app.settle_notes())
            .expect("settling the Notes in flight");
        self
    }

    /// Let a failing writer start answering.
    pub fn notes_recover(&mut self) -> &mut Self {
        self.writer
            .as_ref()
            .expect("a stub writer to recover")
            .recover();
        self
    }

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
        // The real loop looks for finished Notes on every pass; nothing is
        // waited on, so an unsettled Note stays pending.
        self.app.collect_notes().expect("collecting finished Notes");
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
        let (Some(above), Some(below)) = (
            flattened.find(&flatten(first)),
            flattened.find(&flatten(second)),
        ) else {
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
