//! Launching, orienting, and leaving.

mod harness;

use harness::Harness;
use ratatui::crossterm::event::KeyCode;

#[test]
fn the_opening_screen_shows_ascii_art() {
    let mut vocab = Harness::new();

    // The middle bar of the "v", which no other screen draws.
    vocab.assert_shows("╚██╗ ██╔╝");
}

#[test]
fn the_opening_screen_shows_which_book_is_being_read() {
    let mut vocab = Harness::new();
    vocab.submit("/book Moby-Dick").press(KeyCode::Char('y'));

    vocab.assert_shows("Reading Moby-Dick");
}

#[test]
fn help_lists_every_command_and_what_it_takes() {
    let mut vocab = Harness::new();

    vocab.submit("/help");

    vocab
        .assert_shows("/add <word>")
        .assert_shows("/book <name>")
        .assert_shows("/library")
        .assert_shows("/help")
        .assert_shows("/quit");
}

#[test]
fn argument_hints_appear_as_a_command_is_typed() {
    let mut vocab = Harness::new();

    vocab.type_text("/ad");

    // The completion and the argument are both shown ahead of the cursor.
    vocab.assert_shows("/add <word>");
}

#[test]
fn a_hint_waits_until_one_command_is_meant() {
    let mut vocab = Harness::new();

    // A bare slash could still become any of them, so nothing is guessed.
    vocab.type_text("/");
    assert_eq!(vocab.input_line(), "› /");

    // One more character settles it.
    vocab.type_text("b");
    assert_eq!(vocab.input_line(), "› /book <name>");
}

#[test]
fn a_hint_stops_once_the_argument_is_being_typed() {
    let mut vocab = Harness::new();

    vocab.type_text("/add pequod");

    assert_eq!(vocab.input_line(), "› /add pequod");
}

#[test]
fn an_unknown_command_says_so_rather_than_searching() {
    let mut vocab = Harness::new();

    vocab.submit("/nonsense");

    vocab.assert_shows("isn't a command");
}

#[test]
fn add_on_its_own_says_what_it_needs() {
    let mut vocab = Harness::new();

    vocab.submit("/add");

    vocab.assert_shows("/add takes the Word you met");
}

#[test]
fn a_word_must_be_a_single_word() {
    let mut vocab = Harness::new();
    vocab.submit("/book Moby-Dick").press(KeyCode::Char('y'));

    vocab.submit("/add hard cash");

    vocab.assert_shows("A Word is a single word");
}

/// Quitting is the one behaviour with no expression on screen — the tool is
/// gone. This is the only assertion in the suite that reads application state
/// rather than the rendered buffer.
#[test]
fn quit_stops_the_tool() {
    let mut vocab = Harness::new();
    assert!(vocab.is_running());

    vocab.submit("/quit");

    assert!(!vocab.is_running());
}

#[test]
fn escape_from_a_search_returns_to_the_opening_screen() {
    let mut vocab = Harness::new();
    vocab.submit("/book Moby-Dick").press(KeyCode::Char('y'));
    vocab.type_text("cet");

    vocab.press(KeyCode::Esc);

    vocab.assert_shows("Reading Moby-Dick");
}
