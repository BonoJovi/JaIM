/// Dictionary-based Kana to Kanji conversion
///
/// Fast lookup (< 1ms) handling 70-80% of common conversions.
/// Uses trie-based data structure for efficient prefix matching.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictionaryEntry {
    /// Reading in hiragana
    pub reading: String,
    /// Surface form (kanji/mixed)
    pub surface: String,
    /// Part of speech
    pub pos: PartOfSpeech,
    /// Frequency score (higher = more common)
    pub frequency: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PartOfSpeech {
    Noun,
    Verb,
    Adjective,
    Adverb,
    Particle,
    Auxiliary,
    Conjunction,
    Interjection,
    Prefix,
    Suffix,
    Other,
}

pub struct Dictionary {
    // TODO: Trie-based storage
}

impl Dictionary {
    pub fn new() -> Self {
        Self {}
    }

    /// Look up candidates for a given kana reading
    pub fn lookup(&self, _reading: &str) -> Vec<DictionaryEntry> {
        // TODO: Implement trie lookup
        Vec::new()
    }
}
