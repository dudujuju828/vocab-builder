//! The user database: Books, Words, Sightings, and what the tool remembers
//! between launches.
//!
//! Lives in the OS application-data directory rather than the working
//! directory, so `vocab` behaves identically from any folder.

use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use rusqlite::{Connection, OptionalExtension, params};

use crate::domain::{Book, NoteState, Sighting, Word};

/// Everything about one Word that search matches against.
pub struct CorpusEntry {
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

    #[cfg(test)]
    pub fn in_memory() -> Result<Self> {
        let store = Self {
            connection: Connection::open_in_memory()?,
        };
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

             -- Populated by the Anki sync in v3. A Word with no row here has
             -- never been pushed.
             CREATE TABLE IF NOT EXISTS sync_state (
                 word_id      INTEGER PRIMARY KEY REFERENCES words(id),
                 anki_note_id INTEGER,
                 changed      INTEGER NOT NULL DEFAULT 1
             );",
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
        let book = self
            .connection
            .query_row(
                "SELECT id, name FROM books WHERE name = ?1 COLLATE NOCASE",
                [name.trim()],
                |row| {
                    Ok(Book {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        word_count: 0,
                    })
                },
            )
            .optional()?;

        match book {
            Some(mut book) => {
                book.word_count = self.word_count(book.id)?;
                Ok(Some(book))
            }
            None => Ok(None),
        }
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

    fn word_count(&self, book_id: i64) -> Result<usize> {
        let count: i64 = self.connection.query_row(
            "SELECT COUNT(DISTINCT word_id) FROM sightings WHERE book_id = ?1",
            [book_id],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    // -- The Current Book -------------------------------------------------

    pub fn current_book(&self) -> Result<Option<Book>> {
        let id: Option<i64> = self
            .connection
            .query_row("SELECT current_book_id FROM app_state WHERE id = 1", [], |row| {
                row.get(0)
            })
            .optional()?
            .flatten();

        let Some(id) = id else { return Ok(None) };

        let book = self
            .connection
            .query_row("SELECT id, name FROM books WHERE id = ?1", [id], |row| {
                Ok(Book {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    word_count: 0,
                })
            })
            .optional()?;

        match book {
            Some(mut book) => {
                book.word_count = self.word_count(book.id)?;
                Ok(Some(book))
            }
            None => Ok(None),
        }
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
        let word = self
            .connection
            .query_row(
                "SELECT id, spelling FROM words WHERE spelling = ?1 COLLATE NOCASE",
                [spelling.trim()],
                |row| {
                    Ok(Word {
                        id: row.get(0)?,
                        spelling: row.get(1)?,
                    })
                },
            )
            .optional()?;
        Ok(word)
    }

    pub fn word(&self, word_id: i64) -> Result<Option<Word>> {
        let word = self
            .connection
            .query_row("SELECT id, spelling FROM words WHERE id = ?1", [word_id], |row| {
                Ok(Word {
                    id: row.get(0)?,
                    spelling: row.get(1)?,
                })
            })
            .optional()?;
        Ok(word)
    }

    /// Record an encounter, creating the Word if this is the first one.
    ///
    /// The Note is left pending: no Note is written in v1, and a pending row is
    /// exactly what the v2 background writer picks up on next launch.
    pub fn capture(&self, spelling: &str, book_id: i64, sentence: &str) -> Result<i64> {
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

        // A Word that gains a Sighting has changed, so v3 pushes it again
        // rather than creating a second Anki note.
        self.connection.execute(
            "INSERT INTO sync_state (word_id, changed) VALUES (?1, 1)
             ON CONFLICT(word_id) DO UPDATE SET changed = 1",
            [word_id],
        )?;

        Ok(word_id)
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

    /// Every Word with the sentences and Book names attached to it.
    ///
    /// The corpus is thousands of rows at most, so search rebuilds against
    /// memory rather than maintaining an index.
    pub fn corpus(&self) -> Result<Vec<CorpusEntry>> {
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

        let mut corpus: Vec<CorpusEntry> = Vec::new();
        for (word_id, spelling, sentence, book) in rows {
            if corpus.last().map(|entry| entry.word_id) != Some(word_id) {
                corpus.push(CorpusEntry {
                    word_id,
                    spelling,
                    sentences: Vec::new(),
                    books: Vec::new(),
                });
            }
            let entry = corpus.last_mut().expect("just pushed");
            if let Some(sentence) = sentence {
                entry.sentences.push(sentence);
            }
            if let Some(book) = book
                && !entry.books.contains(&book)
            {
                entry.books.push(book);
            }
        }

        Ok(corpus)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_word_is_never_duplicated() {
        let store = Store::in_memory().unwrap();
        let book = store.add_book("Moby-Dick").unwrap();

        let first = store.capture("pequod", book, "The Pequod sailed.").unwrap();
        let second = store.capture("Pequod", book, "Aboard the Pequod.").unwrap();

        assert_eq!(first, second, "differing case must reach the same Word");
        assert_eq!(store.sightings(first).unwrap().len(), 2);
    }

    #[test]
    fn corpus_groups_sightings_under_one_word() {
        let store = Store::in_memory().unwrap();
        let moby = store.add_book("Moby-Dick").unwrap();
        let dune = store.add_book("Dune").unwrap();
        store.capture("cetacean", moby, "A cetacean of note.").unwrap();
        store.capture("cetacean", dune, "No cetaceans here.").unwrap();

        let corpus = store.corpus().unwrap();

        assert_eq!(corpus.len(), 1);
        assert_eq!(corpus[0].sentences.len(), 2);
        assert_eq!(corpus[0].books, vec!["Moby-Dick", "Dune"]);
    }

    #[test]
    fn a_word_with_no_sightings_still_appears_in_the_corpus() {
        let store = Store::in_memory().unwrap();
        store
            .connection
            .execute(
                "INSERT INTO words (spelling, created_at) VALUES ('orphan', '2026-01-01T00:00:00+00:00')",
                [],
            )
            .unwrap();

        let corpus = store.corpus().unwrap();

        assert_eq!(corpus.len(), 1);
        assert!(corpus[0].sentences.is_empty());
    }
}
