//! The application: what is on screen, what the input line is collecting, and
//! what each key does.
//!
//! `App` holds no terminal of its own. It is handed its dependencies, driven
//! with key events, and rendered into a frame — which is what lets tests drive
//! the whole tool the way a reader does.

use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::dictionary::Dictionary;
use crate::domain::{Book, Sense, Sighting, Word};
use crate::search::{Search, SearchResult};
use crate::store::{CorpusEntry, Store};
use crate::ui;

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
        name: "/help",
        argument: "",
        help: "list every command",
    },
    Command {
        name: "/quit",
        argument: "",
        help: "leave, restoring your terminal",
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
    },
    Help,
}

pub struct WordView {
    pub word: Word,
    pub senses: Vec<Sense>,
    pub sightings: Vec<Sighting>,
    /// Where this screen was opened from, so leaving it returns there.
    pub origin: Origin,
}

#[derive(Clone)]
pub enum Origin {
    Home,
    Search { query: String },
    Library,
}

/// What the input line is collecting. Capture is a prompt sequence rather than
/// a Screen of its own.
pub enum Prompt {
    None,
    /// The word is known; collecting the sentence it was met in.
    Sentence { spelling: String },
    /// This Word is already held — add another context for it?
    AnotherSighting { spelling: String },
    /// This Book is not in the Library yet — add it?
    AddBook { name: String },
}

impl Prompt {
    /// The text shown at the head of the input line.
    pub fn label(&self) -> Option<String> {
        match self {
            Self::None => None,
            Self::Sentence { .. } => Some("sentence:".to_string()),
            Self::AnotherSighting { spelling } => {
                Some(format!("add another context for \"{spelling}\"? [y/n]"))
            }
            Self::AddBook { name } => Some(format!("\"{name}\" is not in your Library — add it? [y/n]")),
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

pub struct App {
    store: Store,
    dictionary: Dictionary,
    search: Search,
    corpus: Vec<CorpusEntry>,
    screen: Screen,
    input: String,
    prompt: Prompt,
    message: Option<Message>,
    running: bool,
}

impl App {
    pub fn new(store: Store, dictionary: Dictionary) -> Result<Self> {
        let corpus = store.corpus()?;
        Ok(Self {
            store,
            dictionary,
            search: Search::new(),
            corpus,
            screen: Screen::Home,
            input: String::new(),
            prompt: Prompt::None,
            message: None,
            running: true,
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

    pub fn current_book(&self) -> Option<Book> {
        self.store.current_book().ok().flatten()
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

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
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
        let negative = matches!(key.code, KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc);
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
                    self.store.set_current_book(book_id)?;
                    self.inform(Tone::Info, format!("Now reading {name}."));
                    self.go_home();
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
                // Sighting with no context, which is the whole point of one.
                self.prompt = Prompt::Sentence { spelling };
                self.inform(Tone::Warning, "Type the sentence you met the word in.");
                return Ok(());
            }
            self.input.clear();
            return self.capture(&spelling, &sentence);
        }

        let input = self.input.trim().to_string();
        if input.is_empty() {
            return Ok(());
        }

        if input.starts_with('/') {
            self.input.clear();
            return self.run_command(&input);
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
        if let Screen::Search { results, selected } = &mut self.screen {
            if results.is_empty() {
                return;
            }
            let last = results.len() - 1;
            *selected = match delta {
                d if d < 0 => selected.saturating_sub(1),
                _ => (*selected + 1).min(last),
            };
        }
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
            "/help" => {
                self.screen = Screen::Help;
                Ok(())
            }
            "/quit" => {
                self.running = false;
                Ok(())
            }
            _ => {
                self.inform(
                    Tone::Warning,
                    format!("{name} isn't a command. Try /help."),
                );
                Ok(())
            }
        }
    }

    fn start_capture(&mut self, argument: &str) -> Result<()> {
        let spelling = argument.trim();
        if spelling.is_empty() {
            self.inform(Tone::Warning, "/add takes the word you met: /add <word>");
            return Ok(());
        }
        if spelling.split_whitespace().count() > 1 {
            self.inform(
                Tone::Warning,
                "A Word is a single word — the sentence comes next.",
            );
            return Ok(());
        }
        if self.store.current_book()?.is_none() {
            self.inform(
                Tone::Warning,
                "No Book yet — set one with /book <name> so captures have a source.",
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
        let Some(book) = self.store.current_book()? else {
            self.inform(Tone::Warning, "No Book set — nothing was saved.");
            return Ok(());
        };

        let word_id = self.store.capture(spelling, book.id, sentence)?;
        self.corpus = self.store.corpus()?;

        let Some(word) = self.store.word(word_id)? else {
            self.inform(Tone::Warning, "That Word could not be read back.");
            return Ok(());
        };
        let senses_found = !self.dictionary.look_up(&word.spelling)?.is_empty();
        self.show_word(word, Origin::Home)?;

        if senses_found {
            self.inform(Tone::Info, format!("Captured from {}.", book.name));
        } else {
            self.inform(
                Tone::Info,
                format!("Captured from {} — no definition, but the Sighting is kept.", book.name),
            );
        }
        Ok(())
    }

    fn switch_book(&mut self, argument: &str) -> Result<()> {
        let name = argument.trim();
        if name.is_empty() {
            self.inform(Tone::Warning, "/book takes the title: /book <name>");
            return Ok(());
        }

        match self.store.find_book(name)? {
            Some(book) => {
                self.store.set_current_book(book.id)?;
                self.inform(Tone::Info, format!("Now reading {}.", book.name));
                self.go_home();
            }
            None => {
                self.prompt = Prompt::AddBook {
                    name: name.to_string(),
                };
            }
        }
        Ok(())
    }

    fn show_library(&mut self) -> Result<()> {
        self.screen = Screen::Library {
            books: self.store.books()?,
        };
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
        let senses = self.dictionary.look_up(&word.spelling)?;
        let sightings = self.store.sightings(word.id)?;
        self.screen = Screen::Word(WordView {
            word,
            senses,
            sightings,
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
            Origin::Library => {
                self.input.clear();
                if let Ok(books) = self.store.books() {
                    self.screen = Screen::Library { books };
                } else {
                    self.go_home();
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_complete_an_unambiguous_command() {
        assert_eq!(argument_hint("/ad").as_deref(), Some("d <word>"));
        assert_eq!(argument_hint("/add").as_deref(), Some(" <word>"));
        assert_eq!(argument_hint("/li").as_deref(), Some("brary"));
    }

    #[test]
    fn hints_stop_once_an_argument_is_typed() {
        assert_eq!(argument_hint("/add pequod"), None);
    }

    #[test]
    fn hints_stay_quiet_when_the_command_is_ambiguous_or_unknown() {
        // Both /add and /book would be guesses for a bare slash.
        assert_eq!(argument_hint("/"), None);
        assert_eq!(argument_hint("/nope"), None);
        assert_eq!(argument_hint("whale"), None);
    }
}
