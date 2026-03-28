/// Romaji to Kana conversion state machine
///
/// Converts ASCII key input into Hiragana.
/// Handles: standard romaji, double consonants (っ), "n" ambiguity (ん),
/// compound sounds (しゃ, ちゅ, etc.), small kana (ぁ, っ via x/l prefix).

mod romaji_table;

use romaji_table::ROMAJI_TABLE;

pub struct RomajiConverter {
    buffer: String,
    output: String,
    /// When true, the 'n' in buffer was produced by double-n;
    /// flush should NOT emit another ん
    n_from_double: bool,
}

impl RomajiConverter {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            output: String::new(),
            n_from_double: false,
        }
    }

    /// Process a single key input and return converted kana (if any)
    pub fn process_key(&mut self, key: char) -> Option<String> {
        // If the pending 'n' from double-n gets a new char, it's a real 'n' start
        if self.n_from_double && key != 'n' {
            self.n_from_double = false;
        }
        self.buffer.push(key);
        self.try_convert()
    }

    fn try_convert(&mut self) -> Option<String> {
        // 1. Exact match in romaji table
        if let Some(kana) = exact_lookup(&self.buffer) {
            // Special case: "nn" followed by a vowel/y should be ん + n(vowel)
            // But "nn" at this point is a complete match, so it's fine —
            // the issue is handled by "n" + consonant logic below.
            self.buffer.clear();
            self.output.push_str(kana);
            return Some(kana.to_string());
        }

        // 2. Buffer is a prefix of some table entry — keep buffering
        if has_prefix_match(&self.buffer) {
            return None;
        }

        // 3. Double consonant → っ (or ん for nn) + keep rest
        //    Must check before n-rule so "nn" is handled as double-n
        if self.buffer.len() >= 2 {
            let bytes = self.buffer.as_bytes();
            if bytes[0] == bytes[1] && is_doubling_consonant(bytes[0] as char) {
                let geminate = if bytes[0] == b'n' { "ん" } else { "っ" };
                let is_nn = bytes[0] == b'n';
                let rest = self.buffer[1..].to_string();
                self.buffer = rest;
                self.output.push_str(geminate);
                if is_nn {
                    self.n_from_double = true;
                }

                if let Some(more_kana) = self.try_convert() {
                    return Some(format!("{}{}", geminate, more_kana));
                }
                return Some(geminate.to_string());
            }
        }

        // 4. "n" + non-matching char → ん + restart with remaining
        if self.buffer.len() >= 2 && self.buffer.starts_with('n') {
            let rest = self.buffer[1..].to_string();
            self.buffer = rest;
            self.output.push('ん');

            // Try to convert the remaining buffer recursively
            if let Some(more_kana) = self.try_convert() {
                return Some(format!("ん{}", more_kana));
            }
            return Some("ん".to_string());
        }

        // 5. Non-alpha passthrough (punctuation like '-')
        if self.buffer.len() == 1 {
            let ch = self.buffer.chars().next().unwrap();
            if !ch.is_ascii_alphabetic() {
                if let Some(kana) = exact_lookup(&self.buffer) {
                    self.buffer.clear();
                    self.output.push_str(kana);
                    return Some(kana.to_string());
                }
            }
        }

        // 6. No match — discard buffer
        self.buffer.clear();
        None
    }

    /// Flush remaining buffer (call on space/enter/commit)
    pub fn flush(&mut self) -> Option<String> {
        if self.buffer == "n" {
            if self.n_from_double {
                // "nn" already emitted ん; discard the pending n
                self.buffer.clear();
                self.n_from_double = false;
                return None;
            }
            self.buffer.clear();
            self.output.push('ん');
            return Some("ん".to_string());
        }
        if !self.buffer.is_empty() {
            self.buffer.clear();
        }
        self.n_from_double = false;
        None
    }

    /// Reset the conversion buffer and output
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.output.clear();
        self.n_from_double = false;
    }

    /// Get current romaji buffer (incomplete input)
    pub fn buffer(&self) -> &str {
        &self.buffer
    }

    /// Get accumulated kana output
    pub fn output(&self) -> &str {
        &self.output
    }
}

fn exact_lookup(s: &str) -> Option<&'static str> {
    ROMAJI_TABLE
        .binary_search_by_key(&s, |&(romaji, _)| romaji)
        .ok()
        .map(|i| ROMAJI_TABLE[i].1)
}

fn has_prefix_match(s: &str) -> bool {
    let pos = ROMAJI_TABLE.partition_point(|&(romaji, _)| romaji < s);
    if pos < ROMAJI_TABLE.len() && ROMAJI_TABLE[pos].0.starts_with(s) {
        return true;
    }
    // Also check the entry before (for cases where s falls between entries)
    if pos > 0 && ROMAJI_TABLE[pos - 1].0.starts_with(s) {
        return true;
    }
    false
}

