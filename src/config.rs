//! Configuration: the small file the reader can edit.
//!
//! It lives in the OS configuration directory, beside nothing else, and is
//! written on first run so that there is something to open rather than a
//! filename to guess at.
//!
//! No secret goes in it. The DeepSeek key that writes Notes is read from the
//! environment, which is why this file has no place to put one.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

/// Where cards land when the reader hasn't said otherwise.
pub const DEFAULT_DECK: &str = "Vocab";

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// The Anki deck cards are written to.
    pub deck: String,
    /// Whether leaving pushes everything changed.
    pub sync_on_exit: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            deck: DEFAULT_DECK.to_string(),
            sync_on_exit: true,
        }
    }
}

/// What is written on first run.
///
/// A template rather than a serialised [`Config`], so the file the reader opens
/// explains itself. The values here must match [`Config::default`] — the test
/// below is what keeps them from drifting apart.
const TEMPLATE: &str = "\
# vocab

# The Anki deck your cards are written to. Created if it isn't there yet.
deck = \"Vocab\"

# Push everything that changed when you leave. `/quit now` skips it either way,
# and a sync never delays quitting: if Anki isn't running, your Words simply
# stay queued for next time.
sync_on_exit = true

# Nothing secret belongs in this file. The key that writes Notes is read from
# DEEPSEEK_API_KEY in your environment and is never written down here.
";

impl Config {
    /// Read the configuration, writing the default file if there isn't one yet.
    ///
    /// A file that cannot be parsed is an error rather than a silent fall back
    /// to the defaults: the reader has just edited it, and quietly ignoring
    /// what they wrote would be worse than saying so.
    pub fn load_or_create(path: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text)
                .with_context(|| format!("reading your configuration at {}", path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("creating {}", parent.display()))?;
                }
                fs::write(path, TEMPLATE).with_context(|| format!("writing {}", path.display()))?;
                Ok(Self::default())
            }
            Err(error) => Err(error)
                .with_context(|| format!("opening your configuration at {}", path.display())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The file is only useful as documentation if what it says is what the
    /// tool would have done anyway.
    #[test]
    fn the_file_written_on_first_run_says_what_the_defaults_are() {
        let written: Config = toml::from_str(TEMPLATE).expect("parsing the template");
        let default = Config::default();

        assert_eq!(written.deck, default.deck);
        assert_eq!(written.sync_on_exit, default.sync_on_exit);
        assert_eq!(written.deck, DEFAULT_DECK);
    }

    #[test]
    fn no_secret_is_written_into_it() {
        // The only mention of a key is the line explaining that it is not here.
        assert!(TEMPLATE.contains("never written down here"));
        assert!(!TEMPLATE.contains("DEEPSEEK_API_KEY ="));
    }

    #[test]
    fn a_first_run_writes_the_file_and_starts_from_the_defaults() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("nested").join("config.toml");

        let config = Config::load_or_create(&path).expect("first run");

        assert_eq!(config.deck, DEFAULT_DECK);
        assert!(config.sync_on_exit);
        assert!(path.exists(), "the file should be there to be edited");
    }

    #[test]
    fn what_the_reader_writes_is_what_is_read_back() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("config.toml");
        fs::write(&path, "deck = \"Reading\"\nsync_on_exit = false\n").expect("writing");

        let config = Config::load_or_create(&path).expect("reading it back");

        assert_eq!(config.deck, "Reading");
        assert!(!config.sync_on_exit);
    }

    /// Setting one thing shouldn't mean having to state the other.
    #[test]
    fn a_file_that_sets_only_one_setting_keeps_the_default_for_the_rest() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("config.toml");
        fs::write(&path, "deck = \"Reading\"\n").expect("writing");

        let config = Config::load_or_create(&path).expect("reading it back");

        assert_eq!(config.deck, "Reading");
        assert!(config.sync_on_exit);
    }

    #[test]
    fn a_typo_is_pointed_at_rather_than_quietly_ignored() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("config.toml");
        fs::write(&path, "dekc = \"Reading\"\n").expect("writing");

        let error = Config::load_or_create(&path).expect_err("a misspelled setting");

        assert!(
            format!("{error:#}").contains("config.toml"),
            "the reader needs to be told which file: {error:#}"
        );
    }
}
