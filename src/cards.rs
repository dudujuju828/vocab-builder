//! Cards: what a Word becomes in Anki, and the pushing of it.
//!
//! Vocab does not schedule reviews — Anki owns spaced repetition entirely, and
//! the sync is one way. This module turns a Word, its Definition and all its
//! Sightings into one card, and hands it to a [`CardSync`] to be written. The
//! network half lives in [`crate::anki`] and does not appear here, which is what
//! lets every test assert on real card content without reaching Anki.
//!
//! The mapping is **one Anki note per Word**, never per Sighting. A Word that
//! gains a Sighting carries the identifier of the note it already has, so the
//! push updates that card rather than making a second one: recurrence enriches
//! a card instead of multiplying cards.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Error;
use tokio::runtime::Handle;
use tokio::sync::Semaphore;
use tokio::task::{JoinError, JoinSet};

use crate::domain::{Definition, NoteState, Sighting};

/// How many cards are in the air at once.
///
/// AnkiConnect is one small server on the same machine, not a fleet. The first
/// sync after this feature arrives has every Word ever captured behind it — a
/// thousand of them asking at the same instant would swamp Anki and most would
/// come back queued for nothing. A handful at a time keeps a large backlog
/// moving without ever being rude about it.
const AT_ONCE: usize = 4;

/// A Word whose card is out of date — what the tool knows about it before the
/// Definition and the Sightings are attached.
#[derive(Debug, Clone)]
pub struct CardRequest {
    pub word_id: i64,
    pub spelling: String,
    /// The note this Word already has in Anki, absent until it has been pushed
    /// once. Its presence is what makes the next push an update.
    pub anki_note_id: Option<i64>,
    /// Which revision of the Word this was read at, so a Sighting captured
    /// while the push is in the air is not taken off the queue by the answer to
    /// a question asked before it existed.
    pub revision: i64,
}

/// One Anki note, assembled and ready to push.
///
/// `word_id` and `revision` are the tool's own bookkeeping rather than anything
/// Anki wants; they ride along so the answer can be matched back to the Word it
/// belongs to, exactly as a [`crate::notes::NoteRequest`] carries its Sighting.
#[derive(Debug, Clone)]
pub struct Card {
    pub word_id: i64,
    pub revision: i64,
    pub anki_note_id: Option<i64>,
    pub deck: String,
    pub front: String,
    pub back: String,
}

impl Card {
    /// Build the card for one Word.
    pub fn assemble(
        request: &CardRequest,
        deck: &str,
        definitions: &[Definition],
        sightings: &[Sighting],
    ) -> Self {
        Self {
            word_id: request.word_id,
            revision: request.revision,
            anki_note_id: request.anki_note_id,
            deck: deck.to_string(),
            front: escape(&request.spelling),
            back: back(definitions, sightings),
        }
    }
}

/// The back of the card: the Definition, then every Sighting that made this
/// Word worth keeping.
///
/// The context is what makes the card worth reviewing — a bare word-definition
/// pair is a poor card, and the sentence the Word was actually met in is what
/// makes the meaning concrete. Anki fields are HTML, so this is HTML; the
/// markup is deliberately plain so it reads on the stock Basic card.
fn back(definitions: &[Definition], sightings: &[Sighting]) -> String {
    let mut html = String::new();

    if definitions.is_empty() {
        // Said the same way the Word screen says it: a gap in the dictionary,
        // not a failure, and never an empty panel.
        html.push_str(
            "<div><i>No Definition — the bundled dictionary doesn't have this one.</i></div>",
        );
    }
    for definition in definitions {
        html.push_str(&format!(
            "<div><b>({})</b> {}</div>",
            escape(&definition.part_of_speech),
            escape(&definition.text)
        ));
    }

    for sighting in sightings {
        html.push_str("<hr>");
        html.push_str(&format!("<div>{}</div>", escape(&sighting.sentence)));
        html.push_str(&format!(
            "<div><i>{} · {}</i></div>",
            escape(&sighting.book_name),
            sighting.captured_on()
        ));
        // Only a Note that was actually written. One still pending would put
        // the tool's waiting on the reader's card, which says nothing.
        if let (NoteState::Ready, Some(note)) = (sighting.note_state, &sighting.note) {
            html.push_str(&format!("<div>{}</div>", escape(note)));
        }
    }

    html
}

