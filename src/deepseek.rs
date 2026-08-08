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

use anyhow::{Error, anyhow};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::notes::{BoxedNote, NoteRequest, NoteWriter, Unwritten, Written};

/// The key is read from this variable and held in memory for the run. It is
/// never written to the configuration file, which is why the configuration file
/// has no place to put it.
const KEY_VARIABLE: &str = "DEEPSEEK_API_KEY";

const ENDPOINT: &str = "https://api.deepseek.com/chat/completions";
const MODEL: &str = "deepseek-v4-pro";

/// A Note is one or two sentences; this is the ceiling, not the target.
///
/// Set far above what a reading actually costs — a measured one came back in
/// 62 — so that a long sentence, or a Word that needs a clause more to place,
/// is never cut off half-read. Unused budget is not billed, so the headroom
/// costs nothing to carry.
const MOST_TOKENS: u32 = 600;

/// Both DeepSeek V4 models reason before they answer, and the reasoning is
/// spent out of the same budget as the reply.
///
/// A one-sentence reading of a word in a sentence does not need a reasoning
/// trace, and paying for one is the difference between a Note costing tens of
/// tokens and hundreds: with it on, the reading above cost 292 reasoning tokens
/// to produce 13 of answer. Turning it off is what keeps a Note the single
/// cheap round trip ADR 0004 costed.
///
/// It is also what [`MOST_TOKENS`] used to be sized against. With the trace on
/// and the ceiling at 200, the trace alone exhausted the budget: the reply came
/// back truncated with a `finish_reason` of `length`, and every Note failed.
/// [`read`] refuses such an answer rather than storing half of one, whatever
/// the ceiling is set to.
const REASONING: &str = "none";

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
that particular sentence: which way it is being used there, and what it is \
carrying that a dictionary definition on its own would miss.

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
    /// the tool says once at launch rather than on every capture. A client that
    /// cannot be built at all — a broken TLS setup — is the same condition, and
    /// is treated the same way rather than taking the offline core down with it.
    pub fn from_environment() -> Option<Arc<dyn NoteWriter>> {
        let key = std::env::var(KEY_VARIABLE).ok()?;
        if key.trim().is_empty() {
            return None;
        }
        let client = Client::builder().timeout(PATIENCE).build().ok()?;
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

async fn ask(client: Client, key: String, request: NoteRequest) -> Written {
    let spelling = request.spelling.clone();
    let sent = client
        .post(ENDPOINT)
        .bearer_auth(key)
        .json(&Ask {
            model: MODEL,
            messages: prompt(&request),
            max_tokens: MOST_TOKENS,
            reasoning_effort: REASONING,
            stream: false,
        })
        .send()
        .await;

    // No answer at all. Whatever the cause, the question is still worth asking,
    // so the Sighting stays queued for a launch with a network behind it —
    // which is what makes capturing on a train cost nothing.
    let answer = sent.map_err(|error| {
        Unwritten::Unreachable(
            Error::new(error).context(format!("reaching DeepSeek about {spelling:?}")),
        )
    })?;

    // Past here DeepSeek has answered, so anything wrong is a refusal: the
    // reader's to ask again rather than the tool's to retry forever. The body
    // says far more than the status alone about which refusal it was.
    let status = answer.status();
    if !status.is_success() {
        let body = answer.text().await.unwrap_or_default();
        return Err(Unwritten::Refused(anyhow!(
            "DeepSeek answered {status} for {spelling:?}: {}",
            body.trim()
        )));
    }

    let answer: Answer = answer.json().await.map_err(|error| {
        Unwritten::Refused(
            Error::new(error).context(format!("reading DeepSeek's answer about {spelling:?}")),
        )
    })?;
    read(answer, &spelling)
}

/// Take the Note out of an answer DeepSeek has already given.
///
/// Separated from the sending so that the shapes an answer can arrive in are
/// testable without a network — which is how the truncation below was found
/// only after it had made every Note fail.
fn read(answer: Answer, spelling: &str) -> Written {
    let Some(choice) = answer.choices.into_iter().next() else {
        return Err(Unwritten::Refused(anyhow!(
            "DeepSeek answered about {spelling:?} without a reply in it"
        )));
    };
    let note = choice.message.content.trim().to_string();

    // The ceiling was reached before the reading was finished. Half a sentence
    // stored as a Note would read as though it were the whole answer, and an
    // empty one would look like a refusal — neither says what went wrong.
    if choice.finish_reason.as_deref() == Some("length") {
        return Err(Unwritten::Refused(anyhow!(
            "DeepSeek ran out of room before finishing its reading of {spelling:?}: \
             {MOST_TOKENS} tokens was not enough"
        )));
    }

    // An empty Note would render as though one had been written. Refusing is
    // visible and retryable; a blank line is neither.
    if note.is_empty() {
        return Err(Unwritten::Refused(anyhow!(
            "DeepSeek had nothing to say about {spelling:?}"
        )));
    }
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
    reasoning_effort: &'a str,
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
    message: Reply,
    /// Absent on some OpenAI-compatible answers, so it is read as an option
    /// rather than required — a missing reason is not a truncated reading.
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct Reply {
    content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(content: &str, finish_reason: &str) -> Answer {
        Answer {
            choices: vec![Choice {
                message: Reply {
                    content: content.to_string(),
                },
                finish_reason: Some(finish_reason.to_string()),
            }],
        }
    }

    /// The failure that made every Note fail: the budget went on a reasoning
    /// trace, and what little reply fitted was half a sentence. Stored as a Note
    /// it would read as the whole answer, so it has to be refused.
    #[test]
    fn a_reading_that_ran_out_of_room_is_not_a_note() {
        let cut_off = answer("In this sentence, \"cetacean\" serves as a", "length");

        let Err(Unwritten::Refused(why)) = read(cut_off, "cetacean") else {
            panic!("a truncated reading was taken for a Note");
        };
        assert!(why.to_string().contains("cetacean"), "which Word: {why}");
    }

    #[test]
    fn a_finished_reading_is_the_note() {
        let Ok(note) = read(
            answer("  It labels the whale coldly.  ", "stop"),
            "cetacean",
        ) else {
            panic!("a finished reading was refused");
        };

        assert_eq!(note, "It labels the whale coldly.");
    }

    /// Nothing to say is refused rather than stored: a blank Note renders as
    /// though one had been written, and cannot be told from a real one.
    #[test]
    fn an_empty_reading_is_not_a_note() {
        assert!(matches!(
            read(answer("   ", "stop"), "cetacean"),
            Err(Unwritten::Refused(_))
        ));
    }

    #[test]
    fn an_answer_with_no_reply_in_it_is_not_a_note() {
        let empty = Answer { choices: vec![] };

        assert!(matches!(
            read(empty, "cetacean"),
            Err(Unwritten::Refused(_))
        ));
    }

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
