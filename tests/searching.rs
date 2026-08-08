//! Live search across Words, sentences and Book names.

mod harness;

use harness::Harness;
use ratatui::crossterm::event::KeyCode;

/// A small corpus: two Words from one Book, one from another.
fn stocked() -> Harness {
    let mut vocab = Harness::new();
    vocab.submit("/book Moby-Dick").press(KeyCode::Char('y'));
    vocab.submit("/add cetacean");
    vocab.submit("A great cetacean surfaced beside the boat.");
    vocab.submit("/add doubloon");
    vocab.submit("He nailed the doubloon to the mast.");
    vocab.submit("/book Dune").press(KeyCode::Char('y'));
    vocab.submit("/add sonorous");
    vocab.submit("His voice was sonorous in the still air.");
    vocab
}

#[test]
fn plain_text_with_no_command_prefix_searches() {
    let mut vocab = stocked();

    vocab.type_text("cetacean");

    vocab.assert_shows("1 matching Word").assert_shows("word");
}

#[test]
fn results_update_on_every_keystroke() {
    let mut vocab = stocked();

    vocab.type_text("cet");
    vocab.assert_shows("cetacean");

    // Three more characters that cannot match narrow it to nothing, without
    // anything having been submitted.
    vocab.type_text("zzz");
    vocab.assert_shows("Nothing matches");
}

#[test]
fn a_half_remembered_spelling_still_finds_the_word() {
    let mut vocab = stocked();

    vocab.type_text("ctcn");

    vocab.assert_shows("cetacean");
}

#[test]
fn searching_matches_the_sentences_too() {
    let mut vocab = stocked();

    vocab.type_text("nailed");

    vocab.assert_shows("doubloon").assert_shows("sentence");
}

#[test]
fn searching_matches_book_names() {
    let mut vocab = stocked();

    vocab.type_text("Moby");

    vocab
        .assert_shows("2 matching Words")
        .assert_shows("book")
        .assert_shows("cetacean")
        .assert_shows("doubloon");
}

#[test]
fn word_matches_are_ranked_above_sentence_matches() {
    let mut vocab = Harness::new();
    vocab.submit("/book Moby-Dick").press(KeyCode::Char('y'));
    vocab.submit("/add cetacean");
    vocab.submit("It surfaced beside the boat.");
    vocab.submit("/add doubloon");
    vocab.submit("The cetacean had taken it.");

    vocab.type_text("cetacean");

    vocab.assert_shows_in_order("cetacean", "doubloon");
}

#[test]
fn sentence_matches_are_ranked_above_book_matches() {
    let mut vocab = Harness::new();
    // The Book name is the query, so every Word in it matches on the Book band.
    vocab.submit("/book Cetacean Weekly").press(KeyCode::Char('y'));
    vocab.submit("/add doubloon");
    vocab.submit("He nailed it to the mast.");
    vocab.submit("/add sonorous");
    vocab.submit("A cetacean broke the surface.");

    vocab.type_text("cetacean");

    // sonorous matched a sentence; doubloon only matched the Book it came from.
    vocab.assert_shows_in_order("sonorous", "doubloon");
}

#[test]
fn each_result_says_which_kind_of_match_it_is() {
    let mut vocab = Harness::new();
    vocab.submit("/book Moby-Dick").press(KeyCode::Char('y'));
    vocab.submit("/add cetacean");
    vocab.submit("It surfaced beside the boat.");
    vocab.submit("/add doubloon");
    vocab.submit("The cetacean had taken it.");

    vocab.type_text("cetacean");

    // A sentence hit must not be mistakable for a Word hit.
    vocab
        .assert_shows("cetacean word")
        .assert_shows("doubloon sentence The cetacean had taken it.");
}

#[test]
fn a_word_met_several_times_appears_once() {
    let mut vocab = Harness::new();
    vocab.submit("/book Moby-Dick").press(KeyCode::Char('y'));
    vocab.submit("/add cetacean");
    vocab.submit("A great cetacean surfaced.");
    vocab.submit("/add cetacean").press(KeyCode::Char('y'));
    vocab.submit("Every cetacean had gone.");

    vocab.type_text("cetacea");

    vocab.assert_shows("1 matching Word");
}

#[test]
fn nothing_matching_says_so_plainly() {
    let mut vocab = stocked();

    vocab.type_text("zzzzz");

    vocab
        .assert_shows("Nothing matches")
        .assert_shows("never captured this one");
}

#[test]
fn opening_a_result_lands_on_that_words_detail() {
    let mut vocab = stocked();

    vocab.type_text("cetacean").enter();

    vocab
        .assert_shows("large aquatic carnivorous mammal")
        .assert_shows("A great cetacean surfaced beside the boat.");
}

#[test]
fn the_arrow_keys_choose_between_results() {
    let mut vocab = stocked();

    vocab.type_text("Moby").press(KeyCode::Down).enter();

    // Results are ordered cetacean then doubloon, so one step down opens the second.
    vocab.assert_shows("He nailed the doubloon to the mast.");
}

#[test]
fn leaving_a_word_returns_to_the_search_that_found_it() {
    let mut vocab = stocked();
    vocab.type_text("Moby").enter();

    vocab.press(KeyCode::Esc);

    vocab.assert_shows("2 matching Words").assert_shows("book");
}

#[test]
fn clearing_the_query_returns_to_the_opening_screen() {
    let mut vocab = stocked();
    vocab.type_text("cet");

    for _ in 0..3 {
        vocab.press(KeyCode::Backspace);
    }

    vocab.assert_shows("Reading Dune");
}
