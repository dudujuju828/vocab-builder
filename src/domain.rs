//! The vocabulary of the tool, as defined in CONTEXT.md.

use chrono::{DateTime, Local};

/// A source you are reading and capturing Words from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Book {
    pub id: i64,
    pub name: String,
    /// How many Words have been captured from this Book.
    pub word_count: usize,
}

/// A single lexical item you didn't know, unique by spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub id: i64,
    pub spelling: String,
}

/// One encounter with a Word: the sentence, the Book, and the date.
#[derive(Debug, Clone)]
pub struct Sighting {
    pub id: i64,
    pub sentence: String,
    pub book_name: String,
    pub captured_at: DateTime<Local>,
    /// The AI's reading of this Word in this sentence. Written in v2; always
    /// `None` until then, with every Sighting left [`NoteState::Pending`] so the
    /// backlog drains once a `NoteWriter` exists.
    pub note: Option<String>,
    pub note_state: NoteState,
}

impl Sighting {
    /// The capture date as a reader sees it.
    pub fn captured_on(&self) -> String {
        self.captured_at.format("%Y-%m-%d").to_string()
    }
}

/// Stored rather than inferred, so a Note that was pending when the process
/// exited is still pending when it starts again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteState {
    Pending,
    Ready,
    Failed,
}

impl NoteState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }

    /// Read a state back out of the database.
    ///
    /// Unrecognised values are treated as pending: a Note we cannot classify is
    /// one worth attempting again, and the Sighting itself is never at risk.
    pub fn from_stored(value: &str) -> Self {
        match value {
            "ready" => Self::Ready,
            "failed" => Self::Failed,
            _ => Self::Pending,
        }
    }
}

/// What the bundled dictionary says a Word means. A Word may have several.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sense {
    pub part_of_speech: String,
    pub definition: String,
}
