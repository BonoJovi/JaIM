/// JaIM Conversion Engine
///
/// Orchestrates the 3-stage conversion pipeline:
/// 1. Dictionary lookup + segmentation (fast, < 1ms)
/// 2. Grammar scoring (fast, < 1ms)
/// 3. LLM reranking (20-40ms, background pre-computation)
///
/// Flow: keystroke → romaji → kana → dictionary segment → grammar score
///       → LLM rerank → candidate list → user selects → commit

use crate::core::{
    dictionary::{Dictionary, Segment},
    grammar::{GrammarEngine, GrammarToken},
    llm::LlmEngine,
    romaji::RomajiConverter,
};

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

    /// Process a key event from the IME framework.
    /// Returns the appropriate action for the UI layer.
    pub fn process_key(&mut self, key: char) -> EngineAction {
        if let Some(_kana) = self.romaji.process_key(key) {
            EngineAction::UpdatePreedit(self.preedit())
        } else {
            EngineAction::Buffering(self.preedit())
        }
    }

    /// Get the current preedit string (kana output + pending romaji buffer).
    pub fn preedit(&self) -> String {
        let mut preedit = self.romaji.output().to_string();
        let buf = self.romaji.buffer();
        if !buf.is_empty() {
            preedit.push_str(buf);
        }
        preedit
    }

    /// Trigger conversion (space key pressed).
    /// Runs the full 3-stage pipeline: dictionary → grammar → LLM.
    pub fn convert(&mut self) -> Vec<ConversionCandidate> {
        // Flush any remaining romaji (e.g., trailing 'n' → ん)
        self.romaji.flush();
        let kana = self.romaji.output().to_string();
        if kana.is_empty() {
            return Vec::new();
        }

        // Stage 1: Dictionary segmentation
        let segments = self.dictionary.segment(&kana);
        if segments.is_empty() {
            return Vec::new();
        }

        // Generate candidate sentences from segment combinations
        let candidates = self.build_candidates(&segments);

        // Stage 2 & 3: Score each candidate (grammar + LLM)
        let mut scored: Vec<ConversionCandidate> = candidates
            .into_iter()
            .map(|text| {
                let grammar_tokens = self.tokens_for_grammar(&text, &segments);
                let grammar_result = self.grammar.score(&grammar_tokens);
                let llm_score = self.llm.score_candidate(&text);

                // Combined score: grammar (40%) + LLM (60%)
                let combined = grammar_result.score * 0.4 + llm_score * 0.6;

                ConversionCandidate {
                    text,
                    grammar_score: grammar_result.score,
                    llm_score,
                    score: combined,
                }
            })
            .collect();

        // Sort by combined score (highest first)
        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scored
    }

    /// Commit the selected candidate and update context.
    pub fn commit(&mut self, candidate: &str) -> String {
        self.llm.update_context(candidate);
        self.romaji.reset();
        candidate.to_string()
    }

    /// Reset the engine state (e.g., on focus change).
    pub fn reset(&mut self) {
        self.romaji.reset();
    }

    /// Build candidate sentences from segmented words.
    /// For each segment, pick the top candidates and combine.
    fn build_candidates(&self, segments: &[Segment]) -> Vec<String> {
        // Start with the top candidate for each segment (best conversion)
        let mut results = Vec::new();

        // Best candidate: top surface for each segment
        let best: String = segments
            .iter()
            .map(|seg| {
                seg.candidates
                    .first()
                    .map(|c| c.surface.as_str())
                    .unwrap_or(&seg.reading)
            })
            .collect();
        results.push(best);

        // Generate alternatives by swapping one segment at a time
        for (i, seg) in segments.iter().enumerate() {
            for candidate in seg.candidates.iter().skip(1).take(3) {
                let alt: String = segments
                    .iter()
                    .enumerate()
                    .map(|(j, s)| {
                        if j == i {
                            candidate.surface.as_str()
                        } else {
                            s.candidates
                                .first()
                                .map(|c| c.surface.as_str())
                                .unwrap_or(&s.reading)
                        }
                    })
                    .collect();
                if !results.contains(&alt) {
                    results.push(alt);
                }
            }
        }

        // Also include the raw kana as a candidate
        let raw_kana: String = segments.iter().map(|s| s.reading.as_str()).collect();
        if !results.contains(&raw_kana) {
            results.push(raw_kana);
        }

        results
    }

    /// Create grammar tokens from a candidate text and its segments.
    fn tokens_for_grammar(&self, _text: &str, segments: &[Segment]) -> Vec<GrammarToken> {
        segments
            .iter()
            .map(|seg| {
                let pos = seg
                    .candidates
                    .first()
                    .map(|c| c.pos)
                    .unwrap_or(crate::core::dictionary::PartOfSpeech::Other);
                GrammarToken {
                    surface: seg
                        .candidates
                        .first()
                        .map(|c| c.surface.clone())
                        .unwrap_or_else(|| seg.reading.clone()),
                    pos,
                }
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub enum EngineAction {
    /// Key was buffered (incomplete romaji). Contains current preedit.
    Buffering(String),
    /// Preedit text was updated (kana produced). Contains current preedit.
    UpdatePreedit(String),
    /// Candidates are ready to display.
    ShowCandidates(Vec<ConversionCandidate>),
    /// Text was committed.
    Commit(String),
}

#[derive(Debug, Clone)]
pub struct ConversionCandidate {
    /// Converted text (kanji/mixed)
    pub text: String,
    /// Grammar score (0.0–1.0)
    pub grammar_score: f64,
    /// LLM score (0.0–1.0)
    pub llm_score: f64,
    /// Combined score (grammar * 0.4 + LLM * 0.6)
    pub score: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_key_buffering() {
        let mut engine = ConversionEngine::new();
        match engine.process_key('k') {
            EngineAction::Buffering(preedit) => assert_eq!(preedit, "k"),
            _ => panic!("expected Buffering"),
        }
    }

    #[test]
    fn process_key_produces_kana() {
        let mut engine = ConversionEngine::new();
        engine.process_key('k');
        match engine.process_key('a') {
            EngineAction::UpdatePreedit(preedit) => assert_eq!(preedit, "か"),
            _ => panic!("expected UpdatePreedit"),
        }
    }

    #[test]
    fn preedit_shows_buffer() {
        let mut engine = ConversionEngine::new();
        engine.process_key('k');
        assert_eq!(engine.preedit(), "k");
        engine.process_key('a');
        assert_eq!(engine.preedit(), "か");
        engine.process_key('n');
        assert_eq!(engine.preedit(), "かn");
    }

    #[test]
    fn convert_basic() {
        let mut engine = ConversionEngine::new();
        // Type "kyou" → きょう
        for ch in "kyou".chars() {
            engine.process_key(ch);
        }
        let candidates = engine.convert();
        assert!(!candidates.is_empty());
        // Top candidate should be 今日 (highest frequency + kanji bonus)
        assert_eq!(candidates[0].text, "今日");
    }

    #[test]
    fn convert_sentence() {
        let mut engine = ConversionEngine::new();
        // Type "kyouhaiitenki" → きょうはいいてんき
        for ch in "kyouhaiitenki".chars() {
            engine.process_key(ch);
        }
        let candidates = engine.convert();
        assert!(!candidates.is_empty());
        // Best candidate should contain 今日 and 天気
        assert!(candidates[0].text.contains("今日"));
        assert!(candidates[0].text.contains("天気"));
    }

    #[test]
    fn convert_empty() {
        let mut engine = ConversionEngine::new();
        let candidates = engine.convert();
        assert!(candidates.is_empty());
    }

    #[test]
    fn commit_resets_state() {
        let mut engine = ConversionEngine::new();
        for ch in "kyou".chars() {
            engine.process_key(ch);
        }
        let candidates = engine.convert();
        let committed = engine.commit(&candidates[0].text);
        assert_eq!(committed, candidates[0].text);
        assert_eq!(engine.preedit(), "");
    }

    #[test]
    fn candidates_have_scores() {
        let mut engine = ConversionEngine::new();
        for ch in "kyou".chars() {
            engine.process_key(ch);
        }
        let candidates = engine.convert();
        for c in &candidates {
            assert!(c.score >= 0.0);
            assert!(c.score <= 1.0);
            assert!(c.grammar_score >= 0.0);
            assert!(c.llm_score >= 0.0);
        }
    }

    #[test]
    fn kanji_ranked_above_kana() {
        let mut engine = ConversionEngine::new();
        for ch in "kyou".chars() {
            engine.process_key(ch);
        }
        let candidates = engine.convert();
        // Raw kana きょう should be ranked below kanji candidates
        let kana_pos = candidates.iter().position(|c| c.text == "きょう");
        let kanji_pos = candidates.iter().position(|c| c.text == "今日");
        if let (Some(kp), Some(kap)) = (kana_pos, kanji_pos) {
            assert!(kap < kp, "kanji should rank above kana");
        }
    }
}
