//! Putting right a Sighting that was captured wrong.
//!
//! A sentence is copied out of a book by hand, so getting one wrong is ordinary
//! rather than exceptional. What makes it worth its own file is everything that
//! hangs off the sentence: the Note reads it, the card carries it, and search
//! matches against it — so a correction that only reached the screen would
//! leave three quieter copies of the mistake behind.

mod harness;

use harness::Harness;
use ratatui::crossterm::event::KeyCode;

/// The wrong sentence, as mistyped, and the same sentence as the Book has it.
const MISTYPED: &str = "A great cetacean surfaced beside the goat.";
const CORRECTED: &str = "A great cetacean surfaced beside the boat.";

/// The word that was got wrong, which is all a reader has to back over.
const MISTAKE: &str = "goat.";

/// A Word already captured with the sentence typed wrong, sitting on the Word
/// screen where `/edit` is reached from.
fn miscaptured() -> Harness {
    let mut vocab = Harness::new();
    vocab
        .submit("/book Moby-Dick")
        .press(KeyCode::Char('y'))
        .submit("/add cetacean")
        .submit(MISTYPED);
    vocab
}

/// Correct it the way a reader does: `/edit` starts the line on the sentence
/// that is already there, so putting it right means backing over the word that
/// is wrong and typing it again — not entering the sentence a second time.
fn put_right(vocab: &mut Harness) {
    vocab.submit("/edit");
    for _ in 0..MISTAKE.len() {
        vocab.press(KeyCode::Backspace);
    }
    vocab.submit("boat.");
}

#[test]
fn a_mistyped_sentence_can_be_put_right() {
    let mut vocab = miscaptured();

    put_right(&mut vocab);

    vocab.assert_shows(CORRECTED);
    let screen = vocab.screen();
    assert!(
        !screen.contains("goat"),
        "the sentence as mistyped is still on screen\n\n--- screen ---\n{screen}\n--------------"
    );
}

/// A correction is usually a word or two out of place. Starting from an empty
/// line would mean retyping the sentence to fix one of them, which is the thing
/// that mistyped it in the first place.
#[test]
fn the_line_starts_on_the_sentence_being_corrected() {
    let mut vocab = miscaptured();

    vocab.submit("/edit");

    vocab.assert_shows("corrected sentence:");
    let line = vocab.input_line();
    assert!(
        line.contains("beside the goat."),
        "the line did not start on the sentence: {line:?}"
    );
}

/// One encounter stays one Sighting. The alternative — capturing it again —
/// records a second meeting with the Word that never happened.
#[test]
fn correcting_a_sentence_does_not_make_a_second_sighting() {
    let mut vocab = miscaptured();

    put_right(&mut vocab);

    vocab.assert_shows("1 Sighting");
}

/// The Note is a reading of the Word *in that sentence*. Left alone it would go
/// on explaining a sentence that was never in the Book.
#[test]
fn correcting_a_sentence_has_the_note_written_again() {
    let mut vocab = miscaptured();
    vocab.settle();
    // The stub echoes the sentence it was handed, so the first Note is proof
    // the wrong one reached the writer.
    vocab.assert_shows("beside the goat.");

    put_right(&mut vocab);
    vocab.settle();

    // Numbered by the stub, so this is the second asking rather than the first
    // one still on screen.
    vocab.assert_shows("reading 2").assert_shows(CORRECTED);
}

#[test]
fn a_sighting_being_corrected_shows_its_note_as_pending() {
    let mut vocab = miscaptured();
    vocab.settle();

    put_right(&mut vocab);

    // Before the answer lands: the old reading is gone rather than left
    // standing over a sentence it was not written about.
    vocab.assert_shows("pending");
}

/// Anki holds the sentence too, so a correction that stopped at the database
/// would leave the deck still drilling the mistake.
#[test]
fn a_corrected_sentence_goes_to_anki() {
    let mut vocab = miscaptured();
    vocab.submit("/sync").settle();

    put_right(&mut vocab);
    vocab.settle();
    vocab.submit("/sync").settle();

    let card = vocab.cards().pop().expect("a card to have been pushed");
    assert!(
        card.back.contains(CORRECTED) && !card.back.contains("goat"),
        "the card still carries the mistyped sentence\n\n--- back ---\n{}\n------------",
        card.back
    );
}

/// Sentences are searchable, and the corpus search matches against is held in
/// memory — so a correction has to reach it as well as the database.
#[test]
fn a_corrected_sentence_is_what_search_matches_against() {
    let mut vocab = miscaptured();

    put_right(&mut vocab);
    vocab.press(KeyCode::Esc);
    vocab.type_text("boat");

    vocab.assert_shows("cetacean");
}

#[test]
fn escape_leaves_the_sentence_as_it_was() {
    let mut vocab = miscaptured();

    vocab
        .submit("/edit")
        .type_text(" and away")
        .press(KeyCode::Esc);

    vocab.assert_shows(MISTYPED);
    vocab.assert_shows("Left as it was");
}

/// Emptying the line is not how a Sighting loses its sentence — a Sighting
/// without one is not a Sighting.
#[test]
fn a_sentence_cannot_be_emptied() {
    let mut vocab = miscaptured();

    vocab.submit("/edit");
    for _ in 0..MISTYPED.len() {
        vocab.press(KeyCode::Backspace);
    }
    vocab.enter();

    vocab.assert_shows("Type the sentence as the Book has it.");
    vocab.press(KeyCode::Esc).assert_shows(MISTYPED);
}

#[test]
fn edit_away_from_a_word_says_what_it_needs() {
    let mut vocab = Harness::new();

    vocab.submit("/edit");

    vocab.assert_shows("open a Word first");
}

#[test]
fn typing_a_correction_does_not_run_a_search() {
    let mut vocab = miscaptured();

    // "cetacean" would match the Word if this were taken for a query, and the
    // Word screen the correction was started from would be gone.
    vocab.submit("/edit").type_text(" cetacean");

    vocab.assert_shows("1 Sighting");
}
