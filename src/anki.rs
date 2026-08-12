//! The production [`CardSync`]: AnkiConnect, over HTTP on the local machine.
//!
//! Not MCP — that is a Claude-side mechanism and is not available to a
//! standalone binary. Vocab posts to AnkiConnect's own endpoint, which means
//! Anki has to be running for a sync to succeed.
//!
//! **Anki not running is an expected condition, not an error.** It is the
//! normal state of the machine most of the time. The push comes back
//! [`Unpushed::Unavailable`], the Word stays queued, and nothing about it ever
//! surfaces as a crash or holds up quitting.
//!
//! This is the thin network half of the feature. What a card carries, and which
//! Words are due, is decided in [`crate::cards`] and does not know this module
//! exists.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Error, Result, anyhow};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::cards::{BoxedPush, BoxedRemoval, Card, CardSync, Pushed, Removed, Unpushed};

/// AnkiConnect listens here and nowhere else. The literal address rather than
/// `localhost`, so a machine that resolves that to IPv6 first doesn't pay for a
/// refused connection before trying the one that works.
const ENDPOINT: &str = "http://127.0.0.1:8765";

/// The version of AnkiConnect's protocol these actions belong to.
const PROTOCOL: u8 = 6;

/// Anki's stock two-field note type. Front and Back are its field names, and a
/// card built on it renders without the reader having to set anything up.
const MODEL: &str = "Basic";

/// Put on every note written, so the reader can tell in Anki which cards came
/// from here.
const TAG: &str = "vocab";

/// Anki is on the same machine, so an answer is either immediate or not coming.
/// This is only a backstop against a wedged Anki holding a push open.
const PATIENCE: Duration = Duration::from_secs(10);

pub struct Anki {
    client: Client,
}

impl Anki {
    /// Anki is expected on this machine and nowhere else — there is no address
    /// to configure, because AnkiConnect only ever listens locally.
    pub fn on_this_machine() -> Result<Arc<dyn CardSync>> {
        let client = Client::builder()
            .timeout(PATIENCE)
            .build()
            .map_err(|error| Error::new(error).context("building the HTTP client for Anki"))?;
        Ok(Arc::new(Self { client }))
    }
}

impl CardSync for Anki {
    fn push(&self, card: Card) -> BoxedPush {
        let client = self.client.clone();
        Box::pin(async move {
            match card.anki_note_id {
                // Already has a note: update it in place, so a Word that gained
                // a Sighting enriches its card rather than growing a second.
                Some(id) => update(&client, id, &card).await,
                None => create(&client, &card).await,
            }
        })
    }

    fn remove(&self, anki_note_id: i64) -> BoxedRemoval {
        let client = self.client.clone();
        Box::pin(async move { delete(&client, anki_note_id).await })
    }
}

async fn create(client: &Client, card: &Card) -> Pushed {
    // Anki refuses a note for a deck it doesn't have, and creating one that is
    // already there is a no-op — so this costs a round trip on the same machine
    // and saves the reader a setup step.
    ask::<_, Option<i64>>(
        client,
        Request {
            action: "createDeck",
            version: PROTOCOL,
            params: DeckParams { deck: &card.deck },
        },
    )
    .await?;

    let created = ask::<_, i64>(
        client,
        Request {
            action: "addNote",
            version: PROTOCOL,
            params: NoteParams {
                note: NewNote {
                    deck_name: &card.deck,
                    model_name: MODEL,
                    fields: Fields {
                        front: &card.front,
                        back: &card.back,
                    },
                    tags: [TAG],
                    options: Options {
                        // The reader may already have their own card for this
                        // Word. Ours is identified by the note we remember, not
                        // by its front, so a clash is not our business.
                        allow_duplicate: true,
                    },
                },
            },
        },
    )
    .await?;

    created.ok_or_else(|| Unpushed::Refused(anyhow!("Anki added the note but named no identifier")))
}

async fn update(client: &Client, id: i64, card: &Card) -> Pushed {
    // The fields only — the deck a note lives in is the reader's to change, and
    // the sync is one way about content, not about where they filed it.
    let asked = ask::<_, Option<i64>>(
        client,
        Request {
            action: "updateNoteFields",
            version: PROTOCOL,
            params: NoteParams {
                note: ExistingNote {
                    id,
                    fields: Fields {
                        front: &card.front,
                        back: &card.back,
                    },
                },
            },
        },
    )
    .await;

    match asked {
        Ok(_) => Ok(id),
        // The reader deleted the card. The identifier we remember is stale and
        // nothing done to it will ever work again, so the Word is written back
        // as a new note — otherwise it would sit in the queue for good, failing
        // the same way on every sync with no way for the reader to clear it.
        Err(Unpushed::Refused(complaint)) if note_is_gone(&complaint.to_string()) => {
            create(client, card).await
        }
        Err(unpushed) => Err(unpushed),
    }
}

