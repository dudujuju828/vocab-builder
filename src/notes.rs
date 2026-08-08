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

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use tokio::runtime::Handle;
use tokio::task::JoinSet;

/// Everything a writer needs to read one Word in one sentence.
#[derive(Debug, Clone)]
pub struct NoteRequest {
    pub sighting_id: i64,
    pub spelling: String,
    pub sentence: String,
    pub book_name: String,
}

/// A Note being written. Boxed so [`NoteWriter`] stays usable behind `dyn`.
pub type BoxedNote = Pin<Box<dyn Future<Output = Result<String>> + Send>>;

/// Where Notes come from. The production implementation calls DeepSeek; tests
/// supply a stub, and no test touches the network.
pub trait NoteWriter: Send + Sync + 'static {
    fn write(&self, request: NoteRequest) -> BoxedNote;
}

/// What a writer came back with, and which Sighting it was for.
pub struct NoteOutcome {
    pub sighting_id: i64,
    pub note: Result<String>,
}

/// The Notes being written right now.
pub struct Notes {
    /// Absent when there is no API key. The queue then simply stands still:
    /// Sightings stay pending and are written whenever a key turns up.
    writer: Option<Arc<dyn NoteWriter>>,
    handle: Handle,
    in_flight: JoinSet<NoteOutcome>,
}

impl Notes {
    pub fn new(writer: Option<Arc<dyn NoteWriter>>, handle: Handle) -> Self {
        Self {
            writer,
            handle,
            in_flight: JoinSet::new(),
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
        self.in_flight.spawn_on(
            async move {
                NoteOutcome {
                    sighting_id,
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
            // A writer that panicked leaves its Sighting pending rather than
            // failed, so it is picked up again on the next launch.
            if let Ok(outcome) = joined {
                finished.push(outcome);
            }
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
            if let Ok(outcome) = joined {
                finished.push(outcome);
            }
        }
        finished
    }
}
