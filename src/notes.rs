//! Notes: the AI's short reading of what a Word is doing in one particular
//! Sighting's sentence.
//!
//! A Note is a second opinion beside the Definition, never a replacement for it,
//! and capture never waits on one. `/add` stores its Sighting with the Note
//! pending and returns; the writing happens out here, and the result is applied
//! back on the application's own thread — which is why nothing in this module
//! touches the database.
//!
//! Because the state is stored rather than inferred, a Note that was pending
//! when the process exited is still pending when it starts again. Reading on a
//! train leaves a queue that drains when the network comes back.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Error;
use tokio::runtime::Handle;
use tokio::task::{JoinError, JoinSet};

/// Everything a writer needs to read one Word in one sentence.
#[derive(Debug, Clone)]
pub struct NoteRequest {
    pub sighting_id: i64,
    pub spelling: String,
    pub sentence: String,
    pub book_name: String,
}

/// Why a Note wasn't written.
///
/// The distinction is the whole of capturing offline. A writer that could not
/// be reached will be reachable later, so its Sighting is left queued and
/// nothing is lost by having asked early. A writer that answered and said no is
/// the reader's to ask again.
pub enum Unwritten {
    /// Never got through — no network, or no answer at all. The Sighting stays
    /// pending, and a later launch picks it up.
    Unreachable(Error),
    /// Reached, and the answer was not a Note. The Sighting is marked failed,
    /// which is visible and retryable through `/explain`.
    Refused(Error),
}

/// What a writer came back with.
pub type Written = Result<String, Unwritten>;

/// A Note being written. Boxed so [`NoteWriter`] stays usable behind `dyn`.
pub type BoxedNote = Pin<Box<dyn Future<Output = Written> + Send>>;

/// Where Notes come from. The production implementation calls DeepSeek; tests
/// supply a stub, and no test touches the network.
pub trait NoteWriter: Send + Sync + 'static {
    fn write(&self, request: NoteRequest) -> BoxedNote;
}

/// One answer, and which asking it answers.
pub struct NoteOutcome {
    pub sighting_id: i64,
    pub note: Written,
    attempt: u64,
}

/// The Notes being written right now.
pub struct Notes {
    /// Absent when there is no API key. The queue then simply stands still:
    /// Sightings stay pending and are written whenever a key turns up.
    writer: Option<Arc<dyn NoteWriter>>,
    handle: Handle,
    in_flight: JoinSet<NoteOutcome>,
    /// Which asking each Sighting is on. `/explain` can ask again while an
    /// earlier answer is still in the air, and answers come back in whatever
    /// order they finish — so an answer to a question already replaced is
    /// dropped rather than allowed to overwrite the Note that replaced it.
    attempt: HashMap<i64, u64>,
    asked: u64,
}

impl Notes {
    pub fn new(writer: Option<Arc<dyn NoteWriter>>, handle: Handle) -> Self {
        Self {
            writer,
            handle,
            in_flight: JoinSet::new(),
            attempt: HashMap::new(),
            asked: 0,
        }
    }

    /// Whether Notes are being written at all.
    pub fn are_on(&self) -> bool {
        self.writer.is_some()
    }

    pub fn enqueue(&mut self, request: NoteRequest) {
        let Some(writer) = self.writer.clone() else {
            return;
        };
        let handle = self.handle.clone();
        let sighting_id = request.sighting_id;

        self.asked += 1;
        let attempt = self.asked;
        self.attempt.insert(sighting_id, attempt);

        self.in_flight.spawn_on(
            async move {
                NoteOutcome {
                    sighting_id,
                    attempt,
                    note: writer.write(request).await,
                }
            },
            &handle,
        );
    }

    /// Every Note that has finished since the last look. Never waits, so the
    /// event loop can ask on every pass.
    pub fn collect(&mut self) -> Vec<NoteOutcome> {
        let mut finished = Vec::new();
        while let Some(joined) = self.in_flight.try_join_next() {
            self.keep(joined, &mut finished);
        }
        finished
    }

    /// Wait for every Note in flight.
    ///
    /// Tests drive the background work to completion through this rather than
    /// sleeping or polling.
    pub async fn settle(&mut self) -> Vec<NoteOutcome> {
        let mut finished = Vec::new();
        while let Some(joined) = self.in_flight.join_next().await {
            self.keep(joined, &mut finished);
        }
        finished
    }

    /// Take an answer, unless it has been overtaken.
    fn keep(&mut self, joined: Result<NoteOutcome, JoinError>, finished: &mut Vec<NoteOutcome>) {
        // A writer that panicked leaves its Sighting pending rather than
        // failed, so it is picked up again on the next launch.
        let Ok(outcome) = joined else { return };

        if self.attempt.get(&outcome.sighting_id) != Some(&outcome.attempt) {
            return; // Asked again since. This answers a question already gone.
        }
        self.attempt.remove(&outcome.sighting_id);
        finished.push(outcome);
    }
}
