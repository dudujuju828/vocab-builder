//! Capturing a Word: the sentence prompt, the Definition, and meeting a Word
//! you already have.

mod harness;

use harness::Harness;
use ratatui::crossterm::event::KeyCode;

/// Start with a Book, since every Sighting is attributed to the Current Book.
fn reading(book: &str) -> Harness {
    let mut vocab = Harness::new();
    vocab.submit(&format!("/book {book}")).press(KeyCode::Char('y'));
    vocab
}

#[test]
fn capturing_a_word_records_it_and_shows_the_definition() {
    let mut vocab = reading("Moby-Dick");

    vocab.submit("/add cetacean");
    vocab.submit("A great cetacean surfaced beside the boat.");

    vocab
        .assert_shows("cetacean")
        .assert_shows("large aquatic carnivorous mammal")
        .assert_shows("A great cetacean surfaced beside the boat.")
        .assert_shows("Moby-Dick")
        .assert_shows("1 Sighting");
}

#[test]
fn the_sentence_is_asked_for_after_the_word() {
    let mut vocab = reading("Moby-Dick");

    vocab.submit("/add cetacean");

    vocab.assert_shows("sentence:");
}

#[test]
fn the_capture_date_is_recorded_without_being_asked_for() {
    let mut vocab = reading("Moby-Dick");

    vocab.submit("/add cetacean");
    vocab.submit("A great cetacean surfaced.");

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    vocab.assert_shows(&today);
}

#[test]
fn a_word_the_dictionary_does_not_have_is_still_captured() {
    let mut vocab = reading("Moby-Dick");

    vocab.submit("/add pequod");
    vocab.submit("The Pequod sailed at dawn.");

    vocab
        .assert_shows("pequod")
        .assert_shows("No definition")
        .assert_shows("The Pequod sailed at dawn.")
        .assert_shows("1 Sighting");
}

#[test]
fn a_capture_can_be_cancelled_part_way_through() {
    let mut vocab = reading("Moby-Dick");

    vocab.submit("/add doubloon");
    vocab.type_text("a half-typed sen").press(KeyCode::Esc);

    vocab.assert_shows("cancelled");

    // Nothing was saved, so there is nothing to find.
    vocab.type_text("doubloon");
    vocab.assert_shows("Nothing matches");
}

#[test]
fn capturing_needs_a_book_to_attribute_to() {
    let mut vocab = Harness::new();

    vocab.submit("/add cetacean");

    vocab.assert_shows("No Book yet");
}

// -- Meeting a Word again -------------------------------------------------

#[test]
fn meeting_a_word_again_says_so_and_shows_where_it_was_met_before() {
    let mut vocab = reading("Moby-Dick");
    vocab.submit("/add cetacean");
    vocab.submit("A great cetacean surfaced beside the boat.");

    vocab.submit("/add cetacean");

    vocab
        .assert_shows("You already have")
        .assert_shows("A great cetacean surfaced beside the boat.")
        .assert_shows("add another context");
}

#[test]
fn saying_yes_attaches_a_second_sighting_to_the_same_word() {
    let mut vocab = reading("Moby-Dick");
    vocab.submit("/add cetacean");
    vocab.submit("A great cetacean surfaced beside the boat.");

    vocab.submit("/add cetacean").press(KeyCode::Char('y'));
    vocab.submit("Every cetacean in the bay had gone.");

    vocab
        .assert_shows("2 Sightings")
        .assert_shows("A great cetacean surfaced beside the boat.")
        .assert_shows("Every cetacean in the bay had gone.");
}

#[test]
fn saying_no_leaves_the_word_exactly_as_it_was() {
    let mut vocab = reading("Moby-Dick");
    vocab.submit("/add cetacean");
    vocab.submit("A great cetacean surfaced beside the boat.");

    vocab.submit("/add cetacean").press(KeyCode::Char('n'));

    vocab
        .assert_shows("Left unchanged")
        .assert_shows("1 Sighting")
        .assert_does_not_show("2 Sightings");
}

#[test]
fn a_second_sighting_can_come_from_a_different_book() {
    let mut vocab = reading("Moby-Dick");
    vocab.submit("/add cetacean");
    vocab.submit("A great cetacean surfaced beside the boat.");

    vocab.submit("/book Dune").press(KeyCode::Char('y'));
    vocab.submit("/add cetacean").press(KeyCode::Char('y'));
    vocab.submit("No cetacean has ever crossed this sand.");

    vocab
        .assert_shows("2 Sightings")
        .assert_shows("Moby-Dick")
        .assert_shows("Dune");
}

#[test]
fn differing_case_reaches_the_same_word() {
    let mut vocab = reading("Moby-Dick");
    vocab.submit("/add cetacean");
    vocab.submit("A great cetacean surfaced.");

    vocab.submit("/add Cetacean");

    vocab.assert_shows("You already have");
}
