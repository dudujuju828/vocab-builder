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
fn an_unknown_command_says_so_rather_than_searching() {
    let mut vocab = Harness::new();

    vocab.submit("/nonsense");

    vocab.assert_shows("isn't a command");
}

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
