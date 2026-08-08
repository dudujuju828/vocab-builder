//! The Note: the AI's reading of a Word in one Sighting's sentence.
//!
//! Notes are written in the background, so every test here drives the pending
//! work to completion with `settle` rather than sleeping or polling — and the
//! writer behind it is a stub, so nothing reaches the network.

mod harness;

use harness::Harness;
use ratatui::crossterm::event::KeyCode;

/// Start with a Book and one captured Word, which is where every Note begins.
fn captured(mut vocab: Harness) -> Harness {
    vocab.submit("/book Moby-Dick").press(KeyCode::Char('y'));
    vocab.submit("/add cetacean");
    vocab.submit("A great cetacean surfaced beside the boat.");
    vocab
}

#[test]
fn a_capture_comes_back_before_its_note_does() {
    let mut vocab = captured(Harness::new());

    vocab
        .assert_shows("A great cetacean surfaced beside the boat.")
        .assert_shows("Note pending")
        .assert_does_not_show("reading 1");
}

#[test]
fn the_note_arrives_without_the_reader_going_anywhere() {
    let mut vocab = captured(Harness::new());

    vocab.settle();

    vocab
        .assert_shows("reading 1")
        .assert_does_not_show("Note pending");
}

#[test]
fn the_writer_is_given_the_word_the_book_and_the_sentence() {
    let mut vocab = captured(Harness::new());

    vocab.settle();

    // The stub echoes back all three, so this only matches if all three
    // reached it.
    vocab.assert_shows(r#""cetacean" from Moby-Dick: A great cetacean surfaced beside the boat."#);
}

#[test]
fn the_note_sits_beside_the_definition_rather_than_replacing_it() {
    let mut vocab = captured(Harness::new());

    vocab.settle();

    vocab
        .assert_shows("large aquatic carnivorous mammal")
        .assert_shows_in_order("large aquatic carnivorous mammal", "Note");
}

#[test]
fn each_sighting_gets_its_own_note() {
    let mut vocab = captured(Harness::new());
    vocab.submit("/add cetacean").press(KeyCode::Char('y'));
    vocab.submit("Every cetacean in the bay had gone.");

    vocab.settle();

    vocab
        .assert_shows("A great cetacean surfaced beside the boat.")
        .assert_shows("Every cetacean in the bay had gone.")
        .assert_does_not_show("Note pending");
    assert_eq!(vocab.count_of("from Moby-Dick:"), 2);
}

// -- Surviving a restart --------------------------------------------------

#[test]
fn a_note_left_pending_is_written_on_the_next_launch() {
    // Captured with the Note never settled, as if the tool was closed on a
    // train before the writer ever ran.
    let vocab = captured(Harness::new());

    let mut vocab = vocab.restart();
    vocab.type_text("cetacean").enter();
    vocab.assert_shows("Note pending");

    vocab.settle();

    vocab
        .assert_shows("reading 1")
        .assert_does_not_show("Note pending");
}

#[test]
fn a_note_already_written_is_not_written_again() {
    let mut vocab = captured(Harness::new());
    vocab.settle();

    let mut vocab = vocab.restart();
    vocab.type_text("cetacean").enter();
    vocab.settle();

    // Still the first reading: the backlog was empty, so the writer was never
    // asked a second time.
    vocab.assert_shows("reading 1");
}

// -- When the writer fails ------------------------------------------------

#[test]
fn a_failed_note_says_so_and_leaves_its_sighting_whole() {
    let mut vocab = captured(Harness::failing());

    vocab.settle();

    vocab
        .assert_shows("Note couldn't be written")
        .assert_shows("/explain")
        .assert_shows("A great cetacean surfaced beside the boat.")
        .assert_shows("large aquatic carnivorous mammal")
        .assert_shows("1 Sighting");
}

#[test]
fn a_failed_note_stays_failed_across_a_restart_rather_than_retrying_itself() {
    let mut vocab = captured(Harness::failing());
    vocab.settle();

    let mut vocab = vocab.restart();
    vocab.type_text("cetacean").enter();

    // Failure is recoverable, but on the reader's say-so — not silently on
    // every launch.
    vocab
        .assert_shows("Note couldn't be written")
        .assert_does_not_show("Note pending");
}

// -- With no API key ------------------------------------------------------

#[test]
fn notes_are_off_without_a_key_and_the_tool_says_so_once() {
    let mut vocab = Harness::without_notes();

    vocab.assert_shows("Notes are off");

    vocab.submit("/book Moby-Dick").press(KeyCode::Char('y'));
    vocab.submit("/add cetacean");
    vocab.submit("A great cetacean surfaced beside the boat.");

    // The capture is ordinary — the missing key is not an error per capture.
    vocab
        .assert_shows("Captured from Moby-Dick")
        .assert_shows("Note pending")
        .assert_does_not_show("Notes are off");
}

// -- /explain -------------------------------------------------------------

#[test]
fn explain_is_listed_in_help() {
    let mut vocab = Harness::new();

    vocab.submit("/help");

    vocab.assert_shows("/explain");
}

#[test]
fn explain_rewrites_a_note_that_was_already_written() {
    let mut vocab = captured(Harness::new());
    vocab.settle();
    vocab.assert_shows("reading 1");

    vocab.submit("/explain");
    vocab
        .assert_shows("Note pending")
        .assert_does_not_show("reading 1");

    vocab.settle();
    vocab
        .assert_shows("reading 2")
        .assert_does_not_show("reading 1");
}

#[test]
fn explain_retries_a_note_that_failed() {
    let mut vocab = captured(Harness::failing());
    vocab.settle();
    vocab.assert_shows("Note couldn't be written");

    vocab.notes_recover();
    vocab.submit("/explain").settle();

    vocab
        .assert_shows(r#""cetacean" from Moby-Dick: A great cetacean surfaced beside the boat."#)
        .assert_does_not_show("Note couldn't be written");
}

#[test]
fn explain_asks_again_about_the_sighting_being_looked_at() {
    let mut vocab = captured(Harness::failing());
    vocab.submit("/add cetacean").press(KeyCode::Char('y'));
    vocab.submit("Every cetacean in the bay had gone.");
    vocab.settle();
    assert_eq!(vocab.count_of("Note couldn't be written"), 2);

    // Sightings run most recent first, so the second one down is the older.
    vocab.notes_recover();
    vocab.press(KeyCode::Down);
    vocab.submit("/explain").settle();

    vocab
        .assert_shows("from Moby-Dick: A great cetacean surfaced beside the boat.")
        .assert_does_not_show("from Moby-Dick: Every cetacean in the bay had gone.");
    assert_eq!(vocab.count_of("Note couldn't be written"), 1);
}

#[test]
fn explain_needs_a_sighting_to_work_on() {
    let mut vocab = Harness::new();

    vocab.submit("/explain");

    vocab.assert_shows("open a Word first");
}

#[test]
fn explain_says_nothing_will_happen_when_notes_are_off() {
    let mut vocab = Harness::without_notes();
    vocab.submit("/book Moby-Dick").press(KeyCode::Char('y'));
    vocab.submit("/add cetacean");
    vocab.submit("A great cetacean surfaced beside the boat.");

    vocab.submit("/explain");

    vocab.assert_shows("Notes are off");
}
