//! Asks DeepSeek for one real Note, over the network, and prints what came
//! back.
//!
//! The network half of the Note feature is the one part the test suite cannot
//! reach: [`crate::deepseek`] is unit-tested on the shapes an answer arrives in,
//! never on whether an answer arrives at all. So a key that has expired, a model
//! string that no longer names a model, or a request the endpoint has stopped
//! accepting all look exactly like a healthy build until a reader captures a
//! word and no Note appears. This is how to tell those apart on purpose.
//!
//!     cargo run --example note
//!
//! Reads `DEEPSEEK_API_KEY` from the environment like the tool does, and costs
//! one Note's worth of tokens to run.

use vocab::deepseek::DeepSeek;
use vocab::notes::{NoteRequest, Unwritten};

fn main() -> anyhow::Result<()> {
    let Some(writer) = DeepSeek::from_environment() else {
        anyhow::bail!(
            "no DEEPSEEK_API_KEY in the environment, so the tool would run with \
             Notes turned off"
        );
    };

    let request = NoteRequest {
        sighting_id: 1,
        spelling: "cetacean".to_string(),
        book_name: "Moby-Dick".to_string(),
        sentence: "A great cetacean surfaced.".to_string(),
    };
    println!("asking about {:?}...", request.spelling);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    // Which failure it was is the whole answer: unreachable means try again on a
    // launch with a network behind it, refused means the key, the model or the
    // request itself is wrong and no amount of retrying will fix it.
    match runtime.block_on(writer.write(request)) {
        Ok(note) => println!("\nDeepSeek wrote:\n{note}"),
        Err(Unwritten::Unreachable(why)) => anyhow::bail!("never got through: {why:?}"),
        Err(Unwritten::Refused(why)) => anyhow::bail!("answered, but not with a Note: {why:?}"),
    }
    Ok(())
}
