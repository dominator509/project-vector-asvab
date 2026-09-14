//! Shared text utilities for content retrieval.

/// Minimal English stop-word list used when scoring retrieval relevance.
///
/// Stop words carry no discriminative signal, so including them would let two
/// unrelated passages share a high score purely on common function words.
pub struct StopWords;

impl StopWords {
    const WORDS: &'static [&'static str] = &[
        "a", "an", "and", "are", "as", "at", "be", "but", "by", "for", "from", "has", "have", "he",
        "her", "his", "i", "if", "in", "into", "is", "it", "its", "me", "my", "no", "not", "of",
        "on", "or", "our", "she", "so", "than", "that", "the", "their", "them", "then", "there",
        "these", "they", "this", "to", "was", "we", "were", "what", "when", "which", "who", "will",
        "with", "you", "your",
    ];

    pub fn is_stop_word(term: &str) -> bool {
        Self::WORDS.contains(&term)
    }
}
