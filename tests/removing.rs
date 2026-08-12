//! Taking a Word out.
//!
//! The one thing the tool does that cannot be undone, which is why it asks
//! first. A Word does not go alone: its Sightings and their Notes go with it,
//! search stops matching it, its Book stops counting it, and — because a Word
//! the reader has got rid of should stop coming up for review — the card Anki
//! holds for it is deleted. That last one is the reason this has a file of its
//! own: the deck is the one place a removal has to reach across a process
//! boundary, and Anki is shut most of the time.

mod harness;

use harness::Harness;
use ratatui::crossterm::event::KeyCode;

const SENTENCE: &str = "A great cetacean surfaced beside the boat.";
const AGAIN: &str = "Every cetacean in the bay had gone.";

/// One captured Word, sitting on the Word screen where `/remove` is reached
/// from.
fn captured() -> Harness {
    let mut vocab = Harness::new();
    vocab
        .submit("/book Moby-Dick")
        .press(KeyCode::Char('y'))
        .submit("/add cetacean")
        .submit(SENTENCE);
    vocab
}

/// Meet the same Word a second time, so there is more than one Sighting to lose.
fn met_again(vocab: &mut Harness) {
    vocab
        .submit("/add cetacean")
        .press(KeyCode::Char('y'))
        .submit(AGAIN);
}

fn remove_it(vocab: &mut Harness) {
    vocab.submit("/remove").press(KeyCode::Char('y'));
}

// -- The asking -----------------------------------------------------------

#[test]
fn remove_is_listed_in_help() {
    let mut vocab = Harness::new();

    vocab.submit("/help");

    vocab.assert_shows("/remove");
}

#[test]
fn the_word_screen_says_how_to_remove_what_is_on_it() {
    let mut vocab = captured();

    vocab.assert_shows("/remove this Word");
}

#[test]
fn removing_asks_before_anything_goes() {
    let mut vocab = captured();

    vocab.submit("/remove");

    vocab.assert_shows("remove \"cetacean\" and its 1 Sighting?");
    // Nothing has happened yet: the Word is still on the screen behind it.
    vocab.assert_shows(SENTENCE);
}

/// The Sightings are the part a reader is likely to have forgotten, and they go
/// too — so the asking counts them rather than naming only the Word.
#[test]
fn the_asking_names_how_many_sightings_go() {
    let mut vocab = captured();
    met_again(&mut vocab);

    vocab.submit("/remove");

    vocab.assert_shows("and its 2 Sightings?");
}

#[test]
fn declining_leaves_the_word_exactly_as_it_was() {
    let mut vocab = captured();

    vocab.submit("/remove").press(KeyCode::Char('n'));

    vocab.assert_shows("\"cetacean\" was not removed.");
    vocab.assert_shows(SENTENCE);
    // Still there to be found, not merely still drawn.
    vocab.press(KeyCode::Esc).type_text("cetacean");
    vocab.assert_shows("1 matching Word");
}

#[test]
fn escape_declines_a_removal() {
    let mut vocab = captured();

    vocab.submit("/remove").press(KeyCode::Esc);

    vocab.assert_shows("was not removed");
}

/// Nothing here can be got back, so a key that means neither yes nor no leaves
/// the question standing rather than being read as one of them.
#[test]
fn a_stray_key_neither_removes_nor_declines() {
    let mut vocab = captured();

    vocab.submit("/remove").press(KeyCode::Char('x'));

    vocab.assert_shows("remove \"cetacean\"");
}

#[test]
fn remove_away_from_a_word_says_what_it_needs() {
    let mut vocab = Harness::new();

    vocab.submit("/remove");

    vocab.assert_shows("open one first");
}

// -- What goes with it ----------------------------------------------------

#[test]
fn a_word_that_was_removed_can_no_longer_be_found() {
    let mut vocab = captured();

    remove_it(&mut vocab);

    vocab.assert_shows("Removed \"cetacean\"");
    vocab.type_text("cetacean");
    vocab.assert_shows("Nothing matches");
}

#[test]
fn a_removed_word_is_still_gone_next_time() {
    let mut vocab = captured();
    remove_it(&mut vocab);

    let mut vocab = vocab.restart();
    vocab.type_text("cetacean");

    vocab.assert_shows("Nothing matches");
}

/// The Sightings go with the Word — an encounter with nothing is not a Sighting.
/// Capturing the Word again is a first meeting rather than a third.
#[test]
fn the_sightings_go_with_the_word() {
    let mut vocab = captured();
    met_again(&mut vocab);
    remove_it(&mut vocab);

    vocab.submit("/add cetacean");

    vocab.assert_does_not_show("You already have");
    vocab.submit(SENTENCE);
    vocab.assert_shows("1 Sighting");
}

