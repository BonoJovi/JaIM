/// Dictionary-based Kana to Kanji conversion
///
/// Fast lookup (< 1ms) handling 70-80% of common conversions.
/// Uses trie-based data structure for efficient prefix matching.
/// Includes word segmentation via dynamic programming (minimum-cost path).

mod builtin_dict;
mod trie;

use serde::{Deserialize, Serialize};
use trie::Trie;

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

/// A segment produced by word segmentation
#[derive(Debug, Clone)]
pub struct Segment {
    /// Reading (kana substring)
    pub reading: String,
    /// Start position (char offset)
    pub start: usize,
    /// Length in characters
    pub len: usize,
    /// Candidate entries for this segment
    pub candidates: Vec<DictionaryEntry>,
}

pub struct Dictionary {
    entries: Vec<DictionaryEntry>,
    trie: Trie,
}

impl Dictionary {
    /// Create a new dictionary pre-loaded with the built-in word set.
    pub fn new() -> Self {
        let mut dict = Self {
            entries: Vec::new(),
            trie: Trie::new(),
        };
        dict.load_builtin();
        dict
    }

    /// Add a single entry.
    pub fn add_entry(&mut self, entry: DictionaryEntry) {
        let idx = self.entries.len();
        let reading = entry.reading.clone();
        let frequency = entry.frequency;
        self.entries.push(entry);
        self.trie.insert(&reading, idx, frequency);
    }

    /// Exact lookup: return all candidates for a reading, sorted by frequency (descending).
    pub fn lookup(&self, reading: &str) -> Vec<&DictionaryEntry> {
        let indices = self.trie.exact_lookup(reading);
        let mut entries: Vec<&DictionaryEntry> = indices.iter().map(|&i| &self.entries[i]).collect();
        entries.sort_by(|a, b| b.frequency.cmp(&a.frequency));
        entries
    }

    /// Common prefix search: find all dictionary words that are prefixes of `input`.
    /// Returns Vec of (char_length, entries) sorted by prefix length.
    pub fn common_prefix_search(&self, input: &str) -> Vec<(usize, Vec<&DictionaryEntry>)> {
        self.trie
            .common_prefix_search(input)
            .into_iter()
            .map(|(len, indices)| {
                let mut entries: Vec<&DictionaryEntry> =
                    indices.iter().map(|&i| &self.entries[i]).collect();
                entries.sort_by(|a, b| b.frequency.cmp(&a.frequency));
                (len, entries)
            })
            .collect()
    }

    /// Prefix lookup: return candidates for all readings starting with `prefix`.
    pub fn prefix_lookup(&self, prefix: &str) -> Vec<&DictionaryEntry> {
        let indices = self.trie.prefix_lookup(prefix);
        let mut entries: Vec<&DictionaryEntry> = indices.iter().map(|&i| &self.entries[i]).collect();
        entries.sort_by(|a, b| b.frequency.cmp(&a.frequency));
        entries
    }

    /// Segment a kana string into words using minimum-cost dynamic programming.
    /// Returns the best segmentation as a Vec of Segments.
    pub fn segment(&self, input: &str) -> Vec<Segment> {
        let chars: Vec<char> = input.chars().collect();
        let n = chars.len();
        if n == 0 {
            return Vec::new();
        }

        // DP: best_cost[i] = minimum cost to segment chars[0..i]
        const INF: f64 = 1e18;
        let mut best_cost = vec![INF; n + 1];
        let mut back: Vec<Option<usize>> = vec![None; n + 1]; // back[i] = start of last segment ending at i
        best_cost[0] = 0.0;

        for i in 0..n {
            if best_cost[i] >= INF {
                continue;
            }

            let remaining: String = chars[i..].iter().collect();
            let prefixes = self.trie.common_prefix_search(&remaining);

            for (len, _indices) in &prefixes {
                // Cost: prefer longer matches and higher frequency
                let best_freq = _indices
                    .iter()
                    .map(|&idx| self.entries[idx].frequency)
                    .max()
                    .unwrap_or(1);
                let cost = segment_cost(*len, best_freq);
                let total = best_cost[i] + cost;
                if total < best_cost[i + len] {
                    best_cost[i + len] = total;
                    back[i + len] = Some(i);
                }
            }

            // Fallback: single character as unknown word (high cost)
            let unknown_cost = best_cost[i] + 20.0;
            if unknown_cost < best_cost[i + 1] {
                best_cost[i + 1] = unknown_cost;
                back[i + 1] = Some(i);
            }
        }

        // Reconstruct path
        let mut boundaries = Vec::new();
        let mut pos = n;
        while pos > 0 {
            if let Some(start) = back[pos] {
                boundaries.push((start, pos));
                pos = start;
            } else {
                // Should not happen if fallback works, but handle gracefully
                boundaries.push((pos - 1, pos));
                pos -= 1;
            }
        }
        boundaries.reverse();

        // Build segments
        boundaries
            .into_iter()
            .map(|(start, end)| {
                let reading: String = chars[start..end].iter().collect();
                let candidates: Vec<DictionaryEntry> = self
                    .lookup(&reading)
                    .into_iter()
                    .cloned()
                    .collect();
                Segment {
                    reading,
                    start,
                    len: end - start,
                    candidates,
                }
            })
            .collect()
    }

