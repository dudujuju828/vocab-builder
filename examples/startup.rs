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

    for label in ["cold (unpacks the dictionary)", "warm"] {
        let start = Instant::now();

        let store = vocab::Store::open(&directory.join("vocab.db"))?;
        let dictionary = vocab::Dictionary::unpack_into(&directory)?;
        let _app = vocab::App::new(store, dictionary)?;

        println!("{label}: {:?}", start.elapsed());
    }

    let _ = std::fs::remove_dir_all(&directory);
    Ok(())
}
