//! Measures what `vocab` does before it can draw its first frame.
//!
//! The tool is launched constantly and briefly, mid-page, so startup latency is
//! felt directly — the spec asks for under a tenth of a second. The bundled
//! dictionary is the only part heavy enough to threaten that, and it is paid
//! for once, on the first run.
//!
//!     cargo run --release --example startup

use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let directory = std::env::temp_dir().join("vocab-startup-probe");
    let _ = std::fs::remove_dir_all(&directory);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()?;

    for label in ["cold (unpacks the dictionary)", "warm"] {
        let start = Instant::now();

        let store = vocab::Store::open(&directory.join("vocab.db"))?;
        let dictionary = vocab::Dictionary::unpack_into(&directory)?;
        let notes = vocab::Notes::new(None, runtime.handle().clone());
        let cards = {
            let _inside = runtime.enter();
            vocab::Cards::new(vocab::Anki::on_this_machine()?, runtime.handle().clone())
        };
        let config = vocab::Config::load_or_create(&directory.join("config.toml"));
        let _app = vocab::App::new(store, dictionary, notes, cards, config)?;

        println!("{label}: {:?}", start.elapsed());
    }

    let _ = std::fs::remove_dir_all(&directory);
    Ok(())
}
