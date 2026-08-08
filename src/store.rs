//! The user database: Books, Words, Sightings, and what the tool remembers
//! between launches.
//!
//! Lives in the OS application-data directory rather than the working
//! directory, so `vocab` behaves identically from any folder.

use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::cards::CardRequest;
use crate::domain::{Book, NoteState, Sighting, Word};
use crate::notes::NoteRequest;

/// What one capture created: the Word it belongs to, and the Sighting itself.
pub struct Captured {
    pub word_id: i64,
    pub sighting_id: i64,
}

/// One Word with everything search matches against.
pub struct CorpusWord {
    pub word_id: i64,
    pub spelling: String,
    pub sentences: Vec<String>,
    pub books: Vec<String>,
}

pub struct Store {
    connection: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let connection = Connection::open(path)
            .with_context(|| format!("opening the database at {}", path.display()))?;
        let store = Self { connection };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        self.connection.execute_batch(
            "PRAGMA foreign_keys = ON;

             CREATE TABLE IF NOT EXISTS books (
                 id         INTEGER PRIMARY KEY,
                 name       TEXT NOT NULL UNIQUE COLLATE NOCASE,
                 created_at TEXT NOT NULL
             );

             -- A Word is never duplicated. That is enforced here, in the schema,
             -- rather than by application logic.
             CREATE TABLE IF NOT EXISTS words (
                 id         INTEGER PRIMARY KEY,
                 spelling   TEXT NOT NULL UNIQUE COLLATE NOCASE,
                 created_at TEXT NOT NULL
             );

             CREATE TABLE IF NOT EXISTS sightings (
                 id          INTEGER PRIMARY KEY,
                 word_id     INTEGER NOT NULL REFERENCES words(id),
                 book_id     INTEGER NOT NULL REFERENCES books(id),
                 sentence    TEXT NOT NULL,
                 captured_at TEXT NOT NULL,
                 note        TEXT,
                 note_state  TEXT NOT NULL DEFAULT 'pending'
             );
             CREATE INDEX IF NOT EXISTS sightings_by_word ON sightings(word_id);

             -- Single row: what the tool remembers between launches.
             CREATE TABLE IF NOT EXISTS app_state (
                 id              INTEGER PRIMARY KEY CHECK (id = 1),
                 current_book_id INTEGER REFERENCES books(id)
             );

             -- What Anki holds for each Word. `changed` counts edits since the
             -- last successful push rather than flagging them, so a Sighting
             -- captured while a push is in the air is not taken off the queue by
             -- the answer to a question asked before it existed. Zero means the
             -- card is up to date.
             CREATE TABLE IF NOT EXISTS sync_state (
                 word_id      INTEGER PRIMARY KEY REFERENCES words(id),
                 anki_note_id INTEGER,
                 changed      INTEGER NOT NULL DEFAULT 1
             );

             -- Every Word has a row, so the revision above is always a real
             -- number rather than an assumed one. This is also how the backlog
             -- left by v1 and v2 — captured long before there was a sync —
             -- arrives already queued.
             INSERT OR IGNORE INTO sync_state (word_id, changed)
                  SELECT id, 1 FROM words;",
        )?;
        Ok(())
    }

    // -- Books ------------------------------------------------------------

    /// Add a Book, or return the existing one of that name.
    pub fn add_book(&self, name: &str) -> Result<i64> {
        let name = name.trim();
        if let Some(book) = self.find_book(name)? {
            return Ok(book.id);
        }
        self.connection.execute(
            "INSERT INTO books (name, created_at) VALUES (?1, ?2)",
            params![name, now_string()],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn find_book(&self, name: &str) -> Result<Option<Book>> {
        let name = name.trim();
        // A Library is a handful of Books, so scanning the list it already
        // knows how to build beats a second query that counts Words again.
        Ok(self
            .books()?
            .into_iter()
            .find(|book| book.name.eq_ignore_ascii_case(name)))
    }

    /// Every Book, with how many distinct Words came from each.
    pub fn books(&self) -> Result<Vec<Book>> {
        let mut statement = self.connection.prepare(
            "SELECT books.id, books.name, COUNT(DISTINCT sightings.word_id)
               FROM books
               LEFT JOIN sightings ON sightings.book_id = books.id
              GROUP BY books.id
              ORDER BY books.name COLLATE NOCASE",
        )?;
        let books = statement
            .query_map([], |row| {
                Ok(Book {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    word_count: row.get::<_, i64>(2)? as usize,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(books)
    }

    // -- The Current Book -------------------------------------------------

    pub fn current_book(&self) -> Result<Option<Book>> {
        let id: Option<i64> = self
            .connection
            .query_row(
                "SELECT current_book_id FROM app_state WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .optional()?
            .flatten();

        let Some(id) = id else { return Ok(None) };
        Ok(self.books()?.into_iter().find(|book| book.id == id))
    }

    pub fn set_current_book(&self, book_id: i64) -> Result<()> {
        self.connection.execute(
            "INSERT INTO app_state (id, current_book_id) VALUES (1, ?1)
             ON CONFLICT(id) DO UPDATE SET current_book_id = excluded.current_book_id",
            [book_id],
        )?;
        Ok(())
    }

    // -- Words and Sightings ----------------------------------------------

    pub fn find_word(&self, spelling: &str) -> Result<Option<Word>> {
        Ok(self
            .connection
            .query_row(
                "SELECT id, spelling FROM words WHERE spelling = ?1 COLLATE NOCASE",
                [spelling.trim()],
                to_word,
            )
            .optional()?)
    }

    pub fn word(&self, word_id: i64) -> Result<Option<Word>> {
        Ok(self
            .connection
            .query_row(
                "SELECT id, spelling FROM words WHERE id = ?1",
                [word_id],
                to_word,
            )
            .optional()?)
    }

    /// Record an encounter, creating the Word if this is the first one.
    ///
    /// The Note is left pending. Capture never waits on one, and a pending row
    /// is exactly what the background writer picks up — now if it is running,
    /// on the next launch if it is not.
    pub fn capture(&self, spelling: &str, book_id: i64, sentence: &str) -> Result<Captured> {
        let spelling = spelling.trim();
        let word_id = match self.find_word(spelling)? {
            Some(word) => word.id,
            None => {
                self.connection.execute(
                    "INSERT INTO words (spelling, created_at) VALUES (?1, ?2)",
                    params![spelling, now_string()],
                )?;
                self.connection.last_insert_rowid()
            }
        };

        self.connection.execute(
            "INSERT INTO sightings (word_id, book_id, sentence, captured_at, note_state)
             VALUES (?1, ?2, ?3, ?4, 'pending')",
            params![word_id, book_id, sentence.trim(), now_string()],
        )?;

        // The card's back carries every Sighting, so this one has changed it.
        self.mark_card_changed(word_id)?;

        Ok(Captured {
            word_id,
            sighting_id: self.connection.last_insert_rowid(),
        })
    }

    /// Every Sighting of a Word, most recent first.
    pub fn sightings(&self, word_id: i64) -> Result<Vec<Sighting>> {
        let mut statement = self.connection.prepare(
            "SELECT sightings.id, sightings.sentence, books.name,
                    sightings.captured_at, sightings.note, sightings.note_state
               FROM sightings
               JOIN books ON books.id = sightings.book_id
              WHERE sightings.word_id = ?1
              ORDER BY sightings.captured_at DESC, sightings.id DESC",
        )?;

        let sightings = statement
            .query_map([word_id], |row| {
                let captured_at: String = row.get(3)?;
                let note_state: String = row.get(5)?;
                Ok(Sighting {
                    id: row.get(0)?,
                    sentence: row.get(1)?,
                    book_name: row.get(2)?,
                    captured_at: parse_time(&captured_at),
                    note: row.get(4)?,
                    note_state: NoteState::from_stored(&note_state),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(sightings)
    }

    // -- Notes ------------------------------------------------------------

    /// Every Sighting still waiting for a Note, oldest first.
    ///
    /// This is the backlog: what was queued when the tool last closed, in the
    /// order it was captured.
    pub fn pending_notes(&self) -> Result<Vec<NoteRequest>> {
        let mut statement = self.connection.prepare(&format!(
            "{NOTE_REQUEST} WHERE sightings.note_state = 'pending'
              ORDER BY sightings.captured_at, sightings.id"
        ))?;
        let requests = statement
            .query_map([], to_note_request)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(requests)
    }

    /// What a writer needs in order to read one Sighting.
    pub fn note_request(&self, sighting_id: i64) -> Result<Option<NoteRequest>> {
        Ok(self
            .connection
            .query_row(
                &format!("{NOTE_REQUEST} WHERE sightings.id = ?1"),
                [sighting_id],
                to_note_request,
            )
            .optional()?)
    }

    /// Record the reading a writer came back with.
    pub fn write_note(&self, sighting_id: i64, note: &str) -> Result<()> {
        self.connection.execute(
            "UPDATE sightings SET note = ?2, note_state = 'ready' WHERE id = ?1",
            params![sighting_id, note.trim()],
        )?;
        // The Note goes on the card, so the card is now out of date. A Note
        // arriving after a sync is the ordinary case, not an unusual one: it is
        // written in the background, well after the Sighting it belongs to.
        self.connection.execute(
            "UPDATE sync_state SET changed = changed + 1
              WHERE word_id = (SELECT word_id FROM sightings WHERE id = ?1)",
            [sighting_id],
        )?;
        Ok(())
    }

    /// Mark an attempt as having errored. The Sighting itself is untouched —
    /// the network can never cost a capture.
    pub fn fail_note(&self, sighting_id: i64) -> Result<()> {
        self.connection.execute(
            "UPDATE sightings SET note_state = 'failed' WHERE id = ?1",
            [sighting_id],
        )?;
        Ok(())
    }

    /// Put a Sighting back in the queue, clearing whatever Note it had.
    ///
    /// This is the one path that discards a Note that was written successfully,
    /// and it is only ever reached through `/explain`.
    pub fn queue_note(&self, sighting_id: i64) -> Result<()> {
        self.connection.execute(
            "UPDATE sightings SET note = NULL, note_state = 'pending' WHERE id = ?1",
            [sighting_id],
        )?;
        Ok(())
    }

    // -- Anki sync --------------------------------------------------------

    /// Every Word whose card is out of date, oldest first.
    ///
    /// This is the sync queue. A Word is on it from the moment it is captured
    /// until a push for the revision it was read at comes back, so nothing is
    /// ever taken off the queue on the strength of an attempt that failed.
    pub fn changed_cards(&self) -> Result<Vec<CardRequest>> {
        let mut statement = self.connection.prepare(
            "SELECT words.id, words.spelling, sync_state.anki_note_id, sync_state.changed
               FROM words
               JOIN sync_state ON sync_state.word_id = words.id
              WHERE sync_state.changed != 0
              ORDER BY words.id",
        )?;
        let requests = statement
            .query_map([], |row| {
                Ok(CardRequest {
                    word_id: row.get(0)?,
                    spelling: row.get(1)?,
                    anki_note_id: row.get(2)?,
                    revision: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(requests)
    }

    /// Note that a Word's card no longer says what the Word says.
    ///
    /// The upsert is for the Word that has only just been inserted, which has
    /// no row of its own yet.
    pub fn mark_card_changed(&self, word_id: i64) -> Result<()> {
        self.connection.execute(
            "INSERT INTO sync_state (word_id, changed) VALUES (?1, 1)
             ON CONFLICT(word_id) DO UPDATE SET changed = sync_state.changed + 1",
            [word_id],
        )?;
        Ok(())
    }

    /// Record which Anki note a Word now has.
    ///
    /// The identifier is kept whatever else happened — it is true from now on,
    /// and it is what stops the next push creating a second card. The Word only
    /// comes off the queue if nothing has changed it since `revision` was read;
    /// a Sighting captured while the push was in the air leaves it queued, and
    /// the next sync carries what arrived in the meantime.
    pub fn mark_synced(&self, word_id: i64, anki_note_id: i64, revision: i64) -> Result<()> {
        self.connection.execute(
            "UPDATE sync_state
                SET anki_note_id = ?2,
                    changed = CASE WHEN changed = ?3 THEN 0 ELSE changed END
              WHERE word_id = ?1",
            params![word_id, anki_note_id, revision],
        )?;
        Ok(())
    }

    /// Every Word with the sentences and Book names attached to it.
    ///
    /// The corpus is thousands of rows at most, so search rebuilds against
    /// memory rather than maintaining an index.
    pub fn corpus(&self) -> Result<Vec<CorpusWord>> {
        let mut statement = self.connection.prepare(
            "SELECT words.id, words.spelling, sightings.sentence, books.name
               FROM words
               LEFT JOIN sightings ON sightings.word_id = words.id
               LEFT JOIN books ON books.id = sightings.book_id
              ORDER BY words.id",
        )?;

        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut corpus: Vec<CorpusWord> = Vec::new();
        for (word_id, spelling, sentence, book) in rows {
            if corpus.last().map(|word| word.word_id) != Some(word_id) {
                corpus.push(CorpusWord {
                    word_id,
                    spelling,
                    sentences: Vec::new(),
                    books: Vec::new(),
                });
            }
            let word = corpus.last_mut().expect("just pushed");
            if let Some(sentence) = sentence {
                word.sentences.push(sentence);
            }
            if let Some(book) = book
                && !word.books.contains(&book)
            {
                word.books.push(book);
            }
        }

        Ok(corpus)
    }
}

/// The columns a [`NoteRequest`] is built from, shared by the two queries that
/// select one so they cannot drift apart.
const NOTE_REQUEST: &str = "SELECT sightings.id, words.spelling, sightings.sentence, books.name
       FROM sightings
       JOIN words ON words.id = sightings.word_id
       JOIN books ON books.id = sightings.book_id";

fn to_note_request(row: &Row<'_>) -> rusqlite::Result<NoteRequest> {
    Ok(NoteRequest {
        sighting_id: row.get(0)?,
        spelling: row.get(1)?,
        sentence: row.get(2)?,
        book_name: row.get(3)?,
    })
}

fn to_word(row: &Row<'_>) -> rusqlite::Result<Word> {
    Ok(Word {
        id: row.get(0)?,
        spelling: row.get(1)?,
    })
}

fn now_string() -> String {
    Local::now().to_rfc3339()
}

/// Timestamps are only ever written by [`now_string`], so an unparseable one
/// means a hand-edited database. Falling back to now keeps the Sighting
/// readable rather than failing the whole screen over a display detail.
fn parse_time(value: &str) -> DateTime<Local> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.with_timezone(&Local))
        .unwrap_or_else(|_| Local::now())
}
