//! Anki sync: what a Word becomes on a card, and what happens when Anki isn't
//! there.
//!
//! The card's payload is the one thing in the tool that a reader never sees on
//! screen, so these tests read it back off the fake `CardSync` — which, like the
//! Note stub, never leaves the process.

mod harness;

use harness::Harness;
use ratatui::crossterm::event::KeyCode;

/// A Book and one captured Word — the smallest thing there is to sync.
fn captured(mut vocab: Harness) -> Harness {
    vocab.submit("/book Moby-Dick").press(KeyCode::Char('y'));
    vocab.submit("/add cetacean");
    vocab.submit("A great cetacean surfaced beside the boat.");
    vocab
}

#[test]
fn sync_is_listed_in_help() {
    let mut vocab = Harness::new();

    vocab.submit("/help");

    vocab.assert_shows("/sync");
}

#[test]
fn sync_pushes_a_captured_word_and_says_so() {
    let mut vocab = captured(Harness::new());

    vocab.submit("/sync").settle();

    vocab.assert_shows("Synced 1 Word to Anki");
    assert_eq!(vocab.cards().len(), 1);
}

#[test]
fn syncing_with_nothing_captured_says_there_is_nothing_to_do() {
    let mut vocab = Harness::new();

    vocab.submit("/sync").settle();

    vocab.assert_shows("Nothing to sync");
    assert_eq!(vocab.cards().len(), 0);
}

// -- What the card carries ------------------------------------------------

#[test]
fn the_front_of_the_card_is_the_word() {
    let mut vocab = captured(Harness::new());

    vocab.submit("/sync").settle();

    assert_eq!(vocab.cards()[0].front, "cetacean");
}

#[test]
fn the_back_carries_the_definition_and_the_sighting() {
    let mut vocab = captured(Harness::new());

    vocab.submit("/sync").settle();

    let card = vocab.cards().remove(0);
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    for expected in [
        "large aquatic carnivorous mammal",
        "A great cetacean surfaced beside the boat.",
        "Moby-Dick",
        &today,
    ] {
        assert!(
            card.back.contains(expected),
            "expected the back to carry {expected:?}\n\n--- back ---\n{}\n------------",
            card.back
        );
    }
}

#[test]
fn the_back_carries_the_note_once_it_has_been_written() {
    let mut vocab = captured(Harness::new());
    vocab.settle();

    vocab.submit("/sync").settle();

    let card = vocab.cards().remove(0);
    assert!(
        card.back.contains("reading 1"),
        "expected the back to carry the Note\n\n--- back ---\n{}\n------------",
        card.back
    );
}

#[test]
fn a_word_with_no_definition_still_becomes_a_card() {
    let mut vocab = Harness::new();
    vocab.submit("/book Moby-Dick").press(KeyCode::Char('y'));
    vocab.submit("/add pequod");
    vocab.submit("The Pequod sailed at dawn.");

    vocab.submit("/sync").settle();

    let card = vocab.cards().remove(0);
    assert_eq!(card.front, "pequod");
    assert!(
        card.back.contains("The Pequod sailed at dawn."),
        "the Sighting is the whole of the card here\n\n--- back ---\n{}\n------------",
        card.back
    );
}

// -- One card per Word ----------------------------------------------------

#[test]
fn a_word_is_one_card_however_many_sightings_it_has() {
    let mut vocab = captured(Harness::new());
    vocab.submit("/sync").settle();

    vocab.submit("/add cetacean").press(KeyCode::Char('y'));
    vocab.submit("Every cetacean in the bay had gone.");
    vocab.submit("/sync").settle();

    // Pushed twice, but the second push carried the identifier the first came
    // back with — so it updated that card rather than making a second one.
    assert_eq!(vocab.cards().len(), 2);
    assert_eq!(vocab.anki_notes_created(), 1);
}

