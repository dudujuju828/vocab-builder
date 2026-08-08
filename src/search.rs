//! Live fuzzy search over everything captured.
//!
//! Matches three fields — Word spellings, Sighting sentences, and Book names —
//! and groups results by Word, so a Word met several times appears once.

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::store::CorpusEntry;

/// Which field a result matched on, so a sentence hit is never mistaken for a
/// Word hit. The declaration order is the ranking: Word matches come first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchField {
    Word,
    Sentence,
    Book,
}

impl MatchField {
    /// The tag shown beside a result.
    pub fn label(self) -> &'static str {
        match self {
            Self::Word => "word",
            Self::Sentence => "sentence",
            Self::Book => "book",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub word_id: i64,
    pub spelling: String,
    pub field: MatchField,
    pub score: u32,
    /// The sentence or Book name that matched. `None` when the Word itself did.
    pub context: Option<String>,
}

pub struct Search {
    matcher: Matcher,
}

impl Default for Search {
    fn default() -> Self {
        Self::new()
    }
}

impl Search {
    pub fn new() -> Self {
        Self {
            matcher: Matcher::new(Config::DEFAULT),
        }
    }

    /// Rank `corpus` against `query`, best first.
    ///
    /// A Word is ranked by its strongest band: if the query matches both the
    /// spelling and a sentence, it is a Word hit. Within a band the matcher's
    /// own score orders results, and ties break on spelling so the order is
    /// stable from one keystroke to the next.
    pub fn run(&mut self, corpus: &[CorpusEntry], query: &str) -> Vec<SearchResult> {
        let query = query.trim();
        if query.is_empty() {
            return Vec::new();
        }
        let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);

        let mut results: Vec<SearchResult> = corpus
            .iter()
            .filter_map(|entry| self.best_match(entry, &pattern))
            .collect();

        results.sort_by(|left, right| {
            left.field
                .cmp(&right.field)
                .then(right.score.cmp(&left.score))
                .then_with(|| left.spelling.cmp(&right.spelling))
        });
        results
    }

    fn best_match(&mut self, entry: &CorpusEntry, pattern: &Pattern) -> Option<SearchResult> {
        let result = |field, score, context| SearchResult {
            word_id: entry.word_id,
            spelling: entry.spelling.clone(),
            field,
            score,
            context,
        };

        if let Some(score) = self.score(&entry.spelling, pattern) {
            return Some(result(MatchField::Word, score, None));
        }
        if let Some((score, sentence)) = self.best_of(&entry.sentences, pattern) {
            return Some(result(MatchField::Sentence, score, Some(sentence)));
        }
        let (score, book) = self.best_of(&entry.books, pattern)?;
        Some(result(MatchField::Book, score, Some(book)))
    }

    fn best_of(&mut self, haystacks: &[String], pattern: &Pattern) -> Option<(u32, String)> {
        haystacks
            .iter()
            .filter_map(|haystack| Some((self.score(haystack, pattern)?, haystack.clone())))
            .max_by_key(|(score, _)| *score)
    }

    fn score(&mut self, haystack: &str, pattern: &Pattern) -> Option<u32> {
        let mut buffer = Vec::new();
        pattern.score(Utf32Str::new(haystack, &mut buffer), &mut self.matcher)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(word_id: i64, spelling: &str, sentences: &[&str], books: &[&str]) -> CorpusEntry {
        CorpusEntry {
            word_id,
            spelling: spelling.to_string(),
            sentences: sentences.iter().map(|s| s.to_string()).collect(),
            books: books.iter().map(|b| b.to_string()).collect(),
        }
    }

    #[test]
    fn word_matches_outrank_sentence_and_book_matches() {
        let corpus = vec![
            entry(1, "unrelated", &["a sentence about whales"], &["Whales"]),
            entry(2, "whale", &["nothing relevant"], &["Some Book"]),
        ];

        let results = Search::new().run(&corpus, "whale");

        assert_eq!(results[0].spelling, "whale");
        assert_eq!(results[0].field, MatchField::Word);
        assert_eq!(results[1].field, MatchField::Sentence);
    }

    #[test]
    fn a_word_matched_several_ways_appears_once() {
        let corpus = vec![entry(
            1,
            "cetacean",
            &["a cetacean of note", "another cetacean"],
            &["Cetacean Weekly"],
        )];

        let results = Search::new().run(&corpus, "cetacean");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].field, MatchField::Word);
    }

    #[test]
    fn fuzzy_matching_survives_a_half_remembered_spelling() {
        let corpus = vec![entry(1, "sesquipedalian", &[], &[])];

        assert_eq!(Search::new().run(&corpus, "sesqpdln").len(), 1);
    }

    #[test]
    fn an_empty_query_matches_nothing() {
        let corpus = vec![entry(1, "whale", &[], &[])];

        assert!(Search::new().run(&corpus, "   ").is_empty());
    }

    #[test]
    fn book_matches_pull_up_everything_from_one_book() {
        let corpus = vec![
            entry(1, "alpha", &["one"], &["Moby-Dick"]),
            entry(2, "beta", &["two"], &["Moby-Dick"]),
        ];

        let results = Search::new().run(&corpus, "Moby");

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.field == MatchField::Book));
        assert_eq!(results[0].context.as_deref(), Some("Moby-Dick"));
    }
}
