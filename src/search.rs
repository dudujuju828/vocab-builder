//! Live fuzzy search over everything captured.
//!
//! Matches three fields — Word spellings, Sighting sentences, and Book names —
//! and groups results by Word, so a Word met several times appears once.

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::store::CorpusWord;

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
    /// The sentence or Book name the query matched. `None` when the
    /// spelling itself did.
    pub excerpt: Option<String>,
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
    pub fn run(&mut self, corpus: &[CorpusWord], query: &str) -> Vec<SearchResult> {
        let query = query.trim();
        if query.is_empty() {
            return Vec::new();
        }
        let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);

        let mut results: Vec<SearchResult> = corpus
            .iter()
            .filter_map(|word| self.best_match(word, &pattern))
            .collect();

        results.sort_by(|left, right| {
            left.field
                .cmp(&right.field)
                .then(right.score.cmp(&left.score))
                .then_with(|| left.spelling.cmp(&right.spelling))
        });
        results
    }

    fn best_match(&mut self, word: &CorpusWord, pattern: &Pattern) -> Option<SearchResult> {
        let result = |field, score, excerpt| SearchResult {
            word_id: word.word_id,
            spelling: word.spelling.clone(),
            field,
            score,
            excerpt,
        };

        if let Some(score) = self.score(&word.spelling, pattern) {
            return Some(result(MatchField::Word, score, None));
        }
        if let Some((score, sentence)) = self.best_of(&word.sentences, pattern) {
            return Some(result(MatchField::Sentence, score, Some(sentence)));
        }
        let (score, book) = self.best_of(&word.books, pattern)?;
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
