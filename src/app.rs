//! The application: what is on screen, what the input line is collecting, and
//! what each key does.
//!
//! `App` holds no terminal of its own. It is handed its dependencies, driven
//! with key events, and rendered into a frame — which is what lets tests drive
//! the whole tool the way a reader does.

use std::time::Duration;

use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::cards::{Card, CardOutcome, Cards, Unpushed};
use crate::config::Config;
use crate::dictionary::Dictionary;
use crate::domain::{Book, Definition, Sighting, Word};
use crate::notes::{NoteOutcome, Notes, Unwritten};
use crate::search::{Search, SearchResult};
use crate::store::{CorpusWord, Store};
use crate::ui::{self, plural};

/// Said once at launch when there is no writer, and again if `/explain` is
/// asked for something that cannot happen. Never per capture.
const NOTES_ARE_OFF: &str = "Notes are off — set DEEPSEEK_API_KEY to have them written.";

/// How long leaving will wait on Anki before going anyway.
///
/// Anki is on the same machine, so a sync that is going to work is over in
/// well under this. What this bounds is the case where it isn't going to work:
/// a wedged Anki or a connection that hangs. Quitting always wins, and whatever
/// hasn't answered simply stays queued for next time.
const PATIENCE_ON_THE_WAY_OUT: Duration = Duration::from_secs(3);

/// The command surface. Listed by `/help`, and the source of the argument hints
/// shown as a command is typed.
pub struct Command {
    pub name: &'static str,
    pub argument: &'static str,
    pub help: &'static str,
}

pub const COMMANDS: &[Command] = &[
    Command {
        name: "/add",
        argument: "<word>",
        help: "capture a Word you didn't know",
    },
    Command {
        name: "/book",
        argument: "<name>",
        help: "switch the Book you are reading",
    },
    Command {
        name: "/library",
        argument: "",
        help: "list every Book you have added",
    },
    Command {
        name: "/explain",
        argument: "",
        help: "write the Note for the Sighting you are on again",
    },
    Command {
        name: "/sync",
        argument: "",
        help: "push everything that has changed to Anki",
    },
    Command {
        name: "/help",
        argument: "",
        help: "list every command",
    },
    Command {
        name: "/quit",
        argument: "[now]",
        help: "leave, syncing on the way out — \"now\" skips the sync",
    },
];

/// Each Screen wholly replaces the last, per ADR 0003.
pub enum Screen {
    Home,
    Search {
        results: Vec<SearchResult>,
        selected: usize,
    },
    Word(WordView),
    Library {
        books: Vec<Book>,
        selected: usize,
    },
    /// Deviates from the spec, which names four Screens. `/help` has to render
    /// somewhere, and the alternatives — a message line too short to hold it,
    /// or an overlay, which ADR 0003 rules out — are worse.
    Help,
}

pub struct WordView {
    pub word: Word,
    pub definitions: Vec<Definition>,
    pub sightings: Vec<Sighting>,
    /// Which Sighting `/explain` would ask about again. Sightings run most
    /// recent first, so the freshest one leads.
    pub selected: usize,
    /// Where this screen was opened from, so leaving it returns there.
    pub origin: Origin,
}

#[derive(Clone)]
pub enum Origin {
    Home,
    Search { query: String },
}

/// What the input line is collecting. Capture is a prompt sequence rather than
/// a Screen of its own.
pub enum Prompt {
    None,
    /// The Word is known; collecting the sentence it was met in.
    Sentence {
        spelling: String,
    },
    /// This Word is already held — add another Sighting for it?
    AnotherSighting {
        spelling: String,
    },
    /// This Book is not in the Library yet — add it?
    AddBook {
        name: String,
    },
}

impl Prompt {
    /// The text shown at the head of the input line.
    pub fn label(&self) -> Option<String> {
        match self {
            Self::None => None,
            Self::Sentence { .. } => Some("sentence:".to_string()),
            Self::AnotherSighting { spelling } => {
                Some(format!("add another Sighting for \"{spelling}\"? [y/n]"))
            }
            Self::AddBook { name } => {
                Some(format!("\"{name}\" is not in your Library — add it? [y/n]"))
            }
        }
    }

