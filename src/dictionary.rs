//! The bundled offline dictionary, per ADR 0001.
//!
//! The WordNet-derived database is compiled into the binary and unpacked beside
//! the user database on first run, so there is no download and no setup step
//! between installing the tool and using it. It is read-only and entirely
//! separate from user data, so a new build can replace it wholesale.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};

use crate::domain::Sense;

/// Built by `tools/build_dictionary.py` from a Princeton WordNet release.
const BUNDLED: &[u8] = include_bytes!("../assets/wordnet.db");

pub struct Dictionary {
    connection: Connection,
}

impl Dictionary {
    /// Unpack the bundled database into `directory` if it is not already there,
    /// then open it read-only.
    ///
    /// The unpacked copy is rewritten whenever its length differs from the
    /// bundled one, which is what makes replacing the dictionary in a new
    /// release a no-op for the user.
    pub fn unpack_into(directory: &Path) -> Result<Self> {
        fs::create_dir_all(directory)
            .with_context(|| format!("creating {}", directory.display()))?;
        let path = directory.join("wordnet.db");

        let unpacked = fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
        if unpacked != BUNDLED.len() as u64 {
            fs::write(&path, BUNDLED)
                .with_context(|| format!("unpacking the dictionary to {}", path.display()))?;
        }

        Self::open(&path)
    }

    pub fn open(path: &Path) -> Result<Self> {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .with_context(|| format!("opening the dictionary at {}", path.display()))?;
        Ok(Self { connection })
    }

    /// Every sense of `spelling`, most common first.
    ///
    /// Lookup is by exact spelling, case-insensitively. A miss is a normal
    /// outcome — an empty `Vec`, not an error — because a Word absent from
    /// WordNet is still worth capturing.
    pub fn look_up(&self, spelling: &str) -> Result<Vec<Sense>> {
        let mut statement = self.connection.prepare_cached(
            "SELECT synsets.pos, synsets.definition
               FROM senses
               JOIN synsets ON synsets.id = senses.synset_id
              WHERE senses.word = ?1
              ORDER BY senses.sense_num",
        )?;

        let senses = statement
            .query_map([spelling.trim().to_lowercase()], |row| {
                Ok(Sense {
                    part_of_speech: row.get(0)?,
                    definition: row.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(senses)
    }
}