#[test]
fn a_new_sighting_puts_both_sentences_on_the_one_card() {
    let mut vocab = captured(Harness::new());
    vocab.submit("/sync").settle();

    vocab.submit("/add cetacean").press(KeyCode::Char('y'));
    vocab.submit("Every cetacean in the bay had gone.");
    vocab.submit("/sync").settle();

    let card = vocab.cards().pop().expect("the updated card");
    assert!(
        card.back
            .contains("A great cetacean surfaced beside the boat.")
    );
    assert!(card.back.contains("Every cetacean in the bay had gone."));
}

#[test]
fn a_word_that_has_not_changed_is_not_pushed_again() {
    let mut vocab = captured(Harness::new());
    vocab.settle();
    vocab.submit("/sync").settle();

    vocab.submit("/sync").settle();

    assert_eq!(vocab.cards().len(), 1);
    vocab.assert_shows("Nothing to sync");
}

/// A Note is written in the background, well after the Sighting it belongs to —
/// so the card that was pushed before it arrived is one Note short, and the
/// Word belongs back on the queue.
#[test]
fn a_note_arriving_after_a_sync_puts_the_word_back_on_the_queue() {
    let mut vocab = captured(Harness::new());
    vocab.submit("/sync").settle();
    assert!(!vocab.cards()[0].back.contains("reading 1"));

    vocab.submit("/sync").settle();

    let card = vocab.cards().pop().expect("the second push");
    assert!(
        card.back.contains("reading 1"),
        "expected the Note on the card by now\n\n--- back ---\n{}\n------------",
        card.back
    );
    assert_eq!(vocab.anki_notes_created(), 1);
}

// -- When Anki isn't there ------------------------------------------------

#[test]
fn anki_not_running_says_so_plainly_and_keeps_the_word_queued() {
    let mut vocab = captured(Harness::anki_closed());

    vocab.submit("/sync").settle();

    vocab.assert_shows("Anki isn't running");
    assert_eq!(vocab.cards().len(), 0);
}

#[test]
fn a_word_queued_while_anki_was_closed_goes_once_it_opens() {
    let mut vocab = captured(Harness::anki_closed());
    vocab.submit("/sync").settle();

    vocab.anki_opens();
    vocab.submit("/sync").settle();

    assert_eq!(vocab.cards().len(), 1);
    vocab.assert_shows("Synced 1 Word to Anki");
}

#[test]
fn a_queue_left_by_a_closed_anki_survives_a_restart() {
    let vocab = captured(Harness::anki_closed());

    let mut vocab = vocab.restart();
    vocab.anki_opens();
    vocab.submit("/sync").settle();

    assert_eq!(vocab.cards().len(), 1);
}

#[test]
fn anki_refusing_a_card_keeps_it_queued_rather_than_calling_it_synced() {
    let mut vocab = captured(Harness::anki_refusing());
    vocab.submit("/sync").settle();
    vocab.assert_shows("stays queued");

    vocab.anki_opens();
    vocab.submit("/sync").settle();

    assert_eq!(vocab.cards().len(), 1);
}

// -- Which deck ------------------------------------------------------------

#[test]
fn cards_land_in_the_vocab_deck_by_default() {
    let mut vocab = captured(Harness::new());

    vocab.submit("/sync").settle();

    assert_eq!(vocab.cards()[0].deck, "Vocab");
}

#[test]
fn the_deck_can_be_configured() {
    let mut vocab = captured(Harness::with_deck("Reading"));

    vocab.submit("/sync").settle();

    assert_eq!(vocab.cards()[0].deck, "Reading");
}

// -- Leaving ---------------------------------------------------------------

#[test]
fn quitting_syncs_on_the_way_out() {
    let mut vocab = captured(Harness::new());

    vocab.submit("/quit").leave();

    assert!(!vocab.is_running());
    assert_eq!(vocab.cards().len(), 1);
    vocab.assert_shows("Synced 1 Word to Anki");
}

#[test]
fn quit_now_leaves_without_syncing() {
    let mut vocab = captured(Harness::new());

    vocab.submit("/quit now").leave();

    assert!(!vocab.is_running());
    assert_eq!(vocab.cards().len(), 0);
}

#[test]
fn quit_now_is_listed_in_help() {
    let mut vocab = Harness::new();

    vocab.submit("/help");

    vocab.assert_shows("/quit [now]");
}