#[test]
fn the_book_stops_counting_a_removed_word() {
    let mut vocab = captured();
    remove_it(&mut vocab);

    vocab.submit("/library");

    vocab.assert_shows("Moby-Dick").assert_shows("0 Words");
}

/// Removing from a search returns to it, and the results behind it are the
/// results now — otherwise the Word would still be listed, one keystroke from
/// opening a Screen about nothing.
#[test]
fn removing_from_a_search_goes_back_to_it_without_the_word() {
    let mut vocab = captured();
    vocab.press(KeyCode::Esc).type_text("cetacean").enter();
    vocab.assert_shows("1 Sighting");

    remove_it(&mut vocab);

    vocab
        .assert_shows("Removed")
        .assert_shows("Nothing matches");
}

// -- And the card in Anki -------------------------------------------------

#[test]
fn removing_a_word_deletes_its_card_in_anki() {
    let mut vocab = captured();
    vocab.submit("/sync").settle();

    remove_it(&mut vocab);
    vocab.submit("/sync").settle();

    assert_eq!(vocab.cards_removed().len(), 1);
    vocab.assert_shows("Removed 1 Card from Anki");
}

/// The note deleted is the note this Word had, read off the identifier Anki
/// handed back — not whatever card happens to carry the same word on its front.
#[test]
fn the_card_deleted_is_the_one_the_word_had() {
    let mut vocab = captured();
    vocab.submit("/sync").settle();
    // A second Sighting, so the next push carries the identifier the first came
    // back with and the test can read it.
    met_again(&mut vocab);
    vocab.submit("/sync").settle();
    let had = vocab
        .cards()
        .pop()
        .expect("the second push")
        .anki_note_id
        .expect("the identifier the first push came back with");

    remove_it(&mut vocab);
    vocab.submit("/sync").settle();

    assert_eq!(vocab.cards_removed(), vec![had]);
}

#[test]
fn a_word_that_never_reached_anki_leaves_nothing_to_delete() {
    let mut vocab = captured();

    remove_it(&mut vocab);
    vocab.submit("/sync").settle();

    vocab.assert_shows("Nothing to sync");
    assert!(vocab.cards().is_empty());
    assert!(vocab.cards_removed().is_empty());
}

/// Anki is shut most of the time, and removing a Word must cost nothing when it
/// is. The deletion queues exactly as a push does.
#[test]
fn a_removal_made_while_anki_was_shut_lands_once_it_opens() {
    let mut vocab = captured();
    vocab.submit("/sync").settle();
    vocab.anki_shuts();

    remove_it(&mut vocab);
    vocab.submit("/sync").settle();
    vocab.assert_shows("Anki isn't running");
    assert!(vocab.cards_removed().is_empty());

    vocab.anki_opens();
    vocab.submit("/sync").settle();

    assert_eq!(vocab.cards_removed().len(), 1);
}

#[test]
fn a_queued_removal_survives_a_restart() {
    let mut vocab = captured();
    vocab.submit("/sync").settle();
    vocab.anki_shuts();
    remove_it(&mut vocab);
    vocab.submit("/sync").settle();

    let mut vocab = vocab.restart();
    vocab.anki_opens();
    vocab.submit("/sync").settle();

    assert_eq!(vocab.cards_removed().len(), 1);
}

#[test]
fn a_card_already_deleted_is_not_asked_about_again() {
    let mut vocab = captured();
    vocab.submit("/sync").settle();
    remove_it(&mut vocab);
    vocab.submit("/sync").settle();

    vocab.submit("/sync").settle();

    vocab.assert_shows("Nothing to sync");
    assert_eq!(vocab.cards_removed().len(), 1);
}

/// The narrow window the whole of the queue exists for: the Word is taken out
/// while Anki is still being told about it. The answer comes back for a Word
/// that no longer exists, and the card it just made would otherwise be one
/// nothing here remembers — orphaned in the deck for good.
#[test]
fn a_word_removed_while_its_push_was_in_the_air_leaves_no_card_behind() {
    let mut vocab = captured();
    // Deliberately not settled: the push is in the air.
    vocab.submit("/sync");

    remove_it(&mut vocab);
    vocab.settle();
    vocab.submit("/sync").settle();

    assert_eq!(vocab.cards().len(), 1, "the card was made");
    assert_eq!(vocab.cards_removed().len(), 1, "and then taken out again");
}