/// A sentence copied out of a book can hold anything, and an Anki field is
/// HTML — so `<` in a captured sentence must not become markup.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Why a card didn't go.
///
/// Neither one marks the Word synced: whatever the reason, it stays queued and
/// the next sync tries again. The distinction is only in what the reader is
/// told, and Anki being closed is much the commonest of the two.
pub enum Unpushed {
    /// Anki isn't running. An expected condition, not an error.
    Unavailable(Error),
    /// Anki answered, and the answer was no.
    Refused(Error),
}

/// What a push came back with: the Anki note this Word now has.
pub type Pushed = Result<i64, Unpushed>;

/// A card being pushed. Boxed so [`CardSync`] stays usable behind `dyn`.
pub type BoxedPush = Pin<Box<dyn Future<Output = Pushed> + Send>>;

/// Where cards go. The production implementation speaks AnkiConnect; tests
/// supply a fake, and no test touches the network.
pub trait CardSync: Send + Sync + 'static {
    fn push(&self, card: Card) -> BoxedPush;
}

/// One answer, and which Word at which revision it answers for.
pub struct CardOutcome {
    pub word_id: i64,
    pub revision: i64,
    pub pushed: Pushed,
}

/// The cards being pushed right now.
pub struct Cards {
    sync: Arc<dyn CardSync>,
    handle: Handle,
    in_flight: JoinSet<CardOutcome>,
    turns: Arc<Semaphore>,
}

impl Cards {
    pub fn new(sync: Arc<dyn CardSync>, handle: Handle) -> Self {
        Self {
            sync,
            handle,
            in_flight: JoinSet::new(),
            turns: Arc::new(Semaphore::new(AT_ONCE)),
        }
    }

    /// Whether a sync is already under way.
    ///
    /// Starting a second one over the top of the first would push the same Word
    /// twice, both times as a card Anki has never seen — which is exactly the
    /// duplicate the one-note-per-Word mapping exists to prevent.
    pub fn busy(&self) -> bool {
        !self.in_flight.is_empty()
    }

    pub fn push(&mut self, card: Card) {
        let sync = self.sync.clone();
        let handle = self.handle.clone();
        let turns = self.turns.clone();
        let word_id = card.word_id;
        let revision = card.revision;

        self.in_flight.spawn_on(
            async move {
                // Waits its turn rather than adding to a stampede. The whole
                // batch is spawned at once so that the deadline on the way out
                // applies to all of it; only the asking is rationed.
                let _turn = turns.acquire().await;
                CardOutcome {
                    word_id,
                    revision,
                    pushed: sync.push(card).await,
                }
            },
            &handle,
        );
    }

    /// Every push that has finished since the last look. Never waits, so the
    /// event loop can ask on every pass.
    pub fn collect(&mut self) -> Vec<CardOutcome> {
        let mut finished = Vec::new();
        while let Some(joined) = self.in_flight.try_join_next() {
            keep(joined, &mut finished);
        }
        finished
    }

    /// Wait for everything in flight, but no longer than `patience`.
    ///
    /// Whatever has not answered by then is abandoned rather than waited on —
    /// its Word simply stays queued. This is what makes quitting instant when
    /// Anki is closed and still instant when the connection hangs.
    pub async fn settle(&mut self, patience: Duration) -> Vec<CardOutcome> {
        let mut finished = Vec::new();
        let draining = async {
            while let Some(joined) = self.in_flight.join_next().await {
                keep(joined, &mut finished);
            }
        };
        let _ = tokio::time::timeout(patience, draining).await;
        finished
    }
}

/// A push that panicked leaves its Word queued rather than synced, which is the
/// same place every other failure leaves it.
fn keep(joined: Result<CardOutcome, JoinError>, finished: &mut Vec<CardOutcome>) {
    if let Ok(outcome) = joined {
        finished.push(outcome);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    fn sighting(sentence: &str, note: Option<&str>) -> Sighting {
        Sighting {
            id: 1,
            sentence: sentence.to_string(),
            book_name: "Moby-Dick".to_string(),
            captured_at: Local::now(),
            note: note.map(str::to_string),
            note_state: if note.is_some() {
                NoteState::Ready
            } else {
                NoteState::Pending
            },
        }
    }

    #[test]
    fn a_sentence_carrying_markup_is_not_markup_on_the_card() {
        let back = back(&[], &[sighting("He called it a <thing> & left.", None)]);

        assert!(back.contains("&lt;thing&gt; &amp; left."), "{back}");
    }

    #[test]
    fn a_note_still_pending_is_not_put_on_the_card() {
        let back = back(&[], &[sighting("A great cetacean surfaced.", None)]);

        assert!(back.contains("A great cetacean surfaced."), "{back}");
        assert!(!back.contains("pending"), "{back}");
    }
}
