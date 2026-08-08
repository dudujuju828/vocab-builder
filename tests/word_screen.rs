//! Reading a Word back.

mod harness;

use harness::Harness;
use ratatui::crossterm::event::KeyCode;

#[test]
fn the_definition_is_always_there() {
    let mut vocab = Harness::new();
    vocab.submit("/book Moby-Dick").press(KeyCode::Char('y'));

    vocab.submit("/add abnegation");
    vocab.submit("An abnegation of everything he had claimed.");

    vocab.assert_shows("the denial and rejection of a doctrine or belief");
}

#[test]
fn every_definition_is_shown_with_its_part_of_speech() {
    let mut vocab = Harness::new();
    vocab.submit("/book Moby-Dick").press(KeyCode::Char('y'));

    vocab.submit("/add sesquipedalian");
    vocab.submit("A sesquipedalian streak ran through the prose.");

    vocab
        .assert_shows("(noun) a very long word")
        .assert_shows("(adjective) given to the overuse of long words");
}

#[test]
fn each_sighting_shows_its_sentence_its_book_and_its_date() {
    let mut vocab = Harness::new();
    vocab.submit("/book Moby-Dick").press(KeyCode::Char('y'));
    vocab.submit("/add cetacean");
    vocab.submit("A great cetacean surfaced beside the boat.");

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    vocab
        .assert_shows(&format!("{today} · Moby-Dick"))
        .assert_shows("A great cetacean surfaced beside the boat.");
}

#[test]
fn sightings_are_ordered_most_recent_first() {
    let mut vocab = Harness::new();
    vocab.submit("/book Moby-Dick").press(KeyCode::Char('y'));
    vocab.submit("/add cetacean");
    vocab.submit("The first time I met it.");
    vocab.submit("/add cetacean").press(KeyCode::Char('y'));
    vocab.submit("The second time I met it.");

    vocab.assert_shows_in_order("The second time I met it.", "The first time I met it.");
}

#[test]
fn a_word_with_no_definition_still_shows_all_its_sightings() {
    let mut vocab = Harness::new();
    vocab.submit("/book Moby-Dick").press(KeyCode::Char('y'));
    vocab.submit("/add pequod");
    vocab.submit("The Pequod sailed at dawn.");
    vocab.submit("/add pequod").press(KeyCode::Char('y'));
    vocab.submit("The Pequod was gone by morning.");

    vocab
        .assert_shows("No Definition")
        .assert_shows("2 Sightings")
        .assert_shows("The Pequod sailed at dawn.")
        .assert_shows("The Pequod was gone by morning.");
}

#[test]
fn leaving_a_word_opened_from_a_search_returns_to_that_search() {
    let mut vocab = Harness::new();
    vocab.submit("/book Moby-Dick").press(KeyCode::Char('y'));
    vocab.submit("/add cetacean");
    vocab.submit("A great cetacean surfaced.");

    vocab.type_text("cetacean").enter();
    vocab.assert_shows("large aquatic carnivorous mammal");

    vocab.press(KeyCode::Esc);
    vocab.assert_shows("1 matching Word");
}
