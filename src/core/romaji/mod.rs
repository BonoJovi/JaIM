/// Romaji to Kana conversion state machine
///
/// Converts ASCII key input into Hiragana/Katakana.
/// Handles edge cases: "nn" → "ん", "sha" → "しゃ", etc.

pub struct RomajiConverter {
    buffer: String,
}

impl RomajiConverter {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
        }
    }

    /// Process a single key input and return converted kana (if any)
    pub fn process_key(&mut self, key: char) -> Option<String> {
        self.buffer.push(key);
        // TODO: Implement romaji-to-kana state machine
        None
    }

    /// Reset the conversion buffer
    pub fn reset(&mut self) {
        self.buffer.clear();
    }

    /// Get current buffer content
    pub fn buffer(&self) -> &str {
        &self.buffer
    }
}
