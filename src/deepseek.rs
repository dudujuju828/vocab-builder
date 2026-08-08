//! The production [`NoteWriter`]: DeepSeek, over its OpenAI-compatible
//! endpoint, per ADR 0004.
//!
//! This is the thin network half of the Note feature. Everything about the
//! lifecycle — queueing, pending state, failure, retry — lives in
//! [`crate::notes`] and does not know that this module exists. Because the
//! endpoint is OpenAI-compatible, moving to another provider is a base URL and
//! a model string rather than a rewrite.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::notes::{BoxedNote, NoteRequest, NoteWriter};

/// The key is read from here and held in memory for the run. It is never
/// written to the configuration file, which is why the configuration file has
/// no place to put it.
const API_KEY: &str = "DEEPSEEK_API_KEY";

const ENDPOINT: &str = "https://api.deepseek.com/chat/completions";
const MODEL: &str = "deepseek-v4-pro";

/// A Note is one or two sentences; this is the ceiling, not the target.
const MOST_TOKENS: u32 = 200;

/// Long enough that a slow answer still arrives, short enough that a hung
/// connection doesn't hold a Note open for the rest of the session.
const PATIENCE: Duration = Duration::from_secs(30);

/// The fixed half of the prompt: identical on every request, and sent ahead of
/// everything that varies so DeepSeek's context cache can serve it.
const INSTRUCTIONS: &str = "\
You are helping a reader who is collecting the words they did not know as they \
read, so that they can learn them later.

You will be given one Word, the Book it was met in, and the sentence it \
appeared in. Reply with one or two sentences saying what that Word is doing in \
that particular sentence: which of its senses is in play, and what it is \
carrying there that a dictionary definition on its own would miss.

The reader already has the dictionary definition in front of them, so do not \
restate it. Do not greet them, do not explain what you are about to do, do not \
use headings or lists, and do not quote the sentence back. Reply with the \
reading itself and nothing else.";

pub struct DeepSeek {
    client: Client,
    key: String,
}

impl DeepSeek {
    /// The writer, or `None` when there is no key in the environment.
    ///
    /// `None` is not an error: it disables Note generation for the run, which
    /// the tool says once at launch rather than on every capture.
    pub fn from_environment() -> Option<Arc<dyn NoteWriter>> {
        let key = std::env::var(API_KEY).ok()?;
        if key.trim().is_empty() {
            return None;
        }
        let client = Client::builder()
            .timeout(PATIENCE)
            .build()
            .expect("building an HTTP client");
        Some(Arc::new(Self { client, key }))
    }
}

impl NoteWriter for DeepSeek {
    fn write(&self, request: NoteRequest) -> BoxedNote {
        let client = self.client.clone();
        let key = self.key.clone();
        Box::pin(async move { ask(client, key, request).await })
    }
}

async fn ask(client: Client, key: String, request: NoteRequest) -> Result<String> {
    let spelling = request.spelling.clone();
    let answer = client
        .post(ENDPOINT)
        .bearer_auth(key)
        .json(&Ask {
            model: MODEL,
            messages: prompt(&request),
            max_tokens: MOST_TOKENS,
            stream: false,
        })
        .send()
        .await
        .with_context(|| format!("asking DeepSeek about {spelling:?}"))?;

    // The body says far more than the status alone about why a Note failed,
    // and a failed Note is meant to be recoverable rather than mysterious.
    let status = answer.status();
    if !status.is_success() {
        let body = answer.text().await.unwrap_or_default();
        bail!(
            "DeepSeek answered {status} for {spelling:?}: {}",
            body.trim()
        );
    }

    let answer: Answer = answer
        .json()
        .await
        .with_context(|| format!("reading DeepSeek's answer about {spelling:?}"))?;
    let note = answer
        .choices
        .into_iter()
        .next()
        .map(|choice| choice.message.content)
        .unwrap_or_default();
    let note = note.trim().to_string();

    // An empty Note would render as though one had been written. Better to
    // fail, which is visible and retryable.
    ensure!(
        !note.is_empty(),
        "DeepSeek had nothing to say about {spelling:?}"
    );
    Ok(note)
}

/// The prompt, fixed block first and the Sighting's own details last.
///
/// The ordering is the point: everything that does not vary between captures
/// goes ahead of everything that does, so the prefix is byte-identical on every
/// request and DeepSeek's context cache serves it. Cached input is priced far
/// below a miss, so nearly every request after the first should hit.
fn prompt(request: &NoteRequest) -> [Message; 2] {
    [
        Message {
            role: "system",
            content: INSTRUCTIONS.to_string(),
        },
        Message {
            role: "user",
            content: format!(
                "Word: {}\nBook: {}\nSentence: {}",
                request.spelling, request.book_name, request.sentence
            ),
        },
    ]
}

#[derive(Serialize)]
struct Ask<'a> {
    model: &'a str,
    messages: [Message; 2],
    max_tokens: u32,
    stream: bool,
}

#[derive(Serialize)]
struct Message {
    role: &'static str,
    content: String,
}

#[derive(Deserialize)]
struct Answer {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Spoken,
}

#[derive(Deserialize)]
struct Spoken {
    content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(spelling: &str, book: &str, sentence: &str) -> NoteRequest {
        NoteRequest {
            sighting_id: 1,
            spelling: spelling.to_string(),
            book_name: book.to_string(),
            sentence: sentence.to_string(),
        }
    }

    /// The cost reasoning in ADR 0004 rests on the cache hitting, and the cache
    /// only hits while the fixed block stays ahead of the variable one.
    #[test]
    fn the_prompt_starts_with_a_block_that_does_not_vary() {
        let one = prompt(&request(
            "cetacean",
            "Moby-Dick",
            "A great cetacean surfaced.",
        ));
        let other = prompt(&request(
            "sastruga",
            "The Worst Journey",
            "Sastrugi barred the way.",
        ));

        assert_eq!(one[0].role, other[0].role);
        assert_eq!(one[0].content, other[0].content);
    }

    #[test]
    fn the_word_the_book_and_the_sentence_come_last() {
        let prompt = prompt(&request(
            "cetacean",
            "Moby-Dick",
            "A great cetacean surfaced.",
        ));

        let tail = &prompt[1].content;
        assert!(tail.contains("cetacean"), "the Word: {tail}");
        assert!(tail.contains("Moby-Dick"), "the Book: {tail}");
        assert!(
            tail.contains("A great cetacean surfaced."),
            "the sentence: {tail}"
        );

        // None of it leaked into the block that has to stay identical.
        let fixed = &prompt[0].content;
        assert!(
            !fixed.contains("cetacean"),
            "the fixed block varies: {fixed}"
        );
        assert!(
            !fixed.contains("Moby-Dick"),
            "the fixed block varies: {fixed}"
        );
    }
}