/// Take out the note a removed Word left behind.
///
/// Anki deletes the cards along with the note, which is the whole point: a Word
/// the reader has got rid of should stop coming up for review.
async fn delete(client: &Client, anki_note_id: i64) -> Removed {
    let asked = ask::<_, ()>(
        client,
        Request {
            action: "deleteNotes",
            version: PROTOCOL,
            params: NotesParams {
                notes: [anki_note_id],
            },
        },
    )
    .await;

    match asked {
        Ok(_) => Ok(()),
        // Already gone — the reader deleted it in Anki themselves. A removal
        // asks for exactly this state of affairs, so it has got what it came
        // for; anything else would leave the queue failing the same way for
        // ever with no way to clear it. The mirror of [`update`], which reads
        // the same complaint as a reason to write the card back.
        Err(Unpushed::Refused(complaint)) if note_is_gone(&complaint.to_string()) => Ok(()),
        Err(unpushed) => Err(unpushed),
    }
}

/// Whether Anki's complaint means the note we remember is no longer there.
///
/// AnkiConnect answers with prose rather than a code, so this has to read the
/// prose. It is deliberately narrow: anything it doesn't recognise leaves the
/// Word queued, which is the safe direction to be wrong in — the alternative
/// would be writing a second card for a Word that already has one.
fn note_is_gone(complaint: &str) -> bool {
    let complaint = complaint.to_lowercase();
    complaint.contains("note was not found") || complaint.contains("cannot find note")
}

/// Post one action and read what came back.
///
/// The two failure kinds are the whole of this function. No answer at all means
/// Anki isn't there — the ordinary state of the machine, and the Word waits.
/// An answer that isn't a result means Anki was asked something it wouldn't do,
/// which is worth saying rather than retrying silently forever. Neither one
/// marks the Word synced.
async fn ask<P: Serialize, R: for<'de> Deserialize<'de>>(
    client: &Client,
    request: Request<P>,
) -> Result<Option<R>, Unpushed> {
    let action = request.action;

    let sent = client
        .post(ENDPOINT)
        .json(&request)
        .send()
        .await
        .map_err(|error| {
            Unpushed::Unavailable(Error::new(error).context(format!("asking Anki to {action}")))
        })?;

    let status = sent.status();
    if !status.is_success() {
        let body = sent.text().await.unwrap_or_default();
        return Err(Unpushed::Refused(anyhow!(
            "Anki answered {status} to {action}: {}",
            body.trim()
        )));
    }

    let answer: Answer<R> = sent.json().await.map_err(|error| {
        Unpushed::Refused(Error::new(error).context(format!("reading Anki's answer to {action}")))
    })?;

    if let Some(complaint) = answer.error {
        return Err(Unpushed::Refused(anyhow!(
            "Anki wouldn't {action}: {complaint}"
        )));
    }
    Ok(answer.result)
}

#[derive(Serialize)]
struct Request<P> {
    action: &'static str,
    version: u8,
    params: P,
}

#[derive(Serialize)]
struct DeckParams<'a> {
    deck: &'a str,
}

#[derive(Serialize)]
struct NoteParams<N> {
    note: N,
}

/// `deleteNotes` takes a list, though we only ever hand it the one — a removal
/// belongs to one Word, and batching them would lose which of them failed.
#[derive(Serialize)]
struct NotesParams {
    notes: [i64; 1],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NewNote<'a> {
    deck_name: &'a str,
    model_name: &'static str,
    fields: Fields<'a>,
    tags: [&'static str; 1],
    options: Options,
}

#[derive(Serialize)]
struct ExistingNote<'a> {
    id: i64,
    fields: Fields<'a>,
}

/// Anki's own field names on the Basic note type, capitalised as it spells them.
#[derive(Serialize)]
struct Fields<'a> {
    #[serde(rename = "Front")]
    front: &'a str,
    #[serde(rename = "Back")]
    back: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Options {
    allow_duplicate: bool,
}

/// Every AnkiConnect answer has this shape: one of the two is always null.
#[derive(Deserialize)]
struct Answer<R> {
    result: Option<R>,
    error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What AnkiConnect actually says when the note has been deleted, wrapped
    /// the way [`ask`] wraps it.
    #[test]
    fn a_deleted_note_is_recognised_from_what_anki_says() {
        assert!(note_is_gone(
            "Anki wouldn't updateNoteFields: note was not found: 1700000000000"
        ));
        assert!(note_is_gone("Cannot find note with id 1700000000000"));
    }

    /// Anything else leaves the Word queued rather than writing it a second
    /// card, because a Word with two cards is the one thing this mapping exists
    /// to prevent.
    #[test]
    fn any_other_complaint_leaves_the_word_queued() {
        assert!(!note_is_gone(
            "Anki wouldn't updateNoteFields: collection is not available"
        ));
        assert!(!note_is_gone(
            "Anki wouldn't addNote: deck was not found: Vocab"
        ));
        assert!(!note_is_gone(
            "Anki wouldn't updateNoteFields: model was not found"
        ));
    }
}
