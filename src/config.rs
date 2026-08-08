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
    /// What was wrong with the file, if anything — shown once at launch.
    ///
    /// Never read from the file itself; it is what this module has to say
    /// *about* the file.
    #[serde(skip)]
    pub complaint: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            deck: DEFAULT_DECK.to_string(),
            sync_on_exit: true,
            complaint: None,
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
    /// This cannot fail. Nothing about an optional settings file is worth
    /// standing between the reader and capturing a Word — a typo in it, a
    /// directory that can't be written, a file that can't be read: each falls
    /// back to the defaults and says so once at launch. Being told your deck
    /// name was ignored is recoverable; a tool that won't start is not.
    pub fn load_or_create(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(text) => match toml::from_str::<Self>(&text) {
                Ok(config) => config,
                Err(error) => Self::complaining(format!(
                    "Ignoring {} — {}",
                    path.display(),
                    tersely(&error.to_string())
                )),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => match write(path) {
                Ok(()) => Self::default(),
                // The file is a convenience, not a prerequisite. Not being able
                // to leave one behind changes nothing about this run.
                Err(error) => Self::complaining(format!(
                    "Couldn't write {} — {}",
                    path.display(),
                    tersely(&error.to_string())
                )),
            },
            Err(error) => Self::complaining(format!(
                "Ignoring {} — {}",
                path.display(),
                tersely(&error.to_string())
            )),
        }
    }

    fn complaining(complaint: String) -> Self {
        Self {
            complaint: Some(format!("{complaint}. Carrying on with the defaults.")),
            ..Self::default()
        }
    }
}

fn write(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(path, TEMPLATE).with_context(|| format!("writing {}", path.display()))
}

/// The first line of a complaint, for a message line one row tall.
///
/// TOML errors carry a rendered excerpt of the offending line underneath, which
/// is useful in a terminal that scrolls and unreadable in a terminal that
/// doesn't — the file itself is where the reader goes to see the detail.
fn tersely(error: &str) -> &str {
    error.lines().next().unwrap_or(error).trim()
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

        let config = Config::load_or_create(&path);

        assert_eq!(config.deck, DEFAULT_DECK);
        assert!(config.sync_on_exit);
        assert_eq!(config.complaint, None);
        assert!(path.exists(), "the file should be there to be edited");
    }

    #[test]
    fn what_the_reader_writes_is_what_is_read_back() {
        let config = read("deck = \"Reading\"\nsync_on_exit = false\n");

        assert_eq!(config.deck, "Reading");
        assert!(!config.sync_on_exit);
        assert_eq!(config.complaint, None);
    }

    /// Setting one thing shouldn't mean having to state the other.
    #[test]
    fn a_file_that_sets_only_one_setting_keeps_the_default_for_the_rest() {
        let config = read("deck = \"Reading\"\n");

        assert_eq!(config.deck, "Reading");
        assert!(config.sync_on_exit);
    }

    /// The reader has just edited this file, so silently ignoring what they
    /// wrote would leave them wondering why nothing changed.
    #[test]
    fn a_typo_is_pointed_at_rather_than_quietly_ignored() {
        let config = read("dekc = \"Reading\"\n");

        let complaint = config.complaint.expect("a misspelled setting");
        assert!(
            complaint.contains("config.toml"),
            "the reader needs to be told which file: {complaint}"
        );
        assert!(
            complaint.lines().count() == 1,
            "the message line is one row tall: {complaint}"
        );
    }

    /// The one thing a settings file must never do to a tool whose whole point
    /// is that capturing a Word is cheap.
    #[test]
    fn nothing_the_reader_can_write_in_it_stops_the_tool_starting() {
        for contents in [
            "dekc = \"Reading\"",
            "deck = 3",
            "deck = \"unclosed",
            "sync_on_exit = \"yes\"",
            "[[nonsense]]",
            "\u{0}\u{1}not toml at all",
        ] {
            let config = read(contents);

            assert_eq!(config.deck, DEFAULT_DECK, "for {contents:?}");
            assert!(config.sync_on_exit, "for {contents:?}");
            assert!(config.complaint.is_some(), "unremarked: {contents:?}");
        }
    }

    fn read(contents: &str) -> Config {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("config.toml");
        fs::write(&path, contents).expect("writing");
        Config::load_or_create(&path)
    }
}
