/// JaIM Conversion Engine
///
/// Orchestrates the 3-stage conversion pipeline:
/// 1. Dictionary lookup (fast, < 1ms)
/// 2. Grammar scoring (fast, < 1ms)
/// 3. LLM reranking (20-40ms, background pre-computation)
///
/// Uses multi-threaded architecture with worker threads for:
/// - Dictionary lookup (updated on each keystroke)
/// - Grammar scoring (filters candidates as they arrive)
/// - LLM KV cache warming (pre-computes context during typing)

use crate::core::{dictionary::Dictionary, grammar::GrammarEngine, llm::LlmEngine, romaji::RomajiConverter};

pub struct ConversionEngine {
    romaji: RomajiConverter,
    dictionary: Dictionary,
    grammar: GrammarEngine,
    llm: LlmEngine,
}

impl ConversionEngine {
    pub fn new() -> Self {
        Self {
            romaji: RomajiConverter::new(),
            dictionary: Dictionary::new(),
            grammar: GrammarEngine::new(),
            llm: LlmEngine::new(),
        }
    }

    /// Process a key event from the IME framework
    pub fn process_key(&mut self, key: char) -> EngineAction {
        if let Some(_kana) = self.romaji.process_key(key) {
            // TODO: Trigger background dictionary lookup
            EngineAction::UpdatePreedit
        } else {
            EngineAction::Buffering
        }
    }

    /// Trigger conversion (space key pressed)
    pub fn convert(&self) -> Vec<ConversionCandidate> {
        // TODO: Pipeline: dictionary → grammar filter → LLM rerank
        Vec::new()
    }

    /// Commit the selected candidate
    pub fn commit(&mut self, _candidate: &str) -> String {
        self.romaji.reset();
        // TODO: Return committed text
        String::new()
    }
}

#[derive(Debug)]
pub enum EngineAction {
    /// Key was buffered (incomplete romaji)
    Buffering,
    /// Preedit text should be updated
    UpdatePreedit,
    /// Candidates are ready to display
    ShowCandidates,
    /// Text was committed
    Commit(String),
}

#[derive(Debug, Clone)]
pub struct ConversionCandidate {
    /// Converted text
    pub text: String,
    /// Combined score (grammar + LLM)
    pub score: f64,
}