    /// Total number of entries in the dictionary.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    fn load_builtin(&mut self) {
        for &(reading, surface, pos, frequency) in builtin_dict::BUILTIN_ENTRIES {
            self.add_entry(DictionaryEntry {
                reading: reading.to_string(),
                surface: surface.to_string(),
                pos,
                frequency,
            });
        }
    }
}

/// Cost function for segmentation DP.
/// Lower cost = better. Prefers longer matches and higher frequency.
fn segment_cost(char_len: usize, frequency: u32) -> f64 {
    let freq_cost = -(frequency as f64).ln();
    let length_bonus = (char_len as f64) * -2.0; // longer words get lower cost
    freq_cost + length_bonus
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_dictionary_loads() {
        let dict = Dictionary::new();
        assert!(dict.len() > 100);
    }

    #[test]
    fn lookup_common_words() {
        let dict = Dictionary::new();

        let results = dict.lookup("きょう");
        assert!(!results.is_empty());
        assert_eq!(results[0].surface, "今日"); // highest frequency
    }

    #[test]
    fn lookup_particles() {
        let dict = Dictionary::new();

        let results = dict.lookup("は");
        assert!(!results.is_empty());
        assert_eq!(results[0].pos, PartOfSpeech::Particle);
    }

    #[test]
    fn lookup_miss() {
        let dict = Dictionary::new();
        let results = dict.lookup("zzz");
        assert!(results.is_empty());
    }

    #[test]
    fn lookup_multiple_candidates() {
        let dict = Dictionary::new();

        // あめ should have 雨 and 飴
        let results = dict.lookup("あめ");
        assert!(results.len() >= 2);
        // 雨 (8400) should come before 飴 (6000)
        assert_eq!(results[0].surface, "雨");
        assert_eq!(results[1].surface, "飴");
    }

    #[test]
    fn prefix_lookup_basic() {
        let dict = Dictionary::new();

        let results = dict.prefix_lookup("きょう");
        // Should include きょう (今日, 京) and きょうと (京都) and きょねん etc.
        let surfaces: Vec<&str> = results.iter().map(|e| e.surface.as_str()).collect();
        assert!(surfaces.contains(&"今日"));
        assert!(surfaces.contains(&"京都"));
    }

    #[test]
    fn common_prefix_search_basic() {
        let dict = Dictionary::new();

        let results = dict.common_prefix_search("きょうは");
        // Should find entries for き (木/気) and きょう (今日/京)
        assert!(results.len() >= 2);
    }

    #[test]
    fn segmentation_basic() {
        let dict = Dictionary::new();

        let segments = dict.segment("きょうはいいてんきです");
        let words: Vec<&str> = segments.iter().map(|s| s.reading.as_str()).collect();

        assert_eq!(words, vec!["きょう", "は", "いい", "てんき", "です"]);
    }

    #[test]
    fn segmentation_with_candidates() {
        let dict = Dictionary::new();

        let segments = dict.segment("きょうはいいてんきです");
        // First segment should have 今日 as top candidate
        assert!(!segments[0].candidates.is_empty());
        assert_eq!(segments[0].candidates[0].surface, "今日");
    }

    #[test]
    fn segmentation_unknown_word() {
        let dict = Dictionary::new();

        // ぱぴぷ is not in the dictionary — should be single-char segments
        let segments = dict.segment("ぱぴぷ");
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].reading, "ぱ");
    }

    #[test]
    fn add_entry_runtime() {
        let mut dict = Dictionary::new();
        let count_before = dict.len();

        dict.add_entry(DictionaryEntry {
            reading: "てすと".to_string(),
            surface: "テスト".to_string(),
            pos: PartOfSpeech::Noun,
            frequency: 8000,
        });

        assert_eq!(dict.len(), count_before + 1);
        let results = dict.lookup("てすと");
        assert_eq!(results[0].surface, "テスト");
    }

    #[test]
    fn segmentation_toukyou() {
        let dict = Dictionary::new();

        let segments = dict.segment("とうきょうにいく");
        let words: Vec<&str> = segments.iter().map(|s| s.reading.as_str()).collect();
        assert_eq!(words, vec!["とうきょう", "に", "いく"]);
    }
}
