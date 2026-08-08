//! The Library and the Current Book.

mod harness;

use harness::Harness;
use ratatui::crossterm::event::KeyCode;

#[test]
fn switching_to_an_unknown_book_offers_to_add_it() {
    let mut vocab = Harness::new();

    vocab.submit("/book Moby-Dick");

    vocab.assert_shows("not in your Library");
}

#[test]
fn accepting_adds_the_book_and_starts_reading_it() {
    let mut vocab = Harness::new();

    vocab.submit("/book Moby-Dick").press(KeyCode::Char('y'));

    vocab.assert_shows("Now reading Moby-Dick").assert_shows("Reading Moby-Dick");
}

#[test]
fn declining_leaves_the_library_alone() {
    let mut vocab = Harness::new();

    vocab.submit("/book Moby-Dick").press(KeyCode::Char('n'));

    vocab.submit("/library");
    vocab.assert_shows("No Books yet");
}

#[test]
fn the_library_lists_every_book_with_its_capture_count() {
    let mut vocab = Harness::new();
    vocab.submit("/book Moby-Dick").press(KeyCode::Char('y'));
    vocab.submit("/add cetacean");
    vocab.submit("A great cetacean surfaced.");
    vocab.submit("/add doubloon");
    vocab.submit("He nailed the doubloon to the mast.");
    vocab.submit("/book Dune").press(KeyCode::Char('y'));

    vocab.submit("/library");

    vocab
        .assert_shows("Moby-Dick")
        .assert_shows("2 Words")
        .assert_shows("Dune")
        .assert_shows("0 Words");
}

#[test]
fn the_library_marks_which_book_is_being_read() {
    let mut vocab = Harness::new();
    vocab.submit("/book Moby-Dick").press(KeyCode::Char('y'));
    vocab.submit("/book Dune").press(KeyCode::Char('y'));

    vocab.submit("/library");

    vocab.assert_shows("Dune 0 Words ← reading");
}

#[test]
fn switching_to_a_book_already_in_the_library_just_switches() {
    let mut vocab = Harness::new();
    vocab.submit("/book Moby-Dick").press(KeyCode::Char('y'));
    vocab.submit("/book Dune").press(KeyCode::Char('y'));

    vocab.submit("/book Moby-Dick");

    vocab
        .assert_shows("Reading Moby-Dick")
        .assert_does_not_show("not in your Library");
}

#[test]
fn the_current_book_survives_a_restart() {
    let mut vocab = Harness::new();
    vocab.submit("/book Moby-Dick").press(KeyCode::Char('y'));

    let mut vocab = vocab.restart();

    vocab.assert_shows("Reading Moby-Dick");
}

#[test]
fn captures_after_a_restart_go_to_the_remembered_book() {
    let mut vocab = Harness::new();
    vocab.submit("/book Moby-Dick").press(KeyCode::Char('y'));

    let mut vocab = vocab.restart();
    vocab.submit("/add cetacean");
    vocab.submit("A great cetacean surfaced.");

    vocab.assert_shows("Moby-Dick");
}

#[test]
fn a_reader_with_no_book_is_told_so_on_the_opening_screen() {
    let mut vocab = Harness::new();

    vocab.assert_shows("No Book yet");
}