    /// Whether this prompt takes typed text rather than a single keypress.
    fn takes_text(&self) -> bool {
        matches!(self, Self::None | Self::Sentence { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Info,
    Warning,
}

pub struct Message {
    pub text: String,
    pub tone: Tone,
}

/// What one sync did, accumulated as the answers come back.
///
/// Held rather than derived because the pushes answer one at a time and out of
/// order, and the reader is owed one sentence about the lot of them.
#[derive(Default, Clone, Copy)]
struct Syncing {
    asked: usize,
    answered: usize,
    pushed: usize,
    /// Whether anything came back saying Anki wasn't there, which is much the
    /// commonest reason a sync doesn't happen and the one worth naming.
    anki_was_shut: bool,
}

pub struct App {
    store: Store,
    dictionary: Dictionary,
    notes: Notes,
    cards: Cards,
    config: Config,
    search: Search,
    corpus: Vec<CorpusWord>,
    /// Held rather than queried, so rendering never touches the database and a
    /// read failure cannot silently render as "no Book yet".
    current_book: Option<Book>,
    screen: Screen,
    input: String,
    prompt: Prompt,
    message: Option<Message>,
    syncing: Syncing,
    /// The last thing a sync had to say, kept apart from [`App::message`] so
    /// that what is printed on the way out is about the deck and nothing else.
    farewell: Option<String>,
    running: bool,
    /// Whether leaving should push what changed. Starts from the configuration
    /// and is turned off for this exit by `/quit now`.
    sync_on_exit: bool,
}

impl App {
    pub fn new(
        store: Store,
        dictionary: Dictionary,
        mut notes: Notes,
        cards: Cards,
        config: Config,
    ) -> Result<Self> {
        let corpus = store.corpus()?;
        let current_book = store.current_book()?;

        // Whatever was still queued when the tool last closed is picked up now.
        // Notes that failed are not: those are the reader's to retry, so that a
        // real failure stays visible rather than being quietly re-attempted on
        // every launch.
        for request in store.pending_notes()? {
            notes.enqueue(request);
        }

        Ok(Self {
            store,
            dictionary,
            search: Search::new(),
            corpus,
            current_book,
            screen: Screen::Home,
            input: String::new(),
            prompt: Prompt::None,
            message: (!notes.are_on()).then(|| Message {
                text: NOTES_ARE_OFF.to_string(),
                tone: Tone::Warning,
            }),
            notes,
            cards,
            syncing: Syncing::default(),
            farewell: None,
            running: true,
            sync_on_exit: config.sync_on_exit,
            config,
        })
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn screen(&self) -> &Screen {
        &self.screen
    }

    pub fn input(&self) -> &str {
        &self.input
    }

    pub fn prompt(&self) -> &Prompt {
        &self.prompt
    }

    pub fn message(&self) -> Option<&Message> {
        self.message.as_ref()
    }

    /// What the last sync had to say, if there was one.
    ///
    /// The alternate screen is torn down the instant the tool leaves, so a
    /// farewell drawn into it is a farewell nobody reads. This is what gets
    /// printed once the terminal is the reader's again — and it is only ever
    /// about the deck, so quitting doesn't echo whatever happened to be on the
    /// message line at the time.
    pub fn farewell(&self) -> Option<&str> {
        self.farewell.as_deref()
    }

    pub fn current_book(&self) -> Option<&Book> {
        self.current_book.as_ref()
    }

    /// Whether Notes are being written at all, so a pending one can say which
    /// kind of waiting it is doing.
    pub fn notes_are_on(&self) -> bool {
        self.notes.are_on()
    }

    pub fn draw(&self, frame: &mut Frame) {
        ui::draw(self, frame);
    }

    // -- Key handling -----------------------------------------------------

    pub fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        // Windows terminals report releases as well as presses; acting on both
        // would double every keystroke.
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }

        // Not in the spec's command surface, but a terminal tool that cannot be
        // interrupted is one that can strand a reader with a mangled terminal.
        // Routing it through the same exit restores the screen either way — and
        // an interrupt means leave now, so it skips the sync exactly as
        // `/quit now` does. Nothing is lost by that: the Words stay queued.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.sync_on_exit = false;
            self.running = false;
            return Ok(());
        }

        if !self.prompt.takes_text() {
            return self.answer_prompt(key);
        }

        match key.code {
            KeyCode::Esc => self.cancel(),
            KeyCode::Enter => return self.submit(),
            KeyCode::Backspace => {
                self.input.pop();
                self.input_changed();
            }
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Char(character) => {
                self.input.push(character);
                self.input_changed();
            }
            _ => {}
        }
        Ok(())
    }

    /// Answer a yes/no prompt. Anything that isn't yes is not taken as no —
    /// only an explicit `n` or Escape declines, so a stray key cannot discard a
    /// capture.
    fn answer_prompt(&mut self, key: KeyEvent) -> Result<()> {
        let affirmative = matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y'));
        let negative = matches!(
            key.code,
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc
        );
        if !affirmative && !negative {
            return Ok(());
        }

        match std::mem::replace(&mut self.prompt, Prompt::None) {
            Prompt::AnotherSighting { spelling } => {
                if affirmative {
                    self.prompt = Prompt::Sentence { spelling };
                } else {
                    self.inform(Tone::Info, "Left unchanged.");
                }
            }
            Prompt::AddBook { name } => {
                if affirmative {
                    let book_id = self.store.add_book(&name)?;
                    self.start_reading(book_id)?;
                } else {
                    self.inform(Tone::Info, format!("{name} was not added."));
                }
            }
            other => self.prompt = other,
        }
        Ok(())
    }

    /// Escape backs out of wherever you are: a part-finished capture first,
    /// then a Screen, then to Home.
    fn cancel(&mut self) {
        if let Prompt::Sentence { .. } = self.prompt {
            self.prompt = Prompt::None;
            self.input.clear();
            self.inform(Tone::Info, "Capture cancelled — nothing was saved.");
            return;
        }

        if let Screen::Word(view) = &self.screen {
            let origin = view.origin.clone();
            self.return_to(origin);
            return;
        }

        self.go_home();
    }

    fn submit(&mut self) -> Result<()> {
        if let Prompt::Sentence { spelling } = std::mem::replace(&mut self.prompt, Prompt::None) {
            let sentence = self.input.trim().to_string();
            if sentence.is_empty() {
                // Nothing typed yet: keep collecting rather than saving a
                // Sighting with no sentence, which is the whole point of one.
                self.prompt = Prompt::Sentence { spelling };
                self.inform(Tone::Warning, "Type the sentence you met the Word in.");
                return Ok(());
            }
            self.input.clear();
            return self.capture(&spelling, &sentence);
        }

        let input = self.input.trim().to_string();
        if input.starts_with('/') {
            self.input.clear();
            return self.run_command(&input);
        }
        if input.is_empty() {
            // Enter on a list opens what is highlighted.
            return self.pick_book();
        }

        // Plain text is a search, and Enter opens what it found.
        self.open_selected()
    }

    fn input_changed(&mut self) {
        if !self.prompt.takes_text() || matches!(self.prompt, Prompt::Sentence { .. }) {
            return;
        }
        self.message = None;

        let query = self.input.trim();
        if query.is_empty() {
            self.go_home();
            return;
        }
        if query.starts_with('/') {
            return; // A command in progress is not a search.
        }

        let results = self.search.run(&self.corpus, query);
        self.screen = Screen::Search {
            results,
            selected: 0,
        };
    }

    fn move_selection(&mut self, delta: isize) {
        let (length, selected) = match &mut self.screen {
            Screen::Search { results, selected } => (results.len(), selected),
            Screen::Library { books, selected } => (books.len(), selected),
            Screen::Word(view) => (view.sightings.len(), &mut view.selected),
            _ => return,
        };
        if length == 0 {
            return;
        }
        *selected = if delta < 0 {
            selected.saturating_sub(1)
        } else {
            (*selected + 1).min(length - 1)
        };
    }

    // -- Commands ---------------------------------------------------------

    fn run_command(&mut self, input: &str) -> Result<()> {
        let (name, argument) = match input.split_once(char::is_whitespace) {
            Some((name, argument)) => (name, argument.trim()),
            None => (input, ""),
        };

        match name {
            "/add" => self.start_capture(argument),
            "/book" => self.switch_book(argument),
            "/library" => self.show_library(),
            "/explain" => self.explain(),
            "/sync" => self.sync(),
            "/help" => {
                self.screen = Screen::Help;
                Ok(())
            }
            "/quit" => self.quit(argument),
            _ => {
                self.inform(Tone::Warning, format!("{name} isn't a command. Try /help."));
                Ok(())
            }
        }
    }

    fn start_capture(&mut self, argument: &str) -> Result<()> {
        let spelling = argument.trim();
        if spelling.is_empty() {
            self.inform(Tone::Warning, "/add takes the Word you met: /add <word>");
            return Ok(());
        }
        if spelling.split_whitespace().count() > 1 {
            self.inform(
                Tone::Warning,
                "A Word is a single word — the sentence comes next.",
            );
            return Ok(());
        }
        if self.current_book.is_none() {
            self.inform(
                Tone::Warning,
                "No Book yet — set one with /book <name> so captures have a Book to belong to.",
            );
            return Ok(());
        }

        // Meeting a Word again is the signal the tool exists to give, so say so
        // before asking for a sentence rather than after it has been typed.
        match self.store.find_word(spelling)? {
            Some(word) => {
                self.show_word(word, Origin::Home)?;
                self.prompt = Prompt::AnotherSighting {
                    spelling: spelling.to_string(),
                };
                self.inform(
                    Tone::Info,
                    format!("You already have \"{spelling}\" — here is where you met it."),
                );
            }
            None => {
                self.prompt = Prompt::Sentence {
                    spelling: spelling.to_string(),
                };
            }
        }
        Ok(())
    }

    fn capture(&mut self, spelling: &str, sentence: &str) -> Result<()> {
        let Some(book) = self.current_book.clone() else {
            self.inform(Tone::Warning, "No Book set — nothing was saved.");
            return Ok(());
        };

        let captured = self.store.capture(spelling, book.id, sentence)?;
        self.corpus = self.store.corpus()?;
        self.current_book = self.store.current_book()?;

        // Queued, not waited on: the Sighting is already safe, and the Note
        // arrives on the screen the reader is already looking at.
        self.ask_for_note(captured.sighting_id)?;

        let Some(word) = self.store.word(captured.word_id)? else {
            self.inform(Tone::Warning, "That Word could not be read back.");
            return Ok(());
        };
        let defined = !self.dictionary.look_up(&word.spelling)?.is_empty();
        self.show_word(word, Origin::Home)?;

        if defined {
            self.inform(Tone::Info, format!("Captured from {}.", book.name));
        } else {
            self.inform(
                Tone::Info,
                format!(
                    "Captured from {} — no Definition, but the Sighting is kept.",
                    book.name
                ),
            );
        }
        Ok(())
    }

    fn switch_book(&mut self, argument: &str) -> Result<()> {
        let name = argument.trim();
        if name.is_empty() {
            self.inform(Tone::Warning, "/book takes the Book's name: /book <name>");
            return Ok(());
        }

        match self.store.find_book(name)? {
            Some(book) => self.start_reading(book.id)?,
            None => {
                self.prompt = Prompt::AddBook {
                    name: name.to_string(),
                };
            }
        }
        Ok(())
    }

    fn show_library(&mut self) -> Result<()> {
        let books = self.store.books()?;
        // Open on whatever is being read, so picking is a confirmation rather
        // than a hunt.
        let selected = self
            .current_book
            .as_ref()
            .and_then(|current| books.iter().position(|book| book.id == current.id))
            .unwrap_or(0);
        self.screen = Screen::Library { books, selected };
        Ok(())
    }

    /// Pick the highlighted Book out of the Library and read it.
    fn pick_book(&mut self) -> Result<()> {
        let Screen::Library { books, selected } = &self.screen else {
            return Ok(());
        };
        let Some(book) = books.get(*selected) else {
            return Ok(());
        };
        let book_id = book.id;
        self.start_reading(book_id)
    }

    fn start_reading(&mut self, book_id: i64) -> Result<()> {
        self.store.set_current_book(book_id)?;
        self.current_book = self.store.current_book()?;
        let name = self
            .current_book
            .as_ref()
            .map(|book| book.name.clone())
            .unwrap_or_default();
        self.inform(Tone::Info, format!("Now reading {name}."));
        self.go_home();
        Ok(())
    }

    // -- Notes ------------------------------------------------------------

    /// Ask again about the Sighting being looked at.
    ///
    /// This is the one path that discards a Note that was written successfully:
    /// everywhere else a ready Note stays put.
    fn explain(&mut self) -> Result<()> {
        let Screen::Word(view) = &self.screen else {
            self.inform(
                Tone::Warning,
                "/explain rewrites a Sighting's Note — open a Word first.",
            );
            return Ok(());
        };
        let Some(sighting) = view.sightings.get(view.selected) else {
            self.inform(Tone::Warning, "There is no Sighting to explain.");
            return Ok(());
        };
        if !self.notes.are_on() {
            self.inform(Tone::Warning, NOTES_ARE_OFF);
            return Ok(());
        }

        let sighting_id = sighting.id;
        self.store.queue_note(sighting_id)?;
        self.ask_for_note(sighting_id)?;
        // Re-read straight away, so the Sighting shows as pending from the
        // moment it was asked about rather than once the answer lands.
        self.reread_sightings()?;
        self.inform(Tone::Info, "Asking again…");
        Ok(())
    }

    /// Put one Sighting in front of the writer.
    fn ask_for_note(&mut self, sighting_id: i64) -> Result<()> {
        if let Some(request) = self.store.note_request(sighting_id)? {
            self.notes.enqueue(request);
        }
        Ok(())
    }

    /// Take in every Note that has finished. Never waits, so the event loop can
    /// call it on every pass.
    pub fn collect_notes(&mut self) -> Result<()> {
        let finished = self.notes.collect();
        self.apply(finished)
    }

    /// Wait for every Note in flight, then take them all in.
    ///
    /// Tests drive the background work to completion through this rather than
    /// sleeping or polling.
    pub async fn settle_notes(&mut self) -> Result<()> {
        let finished = self.notes.settle().await;
        self.apply(finished)
    }

    fn apply(&mut self, finished: Vec<NoteOutcome>) -> Result<()> {
        if finished.is_empty() {
            return Ok(());
        }
        for outcome in finished {
            match outcome.note {
                Ok(note) => self.store.write_note(outcome.sighting_id, &note)?,
                // Nothing there to ask. The Sighting stays queued, and the next
                // launch with a network behind it drains the backlog — so a
                // train's worth of captures costs nothing.
                Err(Unwritten::Unreachable(_)) => {}
                // Asked, and refused. The Sighting is left exactly as it was: a
                // refusal costs the reading, never the capture.
                Err(Unwritten::Refused(_)) => self.store.fail_note(outcome.sighting_id)?,
            }
        }
        // The Note should feel like it arrived, so a Word already on screen is
        // re-read in place rather than on the reader's next visit.
        self.reread_sightings()
    }

    fn reread_sightings(&mut self) -> Result<()> {
        let Screen::Word(view) = &self.screen else {
            return Ok(());
        };
        let sightings = self.store.sightings(view.word.id)?;
        if let Screen::Word(view) = &mut self.screen {
            view.selected = view.selected.min(sightings.len().saturating_sub(1));
            view.sightings = sightings;
        }
        Ok(())
    }

    // -- Anki sync --------------------------------------------------------

    fn sync(&mut self) -> Result<()> {
        if self.cards.busy() {
            self.inform(Tone::Info, "Already syncing to Anki…");
            return Ok(());
        }

        let asked = self.start_sync()?;
        if asked == 0 {
            self.inform(Tone::Info, "Nothing to sync — Anki is up to date.");
        } else {
            self.inform(
                Tone::Info,
                format!("Syncing {asked} {} to Anki…", plural(asked, "Word")),
            );
        }
        Ok(())
    }

    /// Put every Word whose card is out of date in front of Anki.
    fn start_sync(&mut self) -> Result<usize> {
        let requests = self.store.changed_cards()?;
        self.syncing = Syncing {
            asked: requests.len(),
            ..Syncing::default()
        };

        for request in &requests {
            let definitions = self.dictionary.look_up(&request.spelling)?;
            let sightings = self.store.sightings(request.word_id)?;
            self.cards.push(Card::assemble(
                request,
                &self.config.deck,
                &definitions,
                &sightings,
            ));
        }
        Ok(requests.len())
    }

    /// Take in every push that has finished. Never waits, so the event loop can
    /// call it on every pass.
    pub fn collect_synced(&mut self) -> Result<()> {
        let finished = self.cards.collect();
        self.took(finished)
    }

    /// Wait for the sync in flight, then take in what came back.
    ///
    /// Bounded, which is why this is also what leaving calls: whatever Anki has
    /// not answered by the deadline is left queued rather than waited on.
    pub async fn settle_sync(&mut self) -> Result<()> {
        let finished = self.cards.settle(PATIENCE_ON_THE_WAY_OUT).await;
        self.took(finished)?;
        // Said even when nothing came back at all, because a sync that pushed
        // nothing is exactly the one the reader needs telling about.
        self.report_sync();
        Ok(())
    }

    fn took(&mut self, finished: Vec<CardOutcome>) -> Result<()> {
        if finished.is_empty() {
            return Ok(());
        }

        for outcome in finished {
            self.syncing.answered += 1;
            match outcome.pushed {
                Ok(anki_note_id) => {
                    self.store
                        .mark_synced(outcome.word_id, anki_note_id, outcome.revision)?;
                    self.syncing.pushed += 1;
                }
                // Neither one is recorded against the Word: it stays queued and
                // the next sync tries again. Only the wording differs.
                Err(Unpushed::Unavailable(_)) => self.syncing.anki_was_shut = true,
                Err(Unpushed::Refused(_)) => {}
            }
        }

        if self.syncing.answered >= self.syncing.asked {
            self.report_sync();
        }
        Ok(())
    }

    /// One sentence about what a sync did, so the reader can trust the state of
    /// their deck rather than guess at it.
    fn report_sync(&mut self) {
        let Syncing {
            asked,
            pushed,
            anki_was_shut,
            ..
        } = self.syncing;
        if asked == 0 {
            return;
        }

        // Everything not pushed is queued, whether Anki refused it or never
        // answered at all — there is nowhere else for it to have gone.
        let queued = asked.saturating_sub(pushed);
        let words = plural(queued, "Word");
        let (tone, text) = match (pushed, queued) {
            (_, 0) => (
                Tone::Info,
                format!("Synced {pushed} {} to Anki.", plural(pushed, "Word")),
            ),
            (0, _) if anki_was_shut => (
                Tone::Warning,
                format!(
                    "Anki isn't running — {queued} {words} {} queued.",
                    stays(queued)
                ),
            ),
            (0, _) => (
                Tone::Warning,
                format!(
                    "Nothing went to Anki — {queued} {words} {} queued.",
                    stays(queued)
                ),
            ),
            _ => (
                Tone::Warning,
                format!(
                    "Synced {pushed} of {asked} Words — {queued} {} queued.",
                    stays(queued)
                ),
            ),
        };
        self.farewell = Some(text.clone());
        self.inform(tone, text);
    }

    // -- Leaving ----------------------------------------------------------

    fn quit(&mut self, argument: &str) -> Result<()> {
        match argument.trim() {
            "" => {}
            "now" => self.sync_on_exit = false,
            other => {
                self.inform(
                    Tone::Warning,
                    format!(
                        "/quit takes nothing, or \"now\" to leave without syncing — not {other:?}."
                    ),
                );
                return Ok(());
            }
        }
        self.running = false;
        Ok(())
    }

    /// Begin the sync that leaving triggers.
    ///
    /// Separated from the waiting so the caller can put the screen in front of
    /// the reader while it happens rather than freezing on the last frame drawn.
    /// Finish it with [`App::settle_sync`], which is what bounds the wait.
    pub fn start_leaving(&mut self) -> Result<()> {
        if !self.sync_on_exit {
            return Ok(());
        }
        // A sync already under way is left to finish rather than doubled: two
        // pushes of one Word would each be a card Anki has never seen, and the
        // reader would get two. Anything captured since goes next time.
        if !self.cards.busy() {
            self.start_sync()?;
        }
        Ok(())
    }

    // -- Screens ----------------------------------------------------------

    fn open_selected(&mut self) -> Result<()> {
        let Screen::Search { results, selected } = &self.screen else {
            return Ok(());
        };
        let Some(result) = results.get(*selected) else {
            return Ok(());
        };

        let word_id = result.word_id;
        let origin = Origin::Search {
            query: self.input.trim().to_string(),
        };
        if let Some(word) = self.store.word(word_id)? {
            self.input.clear();
            self.show_word(word, origin)?;
        }
        Ok(())
    }

    fn show_word(&mut self, word: Word, origin: Origin) -> Result<()> {
        let definitions = self.dictionary.look_up(&word.spelling)?;
        let sightings = self.store.sightings(word.id)?;
        self.screen = Screen::Word(WordView {
            word,
            definitions,
            sightings,
            selected: 0,
            origin,
        });
        Ok(())
    }

    fn return_to(&mut self, origin: Origin) {
        match origin {
            Origin::Search { query } => {
                self.input = query;
                self.input_changed();
            }
            Origin::Home => self.go_home(),
        }
    }

    fn go_home(&mut self) {
        self.input.clear();
        self.screen = Screen::Home;
    }

    fn inform(&mut self, tone: Tone, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            tone,
        });
    }
}

/// "stays" or "stay", so the sentence about what didn't go reads properly
/// whether it was one Word or several.
fn stays(count: usize) -> &'static str {
    if count == 1 { "stays" } else { "stay" }
}

/// The completion and argument hint for a partly typed command, shown dimmed
/// after the cursor so the surface is learned by using it.
///
/// Returns `None` once an argument has been typed, or when the input is not an
/// unambiguous prefix of exactly one command.
pub fn argument_hint(input: &str) -> Option<String> {
    if !input.starts_with('/') {
        return None;
    }

    if let Some((name, argument)) = input.split_once(char::is_whitespace) {
        let command = COMMANDS.iter().find(|command| command.name == name)?;
        return if argument.trim().is_empty() && !command.argument.is_empty() {
            Some(command.argument.to_string())
        } else {
            None
        };
    }

    let mut matches = COMMANDS
        .iter()
        .filter(|command| command.name.starts_with(input));
    let command = matches.next()?;
    if matches.next().is_some() {
        return None; // Ambiguous: don't guess which one is meant.
    }

    let completion = &command.name[input.len()..];
    Some(match command.argument {
        "" => completion.to_string(),
        argument => format!("{completion} {argument}"),
    })
}
