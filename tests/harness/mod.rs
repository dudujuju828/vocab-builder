//! The one seam.
//!
//! The application is constructed with its dependencies injected, driven with
//! synthetic key events, and rendered through Ratatui's `TestBackend` into an
//! in-memory buffer that the test asserts against. Tests read what a reader
//! would see; none of them call into the application's internals or inspect
//! database rows.
//!
//! Of the injected dependencies, only two are faked: the user database is a
//! real SQLite file in a temporary directory, fresh per test, and the dictionary
//! is the real bundled WordNet, which is read-only and deterministic. The
//! `NoteWriter` and the `CardSync` are stubs, because the alternative to each is
//! the network.
//!
//! The card payload is the one thing here that a reader never sees on screen, so
//! it is read back off the `CardSync` stub rather than off the buffer.

#![allow(dead_code)] // Each integration test file uses a different subset.

use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use tempfile::TempDir;
use tokio::runtime::Runtime;
use vocab::cards::{BoxedPush, Card, CardSync, Cards, Unpushed};
use vocab::config::DEFAULT_DECK;
use vocab::notes::{BoxedNote, NoteRequest, NoteWriter, Notes, Unwritten};
use vocab::{App, Config, Dictionary, Store};

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

/// How the stub writer behaves when it is asked.
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

/// How the fake Anki behaves when it is handed a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Anki {
    /// Running, and takes what it is given.
    Open,
    /// Not running — the ordinary state of the machine, and not an error.
    Shut,
    /// Running, and says no.
    Refusing,
    /// Takes the question and never answers it, as a wedged Anki would.
    Hanging,
}

/// How many turns the slowest answer dawdles for. Anything above the number of
/// Notes a test asks for keeps the ordering strictly reversed.
const DAWDLE: usize = 8;

/// The first fake: a `NoteWriter` that never leaves the process.
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

/// The second fake: a `CardSync` that never leaves the process.
///
/// It keeps every card it accepted, so a test can read what the tool tried to
/// write — the Definition, the sentences, the Books, the dates and the Notes —
/// which is the one part of the tool that has no expression on screen.
pub struct StubAnki {
    behaving: Mutex<Anki>,
    written: Mutex<Vec<Card>>,
    /// Stands in for the identifiers Anki hands back. Arbitrary, except that
    /// each is different from the last.
    next_note_id: AtomicI64,
}

impl StubAnki {
    fn new(behaving: Anki) -> Self {
        Self {
            behaving: Mutex::new(behaving),
            written: Mutex::new(Vec::new()),
            next_note_id: AtomicI64::new(1_700_000_000_000),
        }
    }

    /// Let a closed or refusing Anki start taking cards, so a test can watch a
    /// queue drain into it.
    fn opens(&self) {
        *self.behaving.lock().expect("the fake's mode") = Anki::Open;
    }

    fn cards(&self) -> Vec<Card> {
        self.written.lock().expect("the fake's record").clone()
    }
}

impl CardSync for StubAnki {
    fn push(&self, card: Card) -> BoxedPush {
        let behaving = *self.behaving.lock().expect("the fake's mode");

        let pushed = match behaving {
            // Never answers. Nothing is recorded, because nothing arrived.
            Anki::Hanging => return Box::pin(std::future::pending()),
            Anki::Shut => Err(Unpushed::Unavailable(anyhow::anyhow!(
                "this fake Anki isn't running"
            ))),
            Anki::Refusing => Err(Unpushed::Refused(anyhow::anyhow!(
                "this fake Anki was asked to refuse"
            ))),
            Anki::Open => {
                // A card that already names a note updates it and keeps its
                // identifier; one that doesn't is a note being created.
                let note_id = card
                    .anki_note_id
                    .unwrap_or_else(|| self.next_note_id.fetch_add(1, Ordering::SeqCst));
                self.written.lock().expect("the fake's record").push(card);
                Ok(note_id)
            }
        };
        Box::pin(async move { pushed })
    }
}

/// How one test's tool is put together.
#[derive(Debug, Clone, Copy)]
struct Setup {
    /// Absent when there is no API key and Notes are off for the run.
    answering: Option<Answering>,
    anki: Anki,
    deck: &'static str,
    sync_on_exit: bool,
    /// What the configuration file had wrong with it, if anything.
    complaint: Option<&'static str>,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            answering: Some(Answering::Always),
            anki: Anki::Open,
            deck: DEFAULT_DECK,
            sync_on_exit: true,
            complaint: None,
        }
    }
}

pub struct Harness {
    app: App,
    terminal: Terminal<TestBackend>,
    directory: TempDir,
    /// Single-threaded and driven only by [`Harness::settle`] and
    /// [`Harness::leave`], so background work happens exactly when a test says
    /// so and never before. Its clock is paused: a deadline that would hold up
    /// quitting fires as soon as nothing else can run, so proving one exists
    /// costs no test any waiting.
    runtime: Runtime,
    setup: Setup,
    writer: Option<Arc<StubWriter>>,
    anki: Arc<StubAnki>,
}

impl Harness {
    /// Notes answer every time, and Anki is running.
    pub fn new() -> Self {
        Self::set_up(Setup::default())
    }

    /// Notes on, but the writer answers and the answer is no.
    pub fn failing() -> Self {
        Self::writing(Answering::Refusing)
    }

    /// Notes on, but there is nothing to reach — reading on a train.
    pub fn offline() -> Self {
        Self::writing(Answering::Unreachable)
    }