#[test]
fn what_quit_now_skipped_is_still_there_next_time() {
    let vocab = captured(Harness::new());
    let mut vocab = vocab;
    vocab.submit("/quit now").leave();

    let mut vocab = vocab.restart();
    vocab.submit("/sync").settle();

    assert_eq!(vocab.cards().len(), 1);
}

#[test]
fn syncing_on_exit_can_be_turned_off() {
    let mut vocab = captured(Harness::without_sync_on_exit());

    vocab.submit("/quit").leave();

    assert!(!vocab.is_running());
    assert_eq!(vocab.cards().len(), 0);
}

#[test]
fn a_closed_anki_does_not_hold_up_the_exit() {
    let vocab = captured(Harness::anki_closed());
    let mut vocab = vocab;

    vocab.submit("/quit").leave();

    assert!(!vocab.is_running());
    vocab.assert_shows("Anki isn't running");

    // Left queued, so the next launch with Anki open pushes it.
    let mut vocab = vocab.restart();
    vocab.anki_opens();
    vocab.submit("/sync").settle();
    assert_eq!(vocab.cards().len(), 1);
}

/// The stub never answers at all, so without a deadline on the way out this
/// test would hang for ever. **That it finishes is the assertion** — there is
/// deliberately no timing assertion here, because the harness runs with the
/// clock paused, which would make one vacuous. What is bounded is the tool's
/// waiting, not the wall clock.
#[test]
fn a_hanging_anki_does_not_hold_up_the_exit() {
    let mut vocab = captured(Harness::anki_hanging());

    vocab.submit("/quit").leave();

    assert!(!vocab.is_running());
    assert_eq!(vocab.cards().len(), 0);
    vocab.assert_shows("stays queued");
}

/// What never answered is still queued, so nothing was lost by having asked.
#[test]
fn a_word_a_hanging_anki_never_answered_for_goes_next_time() {
    let vocab = captured(Harness::anki_hanging());
    let mut vocab = vocab;
    vocab.submit("/quit").leave();

    let mut vocab = vocab.restart();
    vocab.anki_opens();
    vocab.submit("/sync").settle();

    assert_eq!(vocab.cards().len(), 1);
}

// -- Being told the truth on the way out ----------------------------------

/// Leaving without syncing must not repeat what the last `/sync` said, as
/// though it had just happened again.
#[test]
fn quit_now_does_not_claim_credit_for_an_earlier_sync() {
    let mut vocab = captured(Harness::new());
    vocab.submit("/sync").settle();
    assert_eq!(vocab.farewell(), Some("Synced 1 Word to Anki."));

    vocab.submit("/quit now").leave();

    assert_eq!(vocab.farewell(), None);
}

#[test]
fn an_exit_that_syncs_nothing_because_it_was_turned_off_says_nothing() {
    let mut vocab = captured(Harness::without_sync_on_exit());

    vocab.submit("/quit").leave();

    assert_eq!(vocab.farewell(), None);
}

#[test]
fn an_exit_that_synced_says_so_where_it_can_still_be_read() {
    let mut vocab = captured(Harness::new());

    vocab.submit("/quit").leave();

    assert_eq!(vocab.farewell(), Some("Synced 1 Word to Anki."));
}

// -- A settings file that can't be used ------------------------------------

/// The tool's whole promise is that capturing a Word is cheap. An optional
/// settings file is never worth standing between the reader and that.
#[test]
fn an_unusable_config_is_reported_but_does_not_stop_anything() {
    let mut vocab = Harness::with_an_unusable_config(
        "Ignoring config.toml — unknown field `dekc`. Carrying on with the defaults.",
    );

    vocab.assert_shows("Ignoring config.toml");

    vocab.submit("/book Moby-Dick").press(KeyCode::Char('y'));
    vocab.submit("/add cetacean");
    vocab.submit("A great cetacean surfaced beside the boat.");
    vocab.submit("/sync").settle();

    vocab.assert_shows("Synced 1 Word to Anki");
    // The defaults are what is in force, and they are ordinary defaults.
    assert_eq!(vocab.cards()[0].deck, "Vocab");
}