fn is_doubling_consonant(c: char) -> bool {
    matches!(
        c,
        'b' | 'c' | 'd' | 'f' | 'g' | 'h' | 'j' | 'k' | 'm' | 'n' | 'p' | 'r' | 's' | 't'
            | 'v' | 'w' | 'z'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convert(input: &str) -> String {
        let mut conv = RomajiConverter::new();
        let mut result = String::new();
        for ch in input.chars() {
            if let Some(kana) = conv.process_key(ch) {
                result.push_str(&kana);
            }
        }
        if let Some(kana) = conv.flush() {
            result.push_str(&kana);
        }
        result
    }

    #[test]
    fn vowels() {
        assert_eq!(convert("a"), "あ");
        assert_eq!(convert("i"), "い");
        assert_eq!(convert("u"), "う");
        assert_eq!(convert("e"), "え");
        assert_eq!(convert("o"), "お");
    }

    #[test]
    fn basic_consonant_vowel() {
        assert_eq!(convert("ka"), "か");
        assert_eq!(convert("ki"), "き");
        assert_eq!(convert("ku"), "く");
        assert_eq!(convert("ke"), "け");
        assert_eq!(convert("ko"), "こ");
        assert_eq!(convert("sa"), "さ");
        assert_eq!(convert("ta"), "た");
        assert_eq!(convert("na"), "な");
        assert_eq!(convert("ha"), "は");
        assert_eq!(convert("ma"), "ま");
        assert_eq!(convert("ya"), "や");
        assert_eq!(convert("ra"), "ら");
        assert_eq!(convert("wa"), "わ");
    }

    #[test]
    fn shi_chi_tsu_fu() {
        assert_eq!(convert("shi"), "し");
        assert_eq!(convert("si"), "し");
        assert_eq!(convert("chi"), "ち");
        assert_eq!(convert("ti"), "ち");
        assert_eq!(convert("tsu"), "つ");
        assert_eq!(convert("tu"), "つ");
        assert_eq!(convert("fu"), "ふ");
        assert_eq!(convert("hu"), "ふ");
    }

    #[test]
    fn compound_sounds() {
        assert_eq!(convert("sha"), "しゃ");
        assert_eq!(convert("shu"), "しゅ");
        assert_eq!(convert("sho"), "しょ");
        assert_eq!(convert("cha"), "ちゃ");
        assert_eq!(convert("chu"), "ちゅ");
        assert_eq!(convert("cho"), "ちょ");
        assert_eq!(convert("ja"), "じゃ");
        assert_eq!(convert("ju"), "じゅ");
        assert_eq!(convert("jo"), "じょ");
        assert_eq!(convert("kya"), "きゃ");
        assert_eq!(convert("kyu"), "きゅ");
        assert_eq!(convert("kyo"), "きょ");
        assert_eq!(convert("nya"), "にゃ");
        assert_eq!(convert("nyu"), "にゅ");
        assert_eq!(convert("nyo"), "にょ");
    }

    #[test]
    fn double_consonant() {
        assert_eq!(convert("kka"), "っか");
        assert_eq!(convert("tta"), "った");
        assert_eq!(convert("ssa"), "っさ");
        assert_eq!(convert("ppa"), "っぱ");
        assert_eq!(convert("cchi"), "っち");
    }

    #[test]
    fn tchi() {
        assert_eq!(convert("tchi"), "っち");
    }

    #[test]
    fn n_handling() {
        assert_eq!(convert("na"), "な");
        assert_eq!(convert("ni"), "に");
        assert_eq!(convert("nn"), "ん");  // n+n → ん (via n-rule, second n flushed)
        assert_eq!(convert("n'"), "ん");
        // n before consonant
        assert_eq!(convert("nk"), "ん");
        assert_eq!(convert("kanji"), "かんじ");
        assert_eq!(convert("shinbun"), "しんぶん");
    }

    #[test]
    fn n_flush() {
        let mut conv = RomajiConverter::new();
        conv.process_key('n');
        assert_eq!(conv.flush(), Some("ん".to_string()));
    }

    #[test]
    fn small_kana() {
        assert_eq!(convert("xtu"), "っ");
        assert_eq!(convert("ltu"), "っ");
        assert_eq!(convert("xa"), "ぁ");
        assert_eq!(convert("la"), "ぁ");
    }

    #[test]
    fn long_vowel_mark() {
        assert_eq!(convert("-"), "ー");
    }

    #[test]
    fn word_toukyou() {
        assert_eq!(convert("toukyou"), "とうきょう");
    }

    #[test]
    fn word_gakkou() {
        assert_eq!(convert("gakkou"), "がっこう");
    }

    #[test]
    fn word_konnichiwa() {
        assert_eq!(convert("konnichiwa"), "こんにちわ");
    }

    #[test]
    fn word_nihongo() {
        assert_eq!(convert("nihongo"), "にほんご");
    }

    #[test]
    fn word_senshuu() {
        assert_eq!(convert("senshuu"), "せんしゅう");
    }

    #[test]
    fn output_accumulates() {
        let mut conv = RomajiConverter::new();
        for ch in "toukyou".chars() {
            conv.process_key(ch);
        }
        assert_eq!(conv.output(), "とうきょう");
    }

    #[test]
    fn wo_particle() {
        assert_eq!(convert("wo"), "を");
    }
}