    /// Notes on, but each answer overtakes the one asked before it.
    pub fn dawdling() -> Self {
        Self::writing(Answering::Slowly)
    }

    /// No writer at all, as when the API key is missing.
    pub fn without_notes() -> Self {
        Self::set_up(Setup {
            answering: None,
            ..Setup::default()
        })
    }

    /// Anki isn't running, which is how the machine usually is.
    pub fn anki_closed() -> Self {
        Self::against(Anki::Shut)
    }

    /// Anki is running and won't take the card.
    pub fn anki_refusing() -> Self {
        Self::against(Anki::Refusing)
    }

    /// Anki takes the card and never answers.
    pub fn anki_hanging() -> Self {
        Self::against(Anki::Hanging)
    }

    /// Cards land somewhere other than the default deck.
    pub fn with_deck(deck: &'static str) -> Self {
        Self::set_up(Setup {
            deck,
            ..Setup::default()
        })
    }

    /// Leaving doesn't push anything.
    pub fn without_sync_on_exit() -> Self {
        Self::set_up(Setup {
            sync_on_exit: false,
            ..Setup::default()
        })
    }

    /// The configuration file couldn't be used, so the defaults are in force.
    pub fn with_an_unusable_config(complaint: &'static str) -> Self {
        Self::set_up(Setup {
            complaint: Some(complaint),
            ..Setup::default()
        })
    }

    fn writing(answering: Answering) -> Self {
        Self::set_up(Setup {
            answering: Some(answering),
            ..Setup::default()
        })
    }

    fn against(anki: Anki) -> Self {
        Self::set_up(Setup {
            anki,
            ..Setup::default()
        })
    }

    fn set_up(setup: Setup) -> Self {
        let directory = tempfile::tempdir().expect("creating a temporary directory");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .start_paused(true)
            .build()
            .expect("building the runtime");
        let writer = setup
            .answering
            .map(|answering| Arc::new(StubWriter::new(answering)));
        let anki = Arc::new(StubAnki::new(setup.anki));
        Self::open_in(directory, runtime, setup, writer, anki)
    }

    fn open_in(
        directory: TempDir,
        runtime: Runtime,
        setup: Setup,
        writer: Option<Arc<StubWriter>>,
        anki: Arc<StubAnki>,
    ) -> Self {
        let store = Store::open(&directory.path().join("vocab.db")).expect("opening the store");
        let notes = Notes::new(
            writer.clone().map(|writer| writer as Arc<dyn NoteWriter>),
            runtime.handle().clone(),
        );
        let cards = Cards::new(anki.clone() as Arc<dyn CardSync>, runtime.handle().clone());
        let config = Config {
            deck: setup.deck.to_string(),
            sync_on_exit: setup.sync_on_exit,
            complaint: setup.complaint.map(str::to_string),
        };
        let app =
            App::new(store, dictionary(), notes, cards, config).expect("constructing the app");
        let terminal =
            Terminal::new(TestBackend::new(WIDTH, HEIGHT)).expect("constructing the terminal");
        Self {
            app,
            terminal,
            directory,
            runtime,
            setup,
            writer,
            anki,
        }
    }

    /// Close the tool and open it again against the same database, as a reader
    /// would between reading sessions. Both fakes carry over, so a test can tell
    /// whether the tool asked for the same thing twice.
    pub fn restart(self) -> Self {
        let Self {
            directory,
            runtime,
            setup,
            writer,
            anki,
            ..
        } = self;
        Self::open_in(directory, runtime, setup, writer, anki)
    }

    // -- Driving ----------------------------------------------------------

    /// Drive every Note and every card in flight to completion and apply what
    /// came back.
    ///
    /// This is what keeps the asynchronous half of the tool testable through
    /// the same screen seam as everything else: no sleeping, no polling, and no
    /// second seam onto the background work. Notes settle first, because a Note
    /// arriving is itself a change to the card it belongs on.
    pub fn settle(&mut self) -> &mut Self {
        let Self { app, runtime, .. } = self;
        runtime
            .block_on(app.settle_notes())
            .expect("settling the Notes in flight");
        runtime
            .block_on(app.settle_sync())
            .expect("settling the cards in flight");
        self
    }

    /// Leave, which pushes what changed on the way out.
    pub fn leave(&mut self) -> &mut Self {
        let Self { app, runtime, .. } = self;
        app.start_leaving()
            .expect("starting the sync on the way out");
        runtime
            .block_on(app.settle_sync())
            .expect("waiting on the sync on the way out");
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

    /// Let a closed or refusing Anki start taking cards.
    pub fn anki_opens(&mut self) -> &mut Self {
        self.anki.opens();
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
        // The real loop looks for finished background work on every pass;
        // nothing is waited on, so anything unsettled stays queued.
        self.app.collect_notes().expect("collecting finished Notes");
        self.app
            .collect_synced()
            .expect("collecting finished pushes");
        self
    }

    pub fn is_running(&self) -> bool {
        self.app.is_running()
    }

    // -- Asserting --------------------------------------------------------

    /// Every card the fake Anki accepted, in the order it was handed them.
    pub fn cards(&self) -> Vec<Card> {
        self.anki.cards()
    }

    /// What the tool would print once the terminal was the reader's again.
    pub fn farewell(&self) -> Option<&str> {
        self.app.farewell()
    }

    /// How many of those pushes made a new Anki note rather than updating one
    /// the tool had already been given an identifier for.
    pub fn anki_notes_created(&self) -> usize {
        self.cards()
            .iter()
            .filter(|card| card.anki_note_id.is_none())
            .count()
    }

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
